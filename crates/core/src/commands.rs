//! Commands, from the app and the CLI alike, each answering with the decision it leads to, adding
//! a rule with what it did too, short of a reload and an update check; the rule commits they
//! make; and reading the config again, which is how a change to it takes effect.

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    CheckKind, CheckState, Command, FileError, Install, LoginItem, NightShift, OnOff, Payload,
    Place, Refusal, RuleAdded, RuleIdentity,
};

use crate::rules::places::Places;
use crate::rules::schedule::{CheckedSchedule, OutOfRange};
use crate::server::state::Inner;
use crate::storage::config;
use crate::storage::config::NightShiftConfig;
use crate::storage::document::Unusable;
use crate::storage::file::File;
use crate::{Cause, Effect, Event, Outcome};

impl Inner {
    /// Runs a command from the app or the CLI. The CLI, which waits on the answer, hears of a
    /// refusal there alone; the app, which doesn't, hears of it as an event, short of a reload's:
    /// the config file's check says why already.
    pub(crate) fn command(
        &mut self,
        command: Command,
        waited_on: bool,
    ) -> (Outcome, Result<Payload, Refusal>) {
        self.turn(Cause::Command, |inner, outcome| {
            let answer = inner.run_command(&command, &mut outcome.effects);
            if let Err(why) = &answer
                && !waited_on
                && command != Command::Reload
            {
                tracing::warn!("a command from the app was refused: {}", kind(why));
                outcome.events.push(Event::Refused {
                    command,
                    why: why.clone(),
                });
            }
            answer
        })
    }

    /// Answers a command with the resulting decision, and for adding a rule what it did; a
    /// reload answers with every rule, and an update check that it started.
    fn run_command(
        &mut self,
        command: &Command,
        effects: &mut Vec<Effect>,
    ) -> Result<Payload, Refusal> {
        let night_shift = self.facts.night_shift;
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
            return Err(Refusal::NightShiftUnavailable);
        }
        let now = self.clock.now();
        let mut added = None;
        let done = match command {
            Command::On => {
                self.places.switch(OnOff::On, &self.facts.world(now));
                Ok(())
            }
            Command::Off => {
                self.places.switch(OnOff::Off, &self.facts.world(now));
                Ok(())
            }
            Command::Toggle => {
                let world = self.facts.world(now);
                let to = (self.places.says(&Place::Everywhere, &world))
                    .unwrap_or(world.observed)
                    .other();
                self.places.switch(to, &world);
                Ok(())
            }
            Command::Set {
                place,
                state: on,
                length,
            } => self.places.set(place, *on, *length, &self.facts.world(now)),
            Command::Reset(place) => {
                self.places.reset(place);
                Ok(())
            }
            Command::AddRule { identity, says } => self
                .add_rule(identity, *says)
                .map(|rule| added = Some(rule)),
            Command::RemoveRule(identity) => self.remove_rule(identity),
            Command::OpenAtLogin(chosen) => match self.install {
                Install::SelfManaged => {
                    self.grants.open_at_login = *chosen == OnOff::On;
                    match chosen {
                        OnOff::On => effects.push(Effect::RegisterLoginItem),
                        // An item waiting for approval would still ask for it.
                        OnOff::Off if self.grants.login_item != Some(LoginItem::NotRegistered) => {
                            effects.push(Effect::UnregisterLoginItem);
                        }
                        OnOff::Off => {}
                    }
                    Ok(())
                }
                Install::NotSelfManaged(why) => Err(Refusal::NoLoginItem(why)),
            },
            Command::KeepNotificationsOff => {
                self.to_accept(CheckKind::Notifications, now).map(|()| {
                    tracing::info!("notifications kept off");
                    self.grants.notifications_wanted = false;
                })
            }
            Command::KeepScheduleOff => self.to_accept(CheckKind::NightShift, now).map(|()| {
                tracing::info!("the schedule kept off while sunset to sunrise is set aside");
                self.grants.schedule_off_accepted = true;
            }),
            Command::Strength(value) | Command::PreviewStrength(value) => {
                if self.strength.is_none() {
                    Err(Refusal::StrengthUnavailable)
                } else if *value > 100 {
                    Err(Refusal::StrengthOutOfRange { strength: *value })
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
                Err(OutOfRange(time)) => Err(Refusal::NotATimeOfDay(time)),
                Ok(_) => {
                    tracing::info!("schedule set on command");
                    self.reconciling.writing_schedule(*schedule, night_shift);
                    effects.push(Effect::SetSchedule {
                        schedule: *schedule,
                    });
                    Ok(())
                }
            },
            Command::Websites(chosen) => self.set_websites(*chosen == OnOff::On),
            Command::Reload => {
                return self
                    .reload(effects)
                    .map(|()| Payload::Rules(self.places.rules().rules));
            }
            Command::CheckForUpdates => {
                if let Install::NotSelfManaged(why) = self.install {
                    return Err(Refusal::NoUpdater(why));
                }
                tracing::info!("checking for updates");
                effects.push(Effect::CheckForUpdates);
                return Ok(Payload::UpdateCheckStarted);
            }
        };
        done.map(|()| {
            let decision = self.places.decide(&self.facts.world(now));
            match added {
                Some(rule) => Payload::RuleAdded { rule, decision },
                None => Payload::Decision(decision),
            }
        })
    }

    /// Refuses an accept while `check` doesn't drift at `now`: there's no change to take as the
    /// user's choice.
    fn to_accept(&self, check: CheckKind, now: Timestamp) -> Result<(), Refusal> {
        let drifts = self
            .checks(now)
            .iter()
            .any(|found| found.detail.kind() == check && found.state == CheckState::Drifted);
        if drifts {
            Ok(())
        } else {
            Err(Refusal::NothingToAccept { check })
        }
    }

