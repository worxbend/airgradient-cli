//! When the next automatic refresh is due, and the clock that decides.

use std::time::{Duration, Instant};

/// The loop's read of wall-clock time, injected so tests can drive refresh
/// deadlines deterministically instead of sleeping.
pub(super) trait RuntimeClock {
    fn now(&mut self) -> Instant;
}

pub(super) struct SystemClock;

impl RuntimeClock for SystemClock {
    fn now(&mut self) -> Instant {
        Instant::now()
    }
}

/// When the next automatic refresh is due, and the interval it repeats on.
/// Held together because every event that changes one changes the other.
pub(super) struct RefreshSchedule {
    pub(super) interval: Duration,
    pub(super) next_at: Instant,
}

impl RefreshSchedule {
    /// Restarts the countdown from `now`. Called after every refresh so the
    /// interval is measured between refreshes rather than drifting off a
    /// fixed origin.
    pub(super) fn restart_from(&mut self, now: Instant) {
        self.next_at = now + self.interval;
    }
}
