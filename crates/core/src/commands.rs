//! Commands, from the menu, Settings and the CLI alike, each answering with the decision it
//! leads to; the rule commits they make; and reading the config again, which is how a change to
//! it takes effect.

use jiff::SignedDuration;
use nightcap_protocol::{
    Command, Decision, ErrorCode, ErrorReply, NightShift, OnOff, Rule, RuleIdentity,
};

use crate::adhoc::AdHocError;
use crate::config::{ConfigError, NightShiftConfig};
use crate::places::{Places, Refusal};
use crate::schedule::CheckedSchedule;
use crate::state::State;
use crate::{Core, Effect, LoginItem, config};

impl Core {
    /// Runs a command the user chose in the menu or Settings, which answers as the CLI's would.
    /// A problem it causes isn't announced: the user is looking at what they chose.
    pub(crate) fn choose(&self, command: &Command) -> (Vec<Effect>, Result<Decision, ErrorReply>) {
        self.turn(false, |state, effects| {
            self.run_command(state, command, effects)
        })
    }

    /// Runs a command from the CLI or the menu, answering with the resulting decision.
    pub(crate) fn run_command(
        &self,
        state: &mut State,
        command: &Command,
        effects: &mut Vec<Effect>,
    ) -> Result<Decision, ErrorReply> {
        let night_shift = state.facts.night_shift;
        let needs_night_shift = matches!(
            command,
            Command::On
                | Command::Off
                | Command::Toggle
                | Command::Set { .. }
                | Command::Reset(_)
                | Command::Strength(_)
                | Command::PreviewStrength(_)
                | Command::SetSchedule(_)
        );
        if needs_night_shift && night_shift == NightShift::Unavailable {
            return Err(refused(
                ErrorCode::NightShiftUnavailable,
                "Night Shift isn't available on this Mac",
            ));
        }
        let now = self.clock.now();
        let outcome = match command {
            Command::On => {
                state.places.switch(OnOff::On, &state.facts.world(now));
                Ok(())
            }
            Command::Off => {
                state.places.switch(OnOff::Off, &state.facts.world(now));
                Ok(())
            }
            Command::Toggle => {
                let world = state.facts.world(now);
                let to = state.places.look(&world).everywhere.gives.other();
                state.places.switch(to, &world);
                Ok(())
            }
            Command::Set {
                place,
                state: on,
                length,
            } => state
                .places
                .set(place, *on, *length, &state.facts.world(now))
                .map_err(ErrorReply::from),
            Command::Reset(place) => {
                state.places.reset(place);
                Ok(())
            }
            Command::AddRule { identity, says } => self.add_rule(state, identity, *says),
            Command::RemoveRule(identity) => self.remove_rule(state, identity),
            Command::OpenAtLogin(chosen) => {
                if state.manages_itself {
                    state.grants.open_at_login = *chosen == OnOff::On;
                    match chosen {
                        OnOff::On => effects.push(Effect::RegisterLoginItem),
                        // An item waiting for approval would still ask for it.
                        OnOff::Off if state.grants.login_item != Some(LoginItem::NotRegistered) => {
                            effects.push(Effect::UnregisterLoginItem);
                        }
                        OnOff::Off => {}
                    }
                    Ok(())
                } else {
                    Err(refused(
                        ErrorCode::NoLoginItem,
                        "this install doesn't open itself at login: its installer starts it, or it's a dev build",
                    ))
                }
            }
            Command::Strength(value) | Command::PreviewStrength(value) => {
                if state.strength.is_none() {
                    Err(refused(
                        ErrorCode::StrengthUnavailable,
                        "Night Shift's strength can't be read on this Mac",
                    ))
                } else if *value > 100 {
                    Err(refused(
                        ErrorCode::BadRequest,
                        format!("a strength is 0 to 100, not {value}"),
                    ))
                } else {
                    let commit = matches!(command, Command::Strength(_));
                    if commit {
                        tracing::info!("strength set to {value}");
                    } else {
                        tracing::debug!("previewing strength {value}");
                    }
                    effects.push(Effect::SetStrength {
                        value: *value,
                        commit,
                    });
                    Ok(())
                }
            }
            Command::SetSchedule(schedule) => match CheckedSchedule::try_from(*schedule) {
                Err(error) => Err(refused(ErrorCode::BadRequest, error.to_string())),
                Ok(_) => {
                    tracing::info!("schedule set on command");
                    state.reconciling.writing_schedule(*schedule, night_shift);
                    effects.push(Effect::SetSchedule {
                        schedule: *schedule,
                    });
                    Ok(())
                }
            },
            Command::Websites(chosen) => {
                let on = *chosen == OnOff::On;
                let turned_on = on && !state.facts.website_checking();
                state.set_websites(on).map(|()| {
                    if turned_on && state.grants.accessibility != Some(true) {
                        effects.push(Effect::AskForAccessibility);
                    }
                })
            }
        };
        outcome.map(|()| state.places.look(&state.facts.world(now)).decision)
    }

    fn add_rule(
        &self,
        state: &mut State,
        identity: &RuleIdentity,
        says: OnOff,
    ) -> Result<(), ErrorReply> {
        let mut next = state.places.clone();
        next.add_rule(identity, says)?;
        self.commit_rules(state, next)
    }

    fn remove_rule(&self, state: &mut State, identity: &RuleIdentity) -> Result<(), ErrorReply> {
        let mut next = state.places.clone();
        next.remove_rule(identity)?;
        self.commit_rules(state, next)
    }

