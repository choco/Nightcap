//! Onboarding's part in the core: when it shows, which the config or the install can make
//! unneeded, and what finishing it sets (ADR-0012, ADR-0014). What it asks is the shell's.

use nightcap_protocol::{NightShift, OnboardingAsks};

use crate::Effect;
use crate::commands::kind;
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
            let asks = self.onboarding_asks();
            if self.facts.config_websites.is_some()
                && !asks.strength
                && !asks.schedule
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

    /// What onboarding's Night Shift steps ask (ADR-0014): the strength, unless the config sets
    /// it or it can't be read, and the schedule, unless the config sets it or Night Shift is
    /// unavailable.
    pub(crate) fn onboarding_asks(&self) -> OnboardingAsks {
        let night_shift = &self.loaded.config_night_shift;
        OnboardingAsks {
            strength: self.strength.is_some() && night_shift.strength.is_none(),
            schedule: self.facts.night_shift != NightShift::Unavailable
                && night_shift.schedule.is_none(),
        }
    }

    /// The user went through onboarding, with whether they chose to check websites where it
    /// asked, which is only where the config doesn't set it.
    pub(crate) fn onboarding_finished(&mut self, websites: Option<bool>) {
        tracing::info!("onboarding finished");
        self.onboarding = ONBOARDING;
        if let Some(on) = websites
            && let Err(refusal) = self.set_websites(on)
        {
            // The config came to set it while onboarding was open.
            tracing::info!("the config keeps website checking: {}", kind(&refusal));
        }
    }
}
