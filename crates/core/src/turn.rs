//! One turn of the core, after every event and tick: it catches up on the schedule, then
//! rewatches, writes Night Shift, reschedules its wakeup, announces and saves. Writing Night
//! Shift and reading its status callbacks reconcile against the schedule the core computes
//! (ADR-0004).

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{Decision, NightShift, OnOff, Schedule};

use crate::rows::{name_in_front, until_text};
use crate::schedule::CheckedSchedule;
use crate::state::State;
use crate::{Core, Effect};

/// How far from a computed schedule transition a status callback can arrive and still be macOS
/// performing it: CoreBrightness's clock and the core's can differ slightly (ADR-0004).
const TRANSITION_TOLERANCE: SignedDuration = SignedDuration::from_mins(2);

/// How long after waking macOS may take to perform a schedule transition it missed during sleep.
/// A guess until measured.
const POST_WAKE_GRACE: SignedDuration = SignedDuration::from_mins(5);

impl Core {
    /// One turn of the core, for an event or a tick: it applies the schedule transitions and
    /// the ends that came due, lets `react` change the state, then rewatches, writes Night
    /// Shift, reschedules, announces the problems that appeared unless `aloud` is false, and
    /// saves.
    pub(crate) fn turn<T>(
        &self,
        aloud: bool,
        react: impl FnOnce(&mut State, &mut Vec<Effect>) -> T,
    ) -> (Vec<Effect>, T) {
        let mut state = self.state();
        self.catch_up(&mut state);
        let mut effects = Vec::new();
        let outcome = react(&mut state, &mut effects);
        self.rewatch(&mut state, &mut effects);
        self.enforce(&mut state, &mut effects);
        self.reschedule(&mut state, &mut effects);
        self.announce(&mut state, aloud, &mut effects);
        self.save(&mut state);
        (effects, outcome)
    }

    /// Night Shift as CoreBrightness reports it: while the session is this user's, a schedule
    /// edit is a schedule transition, and anything else is reconciled.
    pub(crate) fn status_changed(
        &self,
        state: &mut State,
        enabled: bool,
        schedule: Schedule,
        sun_schedule_set_aside: bool,
    ) -> Vec<Effect> {
        if state.reconciling.session == Session::Away {
            tracing::debug!("ignored a status callback while dormant");
            return Vec::new();
        }
        state.facts.night_shift = if enabled {
            NightShift::On
        } else {
            NightShift::Off
        };
        state.reconciling.pending_write = None;
        let keep_on = std::mem::take(&mut state.reconciling.keep_on_with_schedule_off);
        // A shell bug, logged rather than a panic, which would stop the app (ADR-0002).
        let checked = CheckedSchedule::try_from(schedule)
            .inspect_err(|error| {
                tracing::error!("the shell reported a schedule it can't have: {error}");
            })
            .ok();
        let previous = std::mem::replace(&mut state.facts.timeline.schedule, checked);
        state
            .grants
            .schedule_reported(sun_schedule_set_aside && schedule == Schedule::Off);
        if state.reconciling.session == Session::Back {
            // The shell's fresh read after returning: nobody could change Night Shift by
            // hand while the session was away.
            state.reconciling.session = Session::Active;
            Vec::new()
        } else if previous
            .zip(checked)
            .is_some_and(|(previous, checked)| previous != checked)
        {
            // macOS applies an edited schedule at once, so the edit is a schedule
            // transition and whatever it switched isn't an external change.
            tracing::info!("schedule edited");
            state.places.schedule_edited();
            if keep_on && schedule == Schedule::Off && !enabled {
                let now = self.clock.now();
                state.places.switch(OnOff::On, &state.facts.world(now));
            }
            Vec::new()
        } else {
            self.reconcile(state, enabled).into_iter().collect()
        }
    }

    /// Reconciles a status callback with what the core wants (ADR-0004). macOS catching up on a
    /// transition the core already applied, or running just ahead of the next one, is that
    /// transition; a match needs nothing; anything else is an external change, handled like the
    /// equivalent command (ADR-0018).
    fn reconcile(&self, state: &mut State, enabled: bool) -> Option<Effect> {
        if state.reconciling.left_by_last_run.take() == Some(OnOff::from(enabled)) {
            tracing::info!("Night Shift is where the last run left it");
            return None;
        }
        let now = self.clock.now();
        match state.reconciling.unreported.take() {
            Some(unreported) if now <= unreported.until && enabled == unreported.scheduled => {
                tracing::info!(
                    "macOS performed the schedule transition at {}, {:#} late",
                    unreported.transition,
                    now.duration_since(unreported.transition)
                );
                return None;
            }
            Some(unreported) if now <= unreported.until => {
                state.reconciling.unreported = Some(unreported);
            }
            _ => {}
        }
        let wanted = wanted(&state.places.look(&state.facts.world(now)).decision)?;
        if enabled == wanted {
            return None;
        }
        if let Some(transition) = state
            .facts
            .timeline
            .next_transition(state.facts.applied_until)
            && transition.duration_since(now) <= TRANSITION_TOLERANCE
            && state.facts.timeline.scheduled_state(transition) == Some(enabled)
        {
            tracing::info!(
                "schedule transition at {transition}, reported {:#} early",
                transition.duration_since(now)
            );
            state.places.expire(transition);
            state.facts.applied_until = transition;
            return None;
        }
        let observed = OnOff::from(enabled);
        tracing::info!("Night Shift turned {observed} outside Nightcap");
        let snooze = state.places.adopt(observed, &state.facts.world(now))?;
        Some(Effect::Notify {
            title: format!("Night Shift turned {} outside Nightcap", snooze.state),
            body: format!(
                "Snoozed the {} rule {}.",
                name_in_front(&snooze.rule, &state.facts.context),
                until_text(snooze.until, snooze.state, &state.facts.timeline.zone)
            ),
            opens: None,
        })
    }

