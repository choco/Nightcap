//! The settings window, which the shell reads from the snapshot (ADR-0002): website checking and
//! opening at login, the checks, where the config is read from, and the Rules tab's rules and
//! what's on or off for now.

use std::path::Path;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{Command, OnOff, Place, Rule, RuleIdentity, SetBy};

use crate::rules::places::{is_app_or_site_rule, place_identity};
use crate::server::state::Inner;
use crate::{LoginItem, SettingsCheck};

/// What the settings window shows, read from the snapshot: the shell only draws it.
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
    /// The apps' and sites' indefinite rules: the config's, then the ad-hoc ones.
    pub rules: Vec<SettingsRule>,
    /// The apps and sites Night Shift is on or off in for now, by temporary rules alone: a
    /// snoozed rule shows on its own row, and everywhere's on the menu's Night Shift item.
    pub for_now: Vec<SettingsForNow>,
}

/// An app's or site's temporary rules as the Rules tab lists them, with ending them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsForNow {
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

/// Choosing it sends `command` back with `Core::send`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RuleAction {
    pub title: String,
    pub command: Command,
}

/// Flipping it sends `command` back with `Core::send`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsSwitch {
    pub on: bool,
    /// Set by the config, so Settings can't change it.
    pub locked: bool,
    pub command: Command,
}

impl Settings {
    /// Settings as `inner` has them at `now`, with the config read from `config_path`.
    pub(crate) fn new(inner: &Inner, now: Timestamp, config_path: &Path) -> Self {
        let look = inner.places.look(&inner.facts.world(now));
        let on = inner.facts.website_checking();
        Settings {
            website_checking: SettingsSwitch {
                on,
                locked: inner.facts.config_websites.is_some(),
                command: Command::Websites(OnOff::from(!on)),
            },
            config_path: config_path.display().to_string(),
            rules: look
                .rules
                .iter()
                .filter(|(rule, _)| is_app_or_site_rule(rule))
                .filter_map(|(rule, snooze)| inner.settings_rule(rule, *snooze))
                .collect(),
            for_now: for_now(&look.rules)
                .into_iter()
                .map(|(place, rules)| SettingsForNow {
                    label: format!(
                        "Night Shift {}",
                        rules_text(&rules, &inner.facts.timeline.zone)
                    ),
                    action: RuleAction {
                        title: "End".to_owned(),
                        command: Command::Reset(place.clone()),
                    },
                    place,
                })
                .collect(),
            // Registered but waiting for approval counts as on; its check explains the rest.
            open_at_login: inner
                .grants
                .login_item
                .filter(|_| inner.manages_itself)
                .map(|item| {
                    let on = item != LoginItem::NotRegistered;
                    SettingsSwitch {
                        on,
                        locked: false,
                        command: Command::OpenAtLogin(OnOff::from(!on)),
                    }
                }),
            checks: inner.settings_checks(now),
        }
    }
}

impl Inner {
    /// A rule's row in the Rules tab: a snooze can be put back, an ad-hoc rule removed, and a
    /// config rule changed only in the config (ADR-0006). The tour's rules have none
    /// (ADR-0016).
    fn settings_rule(&self, rule: &Rule, snooze: Option<&Rule>) -> Option<SettingsRule> {
        let identity = place_identity(&rule.place)?;
        let action = |title: &str, command| {
            Some(RuleAction {
                title: title.to_owned(),
                command,
            })
        };
        let (label, action) = match (snooze, rule.set_by) {
            (_, SetBy::Tour | SetBy::OutsideChange) => return None,
            (Some(snooze), _) => (
                format!(
                    "Snoozed {}",
                    until_text(snooze.length.end(), snooze.says, &self.facts.timeline.zone)
                ),
                action("Put Back", Command::Reset(rule.place.clone())),
            ),
            (None, SetBy::Config) => ("From your config".to_owned(), None),
            (None, SetBy::You) => (
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

/// The apps and sites with temporary rules and no indefinite one, each with its rules, shortest
/// first, in the order the places got their first.
fn for_now(rules: &[(&Rule, Option<&Rule>)]) -> Vec<(Place, Vec<Rule>)> {
    let mut places: Vec<(Place, Vec<Rule>)> = Vec::new();
    for (rule, _) in rules {
        let ruled = rules
            .iter()
            .any(|(other, _)| other.place == rule.place && is_app_or_site_rule(other));
        if rule.place == Place::Everywhere || ruled {
            continue;
        }
        match places.iter_mut().find(|(place, _)| *place == rule.place) {
            Some((_, kept)) => kept.push((*rule).clone()),
            None => places.push((rule.place.clone(), vec![(*rule).clone()])),
        }
    }
    places
}

/// A place's rules, shortest first: "on until 23:30, then off until 07:00".
fn rules_text(rules: &[Rule], zone: &TimeZone) -> String {
    rules
        .iter()
        .map(|rule| {
            format!(
                "{} {}",
                rule.says,
                until_text(rule.length.end(), rule.says, zone)
            )
        })
        .collect::<Vec<_>>()
        .join(", then ")
}

/// "until 07:00", or "until you turn it on" for an off with no end.
fn until_text(until: Option<Timestamp>, state: OnOff, zone: &TimeZone) -> String {
    match until {
        Some(until) => format!("until {}", until.to_zoned(zone.clone()).strftime("%H:%M")),
        None => format!("until you turn it {}", state.other()),
    }
}
