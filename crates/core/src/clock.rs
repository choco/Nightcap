//! Where the core reads the time: the system's clock, and in debug builds one that reads what
//! it's set to, so that tests control the time completely.

#[cfg(any(test, debug_assertions))]
use std::sync::{Mutex, MutexGuard};

use jiff::Timestamp;

/// Where the core reads the time.
pub(crate) trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// The system's clock.
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

/// A clock that reads what it's set to.
#[cfg(any(test, debug_assertions))]
#[derive(Debug)]
pub(crate) struct ManualClock(Mutex<Timestamp>);

#[cfg(any(test, debug_assertions))]
impl ManualClock {
    pub(crate) fn new(now: Timestamp) -> Self {
        Self(Mutex::new(now))
    }

    pub(crate) fn set(&self, now: Timestamp) {
        *self.time() = now;
    }

    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    fn time(&self) -> MutexGuard<'_, Timestamp> {
        self.0.lock().expect("no panic happens while it's held")
    }
}

#[cfg(any(test, debug_assertions))]
impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        *self.time()
    }
}