    /// Applies every schedule transition that came due since the last event, and ends the
    /// timers and holds that ran out.
    fn catch_up(&self, state: &mut State) {
        let now = self.clock.now();
        if now <= state.facts.applied_until {
            return;
        }
        state.places.expire(now);
        if let Some(transition) = state
            .facts
            .timeline
            .next_transition(state.facts.applied_until)
            && transition <= now
        {
            tracing::info!("schedule transition at {transition}");
            state.reconciling.unreported =
                state
                    .facts
                    .timeline
                    .scheduled_state(now)
                    .map(|scheduled| Unreported {
                        transition,
                        scheduled,
                        until: now + TRANSITION_TOLERANCE,
                    });
        }
        state.facts.applied_until = now;
    }

    /// Keeps the shell's one wakeup at the next schedule transition or the end of a timer or
    /// hold, whichever comes first. Sunset to sunrise with no transition ahead lacks the sun
    /// times, which CoreBrightness can take a minute or two after login to work out, so the
    /// wakeup comes at the next whole minute instead: the shell reads them again before each.
    fn reschedule(&self, state: &mut State, effects: &mut Vec<Effect>) {
        let now = self.clock.now();
        let transition = state
            .facts
            .timeline
            .next_transition(state.facts.applied_until);
        let awaits_sun_times = transition.is_none()
            && state.facts.timeline.schedule == Some(CheckedSchedule::SunsetToSunrise);
        state.reconciling.sun_times_awaited_since =
            awaits_sun_times.then(|| state.reconciling.sun_times_awaited_since.unwrap_or(now));
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
        let look = state.places.look(&state.facts.world(now));
        let ends = look
            .layers
            .iter()
            .flat_map(|layers| [&layers.hold, &layers.timer])
            .flatten()
            .filter_map(|layer| layer.until);
        let next = transition.or(retry).into_iter().chain(ends).min();
        if next != state.reconciling.wakeup {
            state.reconciling.wakeup = next;
            effects.push(match next {
                Some(at) => Effect::ScheduleWakeup { at: at.into() },
                None => Effect::CancelWakeup,
            });
        }
    }

    /// Writes Night Shift when what the core wants differs from what was last observed
    /// (ADR-0004), once until a status callback reports the result.
    fn enforce(&self, state: &mut State, effects: &mut Vec<Effect>) {
        if state.facts.night_shift == NightShift::Unavailable
            || state.reconciling.session != Session::Active
        {
            return;
        }
        // A new watch keeps Night Shift as it is until its first read (ADR-0012).
        if state.watching.awaits_first_read() {
            return;
        }
        let look = state.places.look(&state.facts.world(self.clock.now()));
        let Some(wanted) = wanted(&look.decision) else {
            return;
        };
        if wanted != (state.facts.night_shift == NightShift::On)
            && state.reconciling.pending_write != Some(wanted)
        {
            state.reconciling.pending_write = Some(wanted);
            effects.push(Effect::SetNightShift { enabled: wanted });
        }
    }
}

/// What reconciling Night Shift with macOS keeps from one turn to the next (ADR-0004).
pub(crate) struct Reconciling {
    /// The write the shell was last asked for, until a status callback reports the result.
    pending_write: Option<bool>,
    unreported: Option<Unreported>,
    /// Where the last run left Night Shift, which the first status callback will show.
    left_by_last_run: Option<OnOff>,
    session: Session,
    /// Nightcap switched the schedule off while Night Shift was on, and macOS switches it off
    /// with the schedule: the edit's callback turns it back on, as choosing "By hand" means.
    keep_on_with_schedule_off: bool,
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
            pending_write: None,
            unreported: None,
            left_by_last_run,
            session: Session::Active,
            keep_on_with_schedule_off: false,
            wakeup: None,
            sun_times_awaited_since: None,
        }
    }

    /// Since when sunset to sunrise has had no transition ahead, for want of sun times.
    pub(crate) fn sun_times_awaited_since(&self) -> Option<Timestamp> {
        self.sun_times_awaited_since
    }

    /// Nightcap is about to write `schedule` on command, with Night Shift as it is.
    pub(crate) fn writing_schedule(&mut self, schedule: Schedule, night_shift: NightShift) {
        self.keep_on_with_schedule_off = schedule == Schedule::Off && night_shift == NightShift::On;
    }

    pub(crate) fn timer_fired(&mut self) {
        // The shell's timer is spent, whether or not its transition came due.
        self.wakeup = None;
    }

    pub(crate) fn session_changed(&mut self, active: bool) {
        tracing::info!("session active: {active}");
        self.session = if active { Session::Back } else { Session::Away };
    }

    pub(crate) fn woke(&mut self, now: Timestamp) {
        tracing::debug!("woke");
        if let Some(unreported) = &mut self.unreported {
            unreported.until = now + POST_WAKE_GRACE;
        }
    }
}

/// A schedule transition the core applied on its own clock that macOS hasn't reported
/// performing yet. Until `until`, a callback matching it is macOS catching up, not an external
/// change.
struct Unreported {
    transition: Timestamp,
    /// Whether the schedule has Night Shift on after the transition.
    scheduled: bool,
    until: Timestamp,
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
