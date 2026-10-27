//! What hears the core's outcomes: the shell's one listener.

use crate::Outcome;

/// Hears the outcome of every fact and command, whoever sent it, in the order the core decided
/// them (ADR-0002). The core calls it on its own thread, between inputs and outside any lock, so
/// it hands the outcome on and returns: the shell's hops to the main queue.
#[uniffi::export(foreign)]
pub trait Listener: Send + Sync {
    fn outcome(&self, outcome: Outcome);
}
