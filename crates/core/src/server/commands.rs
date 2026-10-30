//! Commands, from the app and the CLI alike, each answering with the decision it leads to, adding
//! a rule with what it did too, short of a reload and an update check; the rule commits they
//! make; and reading the config again, which is how a change to it takes effect.

use jiff::Timestamp;
use nightcap_protocol::{
    CheckKind, CheckState, Command, Done, FileError, Install, LoginItem, NightShift, OnOff, Place,
    Refusal, RuleAdded, RuleIdentity,
};

use crate::Effect;
use crate::rules::Rules;
use crate::schedule::{CheckedSchedule, OutOfRange};
use crate::server::state::Inner;
use crate::storage::config;
use crate::storage::file::File;

impl Inner {
    /// Runs a command from the app or the CLI, decided the same whoever sent it, and answers it:
    /// what it did, or why the core refused it.
    pub(crate) fn command(
        &mut self,
        command: &Command,
        now: Timestamp,
    ) -> (Vec<Effect>, Result<Done, Refusal>) {
        match self.run_command(command, now) {
            Ok((done, effects)) => (effects, Ok(done)),
            Err(why) => {
                tracing::warn!("a command was refused: {}", kind(&why));
                (Vec::new(), Err(why))
            }
        }
    }

    /// Answers a command with the resulting decision, and for adding a rule what it did; a
    /// reload answers with every rule, and an update check that it started. Gives the effects
    /// the command calls for itself; a refused one calls for none.
    fn run_command(
        &mut self,
        command: &Command,
        now: Timestamp,
    ) -> Result<(Done, Vec<Effect>), Refusal> {
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
        let mut effects = Vec::new();
        let mut added = None;
        let done = match command {
            Command::On => {
                self.rules.switch(OnOff::On, &self.facts.world(now));
                Ok(())
            }
            Command::Off => {
                self.rules.switch(OnOff::Off, &self.facts.world(now));
                Ok(())
            }
            Command::Toggle => {
                let world = self.facts.world(now);
                let to = (self.rules.says(&Place::Everywhere, &world))
                    .unwrap_or(world.observed)
                    .other();
                self.rules.switch(to, &world);
                Ok(())
            }
            Command::Set {
                place,
                state: on,
                length,
            } => self.rules.set(place, *on, *length, &self.facts.world(now)),
            Command::Reset(place) => {
                self.rules.reset(place);
                Ok(())
            }
            Command::AddRule { identity, says } => self
                .add_rule(identity, *says)
                .map(|rule| added = Some(rule)),
            Command::RemoveRule(identity) => self.remove_rule(identity),
            Command::OpenAtLogin(chosen) => match self.install {
                Install::SelfManaged => {
                    self.setup.open_at_login = *chosen == OnOff::On;
                    match chosen {
                        OnOff::On => effects.push(Effect::RegisterLoginItem),
                        // An item waiting for approval would still ask for it.
                        OnOff::Off if self.setup.login_item != Some(LoginItem::NotRegistered) => {
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
                    self.setup.notifications_wanted = false;
                })
            }
            Command::KeepScheduleOff => self.to_accept(CheckKind::NightShift, now).map(|()| {
                tracing::info!("the schedule kept off while sunset to sunrise is set aside");
                self.setup.schedule_off_accepted = true;
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
                    effects.push(Effect::SetSchedule {
                        schedule: *schedule,
                    });
                    Ok(())
                }
            },
            Command::Websites(chosen) => self.set_websites(*chosen == OnOff::On),
            Command::Reload => {
                let effects = self.reload()?;
                return Ok((Done::Rules(self.rules.rules().rules), effects));
            }
            Command::CheckForUpdates => {
                if let Install::NotSelfManaged(why) = self.install {
                    return Err(Refusal::NoUpdater(why));
                }
                tracing::info!("checking for updates");
                return Ok((Done::UpdateCheckStarted, vec![Effect::CheckForUpdates]));
            }
        };
        done.map(|()| {
            let decision = self.rules.decide(&self.facts.world(now));
            let done = match added {
                Some(rule) => Done::RuleAdded { rule, decision },
                None => Done::Decision(decision),
            };
            (done, effects)
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
        let mut next = self.rules.clone();
        let added = next.add_rule(identity, says)?;
        self.commit_rules(next)?;
        Ok(added)
    }

    fn remove_rule(&mut self, identity: &RuleIdentity) -> Result<(), Refusal> {
        let mut next = self.rules.clone();
        next.remove_rule(identity)?;
        self.commit_rules(next)
    }

    /// Takes `next` as the places once `rules.toml` keeps the ad-hoc rules it has, if they
    /// changed.
    fn commit_rules(&mut self, next: Rules) -> Result<(), Refusal> {
        if next.ad_hoc() != self.rules.ad_hoc() {
            self.check_ad_hoc_rules_on()?;
            self.ad_hoc_store.save(&next.ad_hoc()).map_err(|error| {
                tracing::warn!("couldn't save rules.toml: {error}");
                Refusal::AdHocRulesNotSaved {
                    path: self.files.path(File::Rules).display().to_string(),
                    failure: error.kind().into(),
                }
            })?;
        }
        self.rules = next;
        Ok(())
    }

    /// Takes the config's rules, dropping the ad-hoc rules it now has, which have graduated
    /// (ADR-0007), from `rules.toml` too. The config wins over them whether or not the file can
    /// be written, and the next load drops them again.
    pub(crate) fn graduate(&mut self, rules: Vec<(RuleIdentity, OnOff)>) {
        let before = self.rules.ad_hoc().len();
        self.rules.set_config(rules);
        let graduated = before - self.rules.ad_hoc().len();
        if graduated > 0 {
            tracing::info!("{graduated} ad-hoc rules graduated into the config");
            if let Err(error) = self.ad_hoc_store.save(&self.rules.ad_hoc()) {
                tracing::warn!("couldn't save rules.toml: {error}");
            }
        }
    }

    /// Loads the config again, with the writes its `[night_shift]` calls for; an invalid one
    /// keeps the rules loaded before, and its check says why.
    pub(crate) fn reload(&mut self) -> Result<Vec<Effect>, Refusal> {
        match config::load(self.files.as_ref()) {
            Ok(config) => {
                tracing::info!("reloaded the config: {} rules", config.rules.len());
                self.facts.config_websites = config.websites;
                self.loaded.config_night_shift = config.night_shift;
                self.loaded.lengths = config.lengths;
                let effects = self.apply_config_night_shift();
                self.loaded.config_error = None;
                self.loaded.config_found = config.found;
                self.graduate(config.rules);
                Ok(effects)
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
    pub(crate) fn apply_config_night_shift(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.facts.night_shift == NightShift::Unavailable {
            return effects;
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
        effects
    }
}

/// A refusal as the log may name it: never the app, domain or path it carries (ADR-0009).
fn kind(refusal: &Refusal) -> &'static str {
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