    /// Takes `next` as the places once `rules.toml` keeps the ad-hoc rules it has, if they
    /// changed.
    fn commit_rules(&self, state: &mut State, next: Places) -> Result<(), ErrorReply> {
        if next.kept().ad_hoc != state.places.kept().ad_hoc {
            state.check_ad_hoc_rules_on()?;
            self.ad_hoc_store
                .save(&next.kept().ad_hoc)
                .map_err(|error| {
                    refused(
                        ErrorCode::AdHocRulesOff,
                        format!("couldn't save rules.toml: {error}"),
                    )
                })?;
        }
        state.places = next;
        Ok(())
    }

    /// Takes the config's rules, dropping the ad-hoc rules it now has, which have graduated
    /// (ADR-0007), from `rules.toml` too. The config wins over them whether or not the file can
    /// be written, and the next load drops them again.
    pub(crate) fn graduate(&self, state: &mut State, rules: Vec<Rule>) {
        let before = state.places.kept().ad_hoc.len();
        state.places.set_config(rules);
        let graduated = before - state.places.kept().ad_hoc.len();
        if graduated > 0 {
            tracing::info!("{graduated} ad-hoc rules graduated into the config");
            if let Err(error) = self.ad_hoc_store.save(&state.places.kept().ad_hoc) {
                tracing::warn!("couldn't save rules.toml: {error}");
            }
        }
    }

    /// Loads the config again; an invalid one keeps the rules loaded before, and its check says
    /// why.
    pub(crate) fn reload(
        &self,
        state: &mut State,
        effects: &mut Vec<Effect>,
    ) -> Result<(), ErrorReply> {
        match config::load(self.files.as_ref()) {
            Ok(config) => {
                tracing::info!("reloaded the config: {} rules", config.rules.len());
                state.facts.config_websites = config.websites;
                state.loaded.config_night_shift = config.night_shift;
                state.loaded.lengths = config.lengths;
                state.apply_config_night_shift(effects);
                state.loaded.config_error = None;
                state.loaded.config_found = config.found;
                self.graduate(state, config.rules);
                Ok(())
            }
            Err(error) => {
                tracing::warn!(
                    "keeping the rules loaded before: the config {}",
                    error.kind()
                );
                let refusal = refused(ErrorCode::InvalidConfig, error.to_string());
                state.loaded.config_error = Some(error);
                Err(refusal)
            }
        }
    }
}

impl State {
    /// Website checking as the user chose it, unless the config sets it (ADR-0012).
    pub(crate) fn set_websites(&mut self, on: bool) -> Result<(), ErrorReply> {
        if self.facts.config_websites.is_some() {
            return Err(refused(
                ErrorCode::SetInConfig,
                "website checking is set by `websites` in the config",
            ));
        }
        tracing::info!("website checking {}", OnOff::from(on));
        self.facts.websites = on;
        Ok(())
    }

    fn check_ad_hoc_rules_on(&self) -> Result<(), ErrorReply> {
        match &self.loaded.ad_hoc_error {
            None => Ok(()),
            Some(error) => Err(refused(
                ErrorCode::AdHocRulesOff,
                format!("ad-hoc rules are off: {error}"),
            )),
        }
    }

    /// Writes the config's strength and schedule where they changed since last applied, once
    /// Night Shift, and for the strength its value, is known (ADR-0014).
    pub(crate) fn apply_config_night_shift(&mut self, effects: &mut Vec<Effect>) {
        if self.facts.night_shift == NightShift::Unavailable {
            return;
        }
        let wanted = self.loaded.config_night_shift;
        if wanted.strength != self.loaded.applied_from_config.strength && self.strength.is_some() {
            if let Some(value) = wanted.strength {
                tracing::info!("strength set to {value} from the config");
                effects.push(Effect::SetStrength {
                    value,
                    commit: true,
                });
            }
            self.loaded.applied_from_config.strength = wanted.strength;
        }
        if wanted.schedule != self.loaded.applied_from_config.schedule {
            if let Some(schedule) = wanted.schedule {
                tracing::info!("schedule set from the config");
                effects.push(Effect::SetSchedule { schedule });
            }
            self.loaded.applied_from_config.schedule = wanted.schedule;
        }
    }
}

/// What the core read from its files: the config's lengths and `[night_shift]`, whether the
/// config was found, and why it or `rules.toml` can't be used; with what of the config's
/// `[night_shift]` was last applied (ADR-0014).
pub(crate) struct Loaded {
    pub(crate) config_error: Option<ConfigError>,
    /// Whether the config file existed when it was last loaded.
    pub(crate) config_found: bool,
    /// The menu's preset lengths, from the config or else 30 minutes, 1 hour and 2 hours.
    pub(crate) lengths: Vec<SignedDuration>,
    pub(crate) config_night_shift: NightShiftConfig,
    pub(crate) applied_from_config: NightShiftConfig,
    /// Why `rules.toml` can't be used; while set, ad-hoc rules are off and the file is left be.
    pub(crate) ad_hoc_error: Option<AdHocError>,
}

impl From<Refusal> for ErrorReply {
    fn from(refusal: Refusal) -> Self {
        let code = match refusal {
            Refusal::TooShort
            | Refusal::TooLong(_)
            | Refusal::Passed
            | Refusal::NoSchedule
            | Refusal::ForGoodIsARule => ErrorCode::BadRequest,
            Refusal::ConfigSays { .. } | Refusal::ConfigRule(_) => ErrorCode::ConfigRule,
            Refusal::NoSuchRule(_) => ErrorCode::NoSuchRule,
        };
        refused(code, refusal.to_string())
    }
}

pub(crate) fn refused(code: ErrorCode, message: impl Into<String>) -> ErrorReply {
    ErrorReply {
        code,
        message: message.into(),
    }
}