    fn add_rule(&mut self, identity: &RuleIdentity, says: OnOff) -> Result<RuleAdded, Refusal> {
        let mut next = self.places.clone();
        let added = next.add_rule(identity, says)?;
        self.commit_rules(next)?;
        Ok(added)
    }

    fn remove_rule(&mut self, identity: &RuleIdentity) -> Result<(), Refusal> {
        let mut next = self.places.clone();
        next.remove_rule(identity)?;
        self.commit_rules(next)
    }

    /// Takes `next` as the places once `rules.toml` keeps the ad-hoc rules it has, if they
    /// changed.
    fn commit_rules(&mut self, next: Places) -> Result<(), Refusal> {
        if next.ad_hoc() != self.places.ad_hoc() {
            self.check_ad_hoc_rules_on()?;
            self.ad_hoc_store.save(&next.ad_hoc()).map_err(|error| {
                tracing::warn!("couldn't save rules.toml: {error}");
                Refusal::AdHocRulesNotSaved {
                    path: self.files.path(File::Rules).display().to_string(),
                    failure: error.kind().into(),
                }
            })?;
        }
        self.places = next;
        Ok(())
    }

    /// Takes the config's rules, dropping the ad-hoc rules it now has, which have graduated
    /// (ADR-0007), from `rules.toml` too. The config wins over them whether or not the file can
    /// be written, and the next load drops them again.
    pub(crate) fn graduate(&mut self, rules: Vec<(RuleIdentity, OnOff)>) {
        let before = self.places.ad_hoc().len();
        self.places.set_config(rules);
        let graduated = before - self.places.ad_hoc().len();
        if graduated > 0 {
            tracing::info!("{graduated} ad-hoc rules graduated into the config");
            if let Err(error) = self.ad_hoc_store.save(&self.places.ad_hoc()) {
                tracing::warn!("couldn't save rules.toml: {error}");
            }
        }
    }

    /// Loads the config again; an invalid one keeps the rules loaded before, and its check says
    /// why.
    pub(crate) fn reload(&mut self, effects: &mut Vec<Effect>) -> Result<(), Refusal> {
        match config::load(self.files.as_ref()) {
            Ok(config) => {
                tracing::info!("reloaded the config: {} rules", config.rules.len());
                self.facts.config_websites = config.websites;
                self.loaded.config_night_shift = config.night_shift;
                self.loaded.lengths = config.lengths;
                self.apply_config_night_shift(effects);
                self.loaded.config_error = None;
                self.loaded.config_found = config.found;
                self.graduate(config.rules);
                Ok(())
            }
            Err(error) => {
                tracing::warn!("keeping the rules loaded before: {error}");
                let refusal = Refusal::InvalidConfig(FileError::from(&error));
                self.loaded.config_error = Some(error);
                Err(refusal)
            }
        }
    }

    /// Website checking as the user chose it, unless the config sets it (ADR-0012).
    pub(crate) fn set_websites(&mut self, on: bool) -> Result<(), Refusal> {
        if self.facts.config_websites.is_some() {
            return Err(Refusal::WebsiteCheckingInConfig);
        }
        tracing::info!("website checking {}", OnOff::from(on));
        self.facts.websites = on;
        Ok(())
    }

    fn check_ad_hoc_rules_on(&self) -> Result<(), Refusal> {
        match &self.loaded.ad_hoc_error {
            None => Ok(()),
            Some(error) => Err(Refusal::AdHocRulesOff(error.into())),
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
    pub(crate) config_error: Option<Unusable>,
    /// Whether the config file existed when it was last loaded.
    pub(crate) config_found: bool,
    /// The lengths a change can be picked for, from the config or else 30 minutes, 1 hour and
    /// 2 hours.
    pub(crate) lengths: Vec<SignedDuration>,
    pub(crate) config_night_shift: NightShiftConfig,
    pub(crate) applied_from_config: NightShiftConfig,
    /// Why `rules.toml` can't be used; while set, ad-hoc rules are off and the file is left be.
    pub(crate) ad_hoc_error: Option<Unusable>,
}

/// A refusal as the log may name it: never the app, domain or path it carries (ADR-0009).
pub(crate) fn kind(refusal: &Refusal) -> &'static str {
    match refusal {
        Refusal::NightShiftUnavailable => "Night Shift unavailable",
        Refusal::StrengthUnavailable => "strength unavailable",
        Refusal::StrengthOutOfRange { .. } => "strength out of range",
        Refusal::NotATimeOfDay(_) => "custom hours out of range",
        Refusal::TooShort => "length too short",
        Refusal::TooLong { .. } => "length too long",
        Refusal::Passed => "time passed",
        Refusal::NoSchedule => "no schedule",
        Refusal::IndefiniteIsAdded => "indefinite rule is added",
        Refusal::ConfigSays { .. } => "the config says otherwise",
        Refusal::ConfigRule { .. } => "a config rule",
        Refusal::NoSuchRule { .. } => "no such rule",
        Refusal::AdHocRulesOff(_) => "ad-hoc rules off",
        Refusal::AdHocRulesNotSaved { .. } => "rules.toml not saved",
        Refusal::InvalidConfig(_) => "invalid config",
        Refusal::WebsiteCheckingInConfig => "website checking in the config",
        Refusal::NoUpdater(_) => "no updater",
        Refusal::NoLoginItem(_) => "no login item",
        Refusal::NothingToAccept { .. } => "nothing to accept",
    }
}
