//! Onboarding's part in the core: when it shows, which the config or the install can make
//! unneeded, and what finishing it sets (ADR-0012, ADR-0014). What it asks is the shell's.

use nightcap_protocol::NightShift;

use crate::Effect;
use crate::server::state::Inner;

/// The current onboarding version: welcome, notifications, and website checking.
const ONBOARDING: u32 = 1;

impl Inner {
    /// Whether the user went through this onboarding version, or the config answered all it
    /// asks.
    pub(crate) fn onboarding_done(&self) -> bool {
        self.onboarding >= ONBOARDING
    }

    /// Shows onboarding from the version last completed, unless the config answers all it would
    /// ask.
    pub(crate) fn offer_onboarding(&mut self, effects: &mut Vec<Effect>) {
        if self.onboarding < ONBOARDING {
            // The config can answer website checking and Night Shift; opening at login
            // it can't.
            if self.facts.config_websites.is_some()
                && self.config_answers_night_shift()
                && !self.manages_itself
            {
                self.onboarding = ONBOARDING;
            } else {
                effects.push(Effect::ShowOnboarding {
                    completed: self.onboarding,
                });
            }
        }
    }

    /// Whether onboarding's Night Shift steps have nothing to ask (ADR-0014): the config sets
    /// the strength, or it can't be read, and the config sets the schedule, or Night Shift is
    /// unavailable.
    fn config_answers_night_shift(&self) -> bool {
        let night_shift = &self.loaded.config_night_shift;
        let strength_answered = self.strength.is_none() || night_shift.strength.is_some();
        let schedule_answered =
            self.facts.night_shift == NightShift::Unavailable || night_shift.schedule.is_some();
        strength_answered && schedule_answered
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
