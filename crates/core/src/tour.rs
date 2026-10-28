//! The tour's start and end (ADR-0016): what changes while it runs lasts only while it runs, and
//! a tour that finds Night Shift off stands in for the schedule with it on.

use jiff::Timestamp;
use nightcap_protocol::NightShift;

use crate::server::state::Inner;

impl Inner {
    /// Starts the tour, unless one runs (ADR-0016). It stands in for the schedule with Night
    /// Shift on if it finds Night Shift off.
    pub(crate) fn tour_started(&mut self) {
        let stands_in = self.facts.night_shift == NightShift::Off;
        if !self.places.start_tour(stands_in) {
            tracing::warn!("a tour started while one runs; keeping the first");
            return;
        }
        tracing::info!(
            "tour started{}",
            if stands_in {
                ", standing in for the schedule with Night Shift on"
            } else {
                ""
            }
        );
    }

    /// Ends the tour, which puts back what it found, short of what ran out by `now`.
    pub(crate) fn tour_ended(&mut self, now: Timestamp) {
        self.places.end_tour(now);
    }
}
