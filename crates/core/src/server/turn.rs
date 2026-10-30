//! One turn of the core, for every fact and command: it catches up on the schedule, then
//! rewatches, writes Night Shift, reschedules its wakeup and saves. Writing Night
//! Shift and reading its status callbacks reconcile against the schedule the core computes
//! (ADR-0004).

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{Decision, NightShift, OnOff, Reason, Schedule};

use crate::rules::schedule::CheckedSchedule;
use crate::server::state::Inner;
use crate::setup::checks::SUN_TIMES_PATIENCE;
use crate::{Cause, Effect, Outcome, SwitchedBy};

impl Inner {
    /// One turn of the core, for a fact or a command, whose outcome answers `cause`: it applies
    /// the schedule transitions and the ends that came due, lets `react` change the state, then
    /// rewatches, writes Night Shift, reschedules and saves.
    pub(crate) fn turn<T>(
        &mut self,
        cause: Cause,
        react: impl FnOnce(&mut Self, &mut Outcome) -> T,
    ) -> (Outcome, T) {
        self.catch_up();
        let mut outcome = Outcome::new(cause);
        let answer = react(self, &mut outcome);
        self.rewatch(&mut outcome.effects);
        self.enforce(&mut outcome.effects);
        self.reschedule(&mut outcome.effects);
        self.save();
        (outcome, answer)
    }

    /// Night Shift as CoreBrightness reports it: while the session is this user's, a schedule
    /// edit is a schedule transition, and anything else is reconciled.
    pub(crate) fn status_changed(
        &mut self,
        enabled: bool,
        schedule: Schedule,
        sun_schedule_set_aside: bool,
        written_at: Option<Timestamp>,
        switched_by: SwitchedBy,
    ) -> Option<Effect> {
        if self.reconciling.session == Session::Away {
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
        self.grants
            .schedule_reported(sun_schedule_set_aside && schedule == Schedule::Off);
        if self.reconciling.session == Session::Back {
            // The shell's fresh read after returning: nobody could change Night Shift by
            // hand while the session was away.
            self.reconciling.session = Session::Active;
            None
        } else if previous
            .zip(checked)
            .is_some_and(|(previous, checked)| previous != checked)
        {
            // macOS applies an edited schedule at once, so the edit is a schedule
            // transition and whatever it switched isn't an outside change.
            tracing::info!("schedule edited");
            self.places.schedule_edited();
            None
        } else {
            self.reconcile(enabled, switched_by, switched)
        }
    }

    /// Reconciles a status callback with what the core wants, by whose write it shows
    /// (ADR-0004). The schedule's switch and Nightcap's own write need nothing. Another app's
    /// write that differs from what the core wants is an outside change, handled like the
    /// equivalent command (ADR-0018).
    fn reconcile(
        &mut self,
        enabled: bool,
        switched_by: SwitchedBy,
        switched: bool,
    ) -> Option<Effect> {
        if self.reconciling.left_by_last_run.take() == Some(OnOff::from(enabled)) {
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
                let now = self.clock.now();
                let wanted = wanted(&self.places.decide(&self.facts.world(now)))?;
                if enabled == wanted {
                    return None;
                }
                tracing::info!("Night Shift turned {observed} outside Nightcap");
                self.places.adopt(observed, &self.facts.world(now))
            }
        }
    }

