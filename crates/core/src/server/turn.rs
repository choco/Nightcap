//! The steps of a turn that keep Night Shift in line with macOS: catching up on the schedule,
//! reading its status reports, writing Night Shift and keeping the wakeup. Writing Night Shift and
//! reading its status reports reconcile against the schedule the core computes (ADR-0004).

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{Decision, NightShift, OnOff, Reason, Schedule};

use crate::schedule::CheckedSchedule;
use crate::server::state::{Inner, Session};
use crate::setup::checks::SUN_TIMES_PATIENCE;
use crate::{Effect, SwitchedBy};

impl Inner {
    /// Night Shift as CoreBrightness reports it: while the session is this user's, a schedule
    /// edit is a schedule transition, and anything else is reconciled.
    pub(crate) fn status_changed(
        &mut self,
        enabled: bool,
        schedule: Schedule,
        sun_schedule_set_aside: bool,
        written_at: Option<Timestamp>,
        switched_by: SwitchedBy,
        now: Timestamp,
    ) -> Option<Effect> {
        if self.facts.session == Session::Away {
            tracing::debug!("ignored a status callback while dormant");
            return None;
        }
        self.facts.written_at = written_at;
        let observed = if enabled {
            NightShift::On
        } else {
            NightShift::Off
        };
        let switched = std::mem::replace(&mut self.facts.night_shift, observed) != observed;
        // A shell bug, logged rather than a panic, which would stop the app (ADR-0002).
        let checked = CheckedSchedule::try_from(schedule)
            .inspect_err(|error| {
                tracing::error!("the shell reported a schedule it can't have: {error}");
            })
            .ok();
        let previous = std::mem::replace(&mut self.facts.timeline.schedule, checked);
        self.setup
            .schedule_reported(sun_schedule_set_aside && schedule == Schedule::Off);
        if self.facts.session == Session::Back {
            // The shell's fresh read after returning: nobody could change Night Shift by
            // hand while the session was away.
            self.facts.session = Session::Active;
            None
        } else if previous
            .zip(checked)
            .is_some_and(|(previous, checked)| previous != checked)
        {
            // macOS applies an edited schedule at once, so the edit is a schedule
            // transition and whatever it switched isn't an outside change.
            tracing::info!("schedule edited");
            self.rules.schedule_edited();
            None
        } else {
            self.reconcile(enabled, switched_by, switched, now)
        }
    }

    /// Reconciles a status callback with what the core wants, by whose write it shows
    /// (ADR-0004). The schedule's switch and Nightcap's own write need nothing. Another app's
    /// write that differs from what the core wants is an outside change, handled like the
    /// equivalent command (ADR-0018), and told.
    fn reconcile(
        &mut self,
        enabled: bool,
        switched_by: SwitchedBy,
        switched: bool,
        now: Timestamp,
    ) -> Option<Effect> {
        if self.facts.left_by_last_run.take() == Some(OnOff::from(enabled)) {
            tracing::info!("Night Shift is where the last run left it");
            return None;
        }
        let observed = OnOff::from(enabled);
        match switched_by {
            SwitchedBy::Schedule => {
                if switched {
                    tracing::info!("the schedule turned Night Shift {observed}");
                }
                None
            }
            SwitchedBy::Nightcap => None,
            SwitchedBy::Outside => {
                let wanted = wanted(&self.rules.decide(&self.facts.world(now)))?;
                if enabled == wanted {
                    return None;
                }
                tracing::info!("Night Shift turned {observed} outside Nightcap");
                self.rules.adopt(observed, &self.facts.world(now))
            }
        }
    }

    /// Ends the rules that ran out, and applies every schedule transition that came due since the
    /// last turn. Rules end by the clock as it reads; the schedule never runs backwards, though
    /// the clock can.
    pub(crate) fn catch_up(&mut self, now: Timestamp) {
        self.rules.expire(now);
        if now <= self.facts.applied_until {
            return;
        }
        if let Some(transition) = self
            .facts
            .timeline
            .next_transition(self.facts.applied_until)
            && transition <= now
        {
            tracing::info!("schedule transition at {transition}");
        }
        self.facts.applied_until = now;
    }

