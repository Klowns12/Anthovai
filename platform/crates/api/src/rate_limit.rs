//! In-memory rate limiting.
//!
//! One counter per instance, which is the honest description: with several API
//! instances behind a load balancer the effective limit is the configured one
//! times the instance count. That is fine for what this is protecting against
//! in P1 — runaway loops and credential stuffing — and it costs no extra
//! infrastructure. A shared Redis counter replaces it when the limits become
//! contractual rather than protective.

use std::collections::HashMap;
use std::sync::Mutex;

use anthovai_core::Clock;
use chrono::{DateTime, Duration, Utc};

/// Verdict for one attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub allowed: bool,
    pub limit: u32,
    pub remaining: u32,
    /// Seconds until the window rolls over.
    pub reset_in_secs: u64,
}

pub struct RateLimiter {
    clock: Clock,
    windows: Mutex<HashMap<String, Window>>,
}

#[derive(Clone, Debug)]
struct Window {
    started_at: DateTime<Utc>,
    hits: u32,
}

impl RateLimiter {
    pub fn new(clock: Clock) -> Self {
        Self {
            clock,
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// Count one attempt against `key`. A fixed window: simple, and its worst
    /// case (twice the limit across a window boundary) does not matter for
    /// either thing this defends against.
    pub fn check(&self, key: &str, limit: u32, window_secs: i64) -> Verdict {
        let now = self.clock.now();
        let window = Duration::seconds(window_secs);
        let mut windows = self.windows.lock().expect("rate limiter mutex poisoned");

        let entry = windows.entry(key.to_owned()).or_insert(Window {
            started_at: now,
            hits: 0,
        });

        if now - entry.started_at >= window {
            entry.started_at = now;
            entry.hits = 0;
        }

        entry.hits += 1;
        let elapsed = (now - entry.started_at).num_seconds().max(0);
        let reset_in_secs = (window_secs - elapsed).max(0) as u64;

        Verdict {
            allowed: entry.hits <= limit,
            limit,
            remaining: limit.saturating_sub(entry.hits),
            reset_in_secs,
        }
    }

    /// Forget a key: called after a successful sign-in, so one person fumbling
    /// their password does not lock out the rest of their window.
    pub fn forget(&self, key: &str) {
        self.windows
            .lock()
            .expect("rate limiter mutex poisoned")
            .remove(key);
    }

    /// Drop windows that rolled over long ago, so a long-running instance does
    /// not accumulate a counter per address it has ever seen.
    pub fn evict_stale(&self, older_than_secs: i64) {
        let cutoff = self.clock.now() - Duration::seconds(older_than_secs);
        self.windows
            .lock()
            .expect("rate limiter mutex poisoned")
            .retain(|_, window| window.started_at > cutoff);
    }

    pub fn len(&self) -> usize {
        self.windows
            .lock()
            .expect("rate limiter mutex poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("tracked_keys", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter() -> (RateLimiter, anthovai_core::time::FixedClock) {
        let start = DateTime::parse_from_rfc3339("2026-09-04T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (clock, hands) = Clock::fixed(start);
        (RateLimiter::new(clock), hands)
    }

    #[test]
    fn allows_up_to_the_limit_then_refuses() {
        let (limiter, _) = limiter();
        for i in 1..=5 {
            let verdict = limiter.check("ip:1.2.3.4", 5, 60);
            assert!(verdict.allowed, "attempt {i} should be allowed");
        }
        assert!(!limiter.check("ip:1.2.3.4", 5, 60).allowed);
    }

    #[test]
    fn reports_what_is_left() {
        let (limiter, _) = limiter();
        assert_eq!(limiter.check("k", 3, 60).remaining, 2);
        assert_eq!(limiter.check("k", 3, 60).remaining, 1);
        assert_eq!(limiter.check("k", 3, 60).remaining, 0);
    }

    #[test]
    fn the_window_rolls_over() {
        let (limiter, hands) = limiter();
        for _ in 0..5 {
            limiter.check("k", 5, 60);
        }
        assert!(!limiter.check("k", 5, 60).allowed);

        hands.advance(Duration::seconds(61));
        assert!(limiter.check("k", 5, 60).allowed);
    }

    #[test]
    fn keys_are_counted_separately() {
        let (limiter, _) = limiter();
        for _ in 0..5 {
            limiter.check("a", 5, 60);
        }
        assert!(!limiter.check("a", 5, 60).allowed);
        assert!(limiter.check("b", 5, 60).allowed);
    }

    #[test]
    fn a_successful_sign_in_clears_the_count() {
        let (limiter, _) = limiter();
        for _ in 0..4 {
            limiter.check("email:owner@abc.ac.th", 5, 900);
        }
        limiter.forget("email:owner@abc.ac.th");
        assert_eq!(limiter.check("email:owner@abc.ac.th", 5, 900).remaining, 4);
    }

    #[test]
    fn reset_counts_down_within_the_window() {
        let (limiter, hands) = limiter();
        assert_eq!(limiter.check("k", 5, 60).reset_in_secs, 60);
        hands.advance(Duration::seconds(20));
        assert_eq!(limiter.check("k", 5, 60).reset_in_secs, 40);
    }

    #[test]
    fn stale_windows_are_evicted() {
        let (limiter, hands) = limiter();
        limiter.check("old", 5, 60);
        hands.advance(Duration::seconds(3_600));
        limiter.check("fresh", 5, 60);

        limiter.evict_stale(600);

        assert_eq!(limiter.len(), 1, "only the fresh window should remain");
    }
}