    /// Ends the rules that ran out, and applies every schedule transition that came due since the
    /// last turn. Rules end by the clock as it reads; the schedule never runs backwards, though
    /// the clock can.
    fn catch_up(&mut self) {
        let now = self.clock.now();
        self.places.expire(now);
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
    /// whichever comes first. Sunset to sunrise with no transition ahead lacks the sun
    /// times, which CoreBrightness can take a minute or two after login to work out, so the
    /// wakeup comes at the next whole minute instead: the shell reads them again before each.
    /// It comes too as the patience for them ends, when going without them becomes a problem
    /// with no input to show it, so that the problem appears in a fact's turn.
    fn reschedule(&mut self, effects: &mut Vec<Effect>) {
        let now = self.clock.now();
        let transition = self
            .facts
            .timeline
            .next_transition(self.facts.applied_until);
        let awaits_sun_times = transition.is_none()
            && self.facts.timeline.schedule == Some(CheckedSchedule::SunsetToSunrise);
        self.reconciling.sun_times_awaited_since =
            awaits_sun_times.then(|| self.reconciling.sun_times_awaited_since.unwrap_or(now));
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
            .reconciling
            .sun_times_awaited_since
            .map(|since| since + SUN_TIMES_PATIENCE)
            .filter(|ends| *ends > now);
        let next = transition
            .or(retry)
            .into_iter()
            .chain(patience_ends)
            .chain(self.places.ends())
            .min();
        if next != self.reconciling.wakeup {
            self.reconciling.wakeup = next;
            effects.push(match next {
                Some(at) => Effect::ScheduleWakeup { at: at.into() },
                None => Effect::CancelWakeup,
            });
        }
    }

    /// Writes Night Shift when what the core wants differs from what was last observed
    /// (ADR-0004), short of what's left to macOS's schedule. The shell writes once while the
    /// same write is on its way, so asking again before its status arrives changes nothing.
    fn enforce(&mut self, effects: &mut Vec<Effect>) {
        if self.facts.night_shift == NightShift::Unavailable
            || self.reconciling.session != Session::Active
        {
            return;
        }
        // A new watch keeps Night Shift as it is until its first read (ADR-0012).
        if self.watching.awaits_first_read() {
            return;
        }
        let decision = self.places.decide(&self.facts.world(self.clock.now()));
        let Some(wanted) = wanted(&decision) else {
            return;
        };
        if self.left_to_macos(&decision) {
            return;
        }
        if wanted != (self.facts.night_shift == NightShift::On) {
            effects.push(Effect::SetNightShift { enabled: wanted });
        }
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

/// What reconciling Night Shift with macOS keeps from one turn to the next (ADR-0004).
pub(crate) struct Reconciling {
    /// Where the last run left Night Shift, which the first status callback will show.
    left_by_last_run: Option<OnOff>,
    session: Session,
    /// The wakeup the shell was last asked for.
    wakeup: Option<Timestamp>,
    /// Since when sunset to sunrise has had no transition ahead, for want of sun times.
    sun_times_awaited_since: Option<Timestamp>,
}

impl Reconciling {
    /// Nothing written or awaited yet, in an active session, with Night Shift where the last
    /// run left it.
    pub(crate) fn new(left_by_last_run: Option<OnOff>) -> Self {
        Self {
            left_by_last_run,
            session: Session::Active,
            wakeup: None,
            sun_times_awaited_since: None,
        }
    }

    /// Since when sunset to sunrise has had no transition ahead, for want of sun times.
    pub(crate) fn sun_times_awaited_since(&self) -> Option<Timestamp> {
        self.sun_times_awaited_since
    }

    pub(crate) fn timer_fired(&mut self) {
        // The shell's timer is spent, whether or not its transition came due.
        self.wakeup = None;
    }

    pub(crate) fn session_changed(&mut self, active: bool) {
        tracing::info!("session active: {active}");
        self.session = if active { Session::Back } else { Session::Away };
    }
}

/// Whether Night Shift is this user's to change: with fast user switching, another user's
/// session can be in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Session {
    Active,
    /// Another user is in front: status callbacks aren't this user's, and nothing is written.
    Away,
    /// Back in front, waiting for the shell's fresh read of a status that went unwatched.
    Back,
}

/// The Night Shift state a decision asks for, if it asks for one.
pub(crate) fn wanted(decision: &Decision) -> Option<bool> {
    decision.state.map(|state| state == OnOff::On)
}