    /// Keeps the shell's one wakeup at the next schedule transition or the end of a rule,
    /// whichever comes first, asking only when that changes. Sunset to sunrise with no
    /// transition ahead lacks the sun times, which CoreBrightness can take a minute or two after
    /// login to work out, so the wakeup comes at the next whole minute instead: the shell reads
    /// them again before each. It comes too as the patience for them ends, when going without
    /// them becomes a problem with no input to show it, so that the problem appears in a fact's
    /// turn.
    pub(crate) fn reschedule(&mut self, now: Timestamp) -> Option<Effect> {
        let transition = self
            .facts
            .timeline
            .next_transition(self.facts.applied_until);
        let awaits_sun_times = transition.is_none()
            && self.facts.timeline.schedule == Some(CheckedSchedule::SunsetToSunrise);
        self.facts.sun_times_awaited_since =
            awaits_sun_times.then(|| self.facts.sun_times_awaited_since.unwrap_or(now));
        #[expect(
            clippy::expect_used,
            reason = "rounding fails only beside jiff's limits, millennia from any clock"
        )]
        let retry = awaits_sun_times.then(|| {
            now.round(
                jiff::TimestampRound::new()
                    .smallest(jiff::Unit::Minute)
                    .mode(jiff::RoundMode::Trunc),
            )
            .expect("a minute rounds any time")
                + SignedDuration::from_mins(1)
        });
        let patience_ends = self
            .facts
            .sun_times_awaited_since
            .map(|since| since + SUN_TIMES_PATIENCE)
            .filter(|ends| *ends > now);
        let next = transition
            .or(retry)
            .into_iter()
            .chain(patience_ends)
            .chain(self.rules.ends())
            .min();
        if next == self.wakeup {
            return None;
        }
        self.wakeup = next;
        Some(match next {
            Some(at) => Effect::ScheduleWakeup { at: at.into() },
            None => Effect::CancelWakeup,
        })
    }

    /// Writes Night Shift when what the core wants differs from what was last observed
    /// (ADR-0004), short of what's left to macOS's schedule. The shell writes once while the
    /// same write is on its way, so asking again before its status arrives changes nothing.
    pub(crate) fn enforce(&self, now: Timestamp) -> Option<Effect> {
        if self.facts.night_shift == NightShift::Unavailable
            || self.facts.session != Session::Active
        {
            return None;
        }
        // A new watch keeps Night Shift as it is until its first read (ADR-0012).
        if self.watching.awaits_first_read() {
            return None;
        }
        let decision = self.rules.decide(&self.facts.world(now));
        let wanted = wanted(&decision)?;
        if self.left_to_macos(&decision) || wanted == (self.facts.night_shift == NightShift::On) {
            return None;
        }
        Some(Effect::SetNightShift { enabled: wanted })
    }

    /// Whether Nightcap leaves Night Shift to macOS's schedule, which performs its own
    /// transitions, fading them over minutes where a write fades in two seconds. It does
    /// wherever the schedule alone decides, unless a write since the schedule's last transition
    /// stands: Nightcap's own, from a rule that no longer decides. Night Shift with no write
    /// standing is as the schedule left it, or as its next transition will, and macOS performs
    /// a transition over any write from before it (ADR-0004).
    fn left_to_macos(&self, decision: &Decision) -> bool {
        let last_transition = (self.facts.timeline).last_transition(self.facts.applied_until);
        matches!(decision.reasons.first(), Some(Reason::Schedule { .. }))
            && self.facts.written_at.is_none_or(|written| {
                last_transition.is_some_and(|transition| written < transition)
            })
    }
}

/// The Night Shift state a decision asks for, if it asks for one.
pub(crate) fn wanted(decision: &Decision) -> Option<bool> {
    decision.state.map(|state| state == OnOff::On)
}
