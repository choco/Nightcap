//! Locking the core's mutexes, which no panic can poison: panics abort (ADR-0002).

use std::sync::{Mutex, MutexGuard};

#[expect(
    clippy::expect_used,
    reason = "panics abort (ADR-0002), so none can poison the lock"
)]
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("no panic happens while it's held")
}
