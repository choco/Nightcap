//! The settings window, which the shell pulls like the menu (ADR-0002): website checking and
//! opening at login, the checks, where the config is read from, and the Rules tab's rules and
//! what's on or off for now.

use std::path::Path;

use jiff::Timestamp;
use nightcap_protocol::{Command, Layer, OnOff, Place, Rule, RuleIdentity, RuleSource};

use crate::rows::{layer_text, until_text};
use crate::{LoginItem, SettingsCheck, State};

/// What the settings window shows, pulled like the menu: the shell only draws it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Settings {
    /// "Check websites", locked while the config sets `websites`.
    pub website_checking: SettingsSwitch,
    /// "Open at login", in self-managed installs only.
    pub open_at_login: Option<SettingsSwitch>,
    /// Shown inline, each under the setting it belongs to.
    pub checks: Vec<SettingsCheck>,
    /// Where the config is read from, whether or not it exists.
    pub config_path: String,
    /// Every rule: the config's, then the ad-hoc ones.
    pub rules: Vec<SettingsRule>,
    /// The apps and sites Night Shift is on or off in for now, with no rule of their own: a
    /// snoozed rule shows on its own row, and everywhere's on the menu's Night Shift item.
    pub for_now: Vec<SettingsLayers>,
}

/// An app's or site's temporary on or off as the Rules tab lists it, with ending it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsLayers {
    pub place: Place,
    /// "Night Shift off until 20:00".
    pub label: String,
    pub action: RuleAction,
}

/// A rule as the Rules tab lists it: what it says, its label and what can be done about it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsRule {
    pub identity: RuleIdentity,
    pub says: OnOff,
    /// "From your config", "Snoozed until 07:00" or "Ad-hoc".
    pub label: String,
    pub snoozed: bool,
    /// None for a config rule that isn't snoozed: only the config changes it.
    pub action: Option<RuleAction>,
}

/// Choosing it sends `command` back as `Event::CommandChosen`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RuleAction {
    pub title: String,
    pub command: Command,
}

/// Flipping it sends `command` back as `Event::CommandChosen`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsSwitch {
    pub on: bool,
    /// Set by the config, so Settings can't change it.
    pub locked: bool,
    pub command: Command,
}

impl Settings {
    /// Settings as `state` has them at `now`, with the config read from `config_path`.
    pub(crate) fn new(state: &State, now: Timestamp, config_path: &Path) -> Self {
        let look = state.places.look(&state.facts.world(now));
        let on = state.facts.website_checking();
        Settings {
            website_checking: SettingsSwitch {
                on,
                locked: state.facts.config_websites.is_some(),
                command: Command::Websites(OnOff::from(!on)),
            },
            config_path: config_path.display().to_string(),
            rules: look
                .rules
                .iter()
                .filter_map(|(rule, snooze)| state.settings_rule(rule, *snooze))
                .collect(),
            for_now: look
                .layers
                .iter()
                .filter(|layers| {
                    layers.place != Place::Everywhere
                        && !look
                            .rules
                            .iter()
                            .any(|(rule, _)| Place::from(rule.identity.clone()) == layers.place)
                })
                .map(|layers| SettingsLayers {
                    place: layers.place.clone(),
                    label: format!(
                        "Night Shift {}",
                        layer_text(
                            layers.timer.as_ref(),
                            layers.hold.as_ref(),
                            &state.facts.timeline.zone
                        )
                    ),
                    action: RuleAction {
                        title: "End".to_owned(),
                        command: Command::Reset(layers.place.clone()),
                    },
                })
                .collect(),
            // Registered but waiting for approval counts as on; its check explains the rest.
            open_at_login: state
                .grants
                .login_item
                .filter(|_| state.grants.opens_itself_at_login)
                .map(|item| {
                    let on = item != LoginItem::NotRegistered;
                    SettingsSwitch {
                        on,
                        locked: false,
                        command: Command::OpenAtLogin(OnOff::from(!on)),
                    }
                }),
            checks: state
                .checks(now)
                .into_iter()
                .map(|check| {
                    let (restore, accept) = state.fixes(&check);
                    SettingsCheck {
                        kind: check.kind,
                        state: check.state,
                        text: check.text,
                        restore,
                        accept,
                    }
                })
                .collect(),
        }
    }
}

impl State {
    /// A rule's row in the Rules tab: a snooze can be put back, an ad-hoc rule removed, and a
    /// config rule changed only in the config (ADR-0006). The tour's rules have none
    /// (ADR-0016).
    fn settings_rule(&self, rule: &Rule, snooze: Option<&Layer>) -> Option<SettingsRule> {
        let identity = rule.identity.clone();
        let place = Place::from(identity.clone());
        let action = |title: &str, command| {
            Some(RuleAction {
                title: title.to_owned(),
                command,
            })
        };
        let (label, action) = match (snooze, rule.source) {
            (_, RuleSource::Tour) => return None,
            (Some(layer), _) => (
                format!(
                    "Snoozed {}",
                    until_text(layer.until, layer.state, &self.facts.timeline.zone)
                ),
                action("Put Back", Command::Reset(place)),
            ),
            (None, RuleSource::Config) => ("From your config".to_owned(), None),
            (None, RuleSource::AdHoc) => (
                "Ad-hoc".to_owned(),
                action("Remove", Command::RemoveRule(identity.clone())),
            ),
        };
        Some(SettingsRule {
            identity,
            says: rule.says,
            label,
            snoozed: snooze.is_some(),
            action,
        })
    }
}
