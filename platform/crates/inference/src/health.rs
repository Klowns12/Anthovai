//! A circuit breaker per (provider, model).
//!
//! Five failures inside a minute opens the circuit for thirty seconds. While
//! open, the router skips that candidate instead of spending a request to
//! rediscover that it is down.

use std::collections::HashMap;
use std::sync::Mutex;

use anthovai_core::Clock;
use chrono::{DateTime, Duration, Utc};

const FAILURE_THRESHOLD: u32 = 5;
const FAILURE_WINDOW_SECS: i64 = 60;
const OPEN_FOR_SECS: i64 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Default)]
struct Entry {
    failures: u32,
    first_failure_at: Option<DateTime<Utc>>,
    opened_at: Option<DateTime<Utc>>,
}

pub struct HealthTracker {
    clock: Clock,
    entries: Mutex<HashMap<String, Entry>>,
}

impl HealthTracker {
    pub fn new(clock: Clock) -> Self {
        Self {
            clock,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn state(&self, key: &str) -> CircuitState {
        let now = self.clock.now();
        let mut entries = self.entries.lock().expect("health mutex poisoned");
        let entry = entries.entry(key.to_owned()).or_default();
        match entry.opened_at {
            Some(at) if now - at < Duration::seconds(OPEN_FOR_SECS) => CircuitState::Open,
            Some(_) => CircuitState::HalfOpen,
            None => CircuitState::Closed,
        }
    }

    /// True when the router may try this candidate.
    pub fn is_usable(&self, key: &str) -> bool {
        self.state(key) != CircuitState::Open
    }

    pub fn record_success(&self, key: &str) {
        let mut entries = self.entries.lock().expect("health mutex poisoned");
        entries.insert(key.to_owned(), Entry::default());
    }

    pub fn record_failure(&self, key: &str) {
        let now = self.clock.now();
        let mut entries = self.entries.lock().expect("health mutex poisoned");
        let entry = entries.entry(key.to_owned()).or_default();

        // A failure long after the last one starts a fresh window.
        let window_expired = entry
            .first_failure_at
            .is_none_or(|first| now - first > Duration::seconds(FAILURE_WINDOW_SECS));
        if window_expired {
            entry.failures = 0;
            entry.first_failure_at = Some(now);
        }

        entry.failures += 1;
        if entry.failures >= FAILURE_THRESHOLD {
            entry.opened_at = Some(now);
            entry.failures = 0;
            entry.first_failure_at = None;
        }
    }
}

impl std::fmt::Debug for HealthTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HealthTracker")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> (HealthTracker, anthovai_core::time::FixedClock) {
        let start = DateTime::parse_from_rfc3339("2026-09-03T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (clock, handle) = Clock::fixed(start);
        (HealthTracker::new(clock), handle)
    }

    #[test]
    fn starts_closed() {
        let (tracker, _) = tracker();
        assert_eq!(tracker.state("anthropic:medium"), CircuitState::Closed);
        assert!(tracker.is_usable("anthropic:medium"));
    }

    #[test]
    fn opens_after_five_failures_in_the_window() {
        let (tracker, _clock) = tracker();
        for _ in 0..4 {
            tracker.record_failure("openai:small");
        }
        assert!(tracker.is_usable("openai:small"));
        tracker.record_failure("openai:small");
        assert_eq!(tracker.state("openai:small"), CircuitState::Open);
        assert!(!tracker.is_usable("openai:small"));
    }

    #[test]
    fn half_opens_after_the_cooldown() {
        let (tracker, clock) = tracker();
        for _ in 0..5 {
            tracker.record_failure("openai:small");
        }
        clock.advance(Duration::seconds(OPEN_FOR_SECS + 1));
        assert_eq!(tracker.state("openai:small"), CircuitState::HalfOpen);
        assert!(tracker.is_usable("openai:small"));
    }

    #[test]
    fn scattered_failures_do_not_open_the_circuit() {
        let (tracker, clock) = tracker();
        for _ in 0..10 {
            tracker.record_failure("openai:small");
            clock.advance(Duration::seconds(FAILURE_WINDOW_SECS + 1));
        }
        assert_eq!(tracker.state("openai:small"), CircuitState::Closed);
    }

    #[test]
    fn success_resets_the_count() {
        let (tracker, _clock) = tracker();
        for _ in 0..4 {
            tracker.record_failure("openai:small");
        }
        tracker.record_success("openai:small");
        tracker.record_failure("openai:small");
        assert!(tracker.is_usable("openai:small"));
    }
}
