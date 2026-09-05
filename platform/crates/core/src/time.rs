//! An injectable clock, so time-dependent logic (expiry, quota periods,
//! backoff) can be tested without sleeping.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};

pub trait ClockSource: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Default)]
pub struct SystemClock;

impl ClockSource for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock that only moves when a test moves it.
#[derive(Clone)]
pub struct FixedClock(Arc<Mutex<DateTime<Utc>>>);

impl FixedClock {
    pub fn new(at: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(at)))
    }

    pub fn advance(&self, by: Duration) {
        let mut guard = self.0.lock().expect("clock mutex poisoned");
        *guard += by;
    }
}

impl ClockSource for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock mutex poisoned")
    }
}

#[derive(Clone)]
pub struct Clock(Arc<dyn ClockSource>);

impl Clock {
    pub fn system() -> Self {
        Self(Arc::new(SystemClock))
    }

    pub fn fixed(at: DateTime<Utc>) -> (Self, FixedClock) {
        let fixed = FixedClock::new(at);
        (Self(Arc::new(fixed.clone())), fixed)
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::system()
    }
}

impl std::fmt::Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clock").field("now", &self.now()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_clock_only_moves_when_told_to() {
        let start = DateTime::parse_from_rfc3339("2026-09-03T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (clock, handle) = Clock::fixed(start);
        assert_eq!(clock.now(), start);
        handle.advance(Duration::hours(2));
        assert_eq!(clock.now(), start + Duration::hours(2));
    }
}
