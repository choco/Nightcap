//! What onboarding asks, which the shell pulls like the menu (ADR-0002), and when it shows: its
//! Night Shift steps (ADR-0014), and what the config or the install already answers.

use nightcap_protocol::{NightShift, Schedule};

use crate::schedule::CheckedSchedule;
use crate::state::State;
use crate::{Effect, LoginItem};

/// The current onboarding version: welcome, notifications, and website checking.
const ONBOARDING: u32 = 1;

/// What onboarding asks, pulled like the menu: the shell only draws it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OnboardingSteps {
    pub night_shift: OnboardingNightShift,
    /// Website checking as the config sets it, which answers its step for good.
    pub websites_from_config: Option<bool>,
    /// Whether Nightcap opens at login now, or None where it never opens itself.
    pub opens_at_login: Option<bool>,
}

/// Onboarding's Night Shift steps, which set the strength and the schedule (ADR-0014).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OnboardingNightShift {
    /// How warm, unless the config sets it or the strength can't be read.
    pub ask_strength: bool,
    /// When it comes on, unless the config sets it or Night Shift is unavailable.
    pub ask_schedule: bool,
    /// On a schedule already, so onboarding offers to skip to Nightcap's part.
    pub already_set_up: bool,
    /// The Mac's values, which the steps start from.
    pub strength: Option<u8>,
    pub schedule: Option<Schedule>,
}

impl OnboardingSteps {
    /// What onboarding asks, as `state` has it.
    pub(crate) fn new(state: &State) -> Self {
        OnboardingSteps {
            night_shift: state.onboarding_night_shift(),
            websites_from_config: state.facts.config_websites,
            opens_at_login: state
                .grants
                .login_item
                .filter(|_| state.manages_itself)
                .map(|item| item != LoginItem::NotRegistered),
        }
    }
}

impl State {
    fn onboarding_night_shift(&self) -> OnboardingNightShift {
        let available = self.facts.night_shift != NightShift::Unavailable;
        let ask_strength =
            self.strength.is_some() && self.loaded.config_night_shift.strength.is_none();
        let ask_schedule = available && self.loaded.config_night_shift.schedule.is_none();
        OnboardingNightShift {
            ask_strength,
            ask_schedule,
            already_set_up: (ask_strength || ask_schedule)
                && self
                    .facts
                    .timeline
                    .schedule
                    .is_some_and(|schedule| schedule != CheckedSchedule::Off),
            strength: self.strength,
            schedule: self.facts.timeline.schedule.map(Schedule::from),
        }
    }

    /// Shows onboarding from the version last completed, unless the config answers all it would
    /// ask.
    pub(crate) fn offer_onboarding(&mut self, effects: &mut Vec<Effect>) {
        if self.onboarding < ONBOARDING {
            // The config can answer website checking and Night Shift; opening at login
            // it can't.
            let night_shift = self.onboarding_night_shift();
            let night_shift_answered = !night_shift.ask_strength && !night_shift.ask_schedule;
            if self.facts.config_websites.is_some() && night_shift_answered && !self.manages_itself
            {
                self.onboarding = ONBOARDING;
            } else {
                effects.push(Effect::ShowOnboarding {
                    completed: self.onboarding,
                });
            }
        }
    }

    /// The user went through onboarding, choosing whether to check websites, which it asks
    /// only where the config doesn't set it.
    pub(crate) fn onboarding_finished(&mut self, websites: bool) {
        tracing::info!("onboarding finished");
        self.onboarding = ONBOARDING;
        if let Err(refusal) = self.set_websites(websites) {
            tracing::debug!("the config keeps website checking: {:?}", refusal.code);
        }
    }
}
