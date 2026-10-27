//! The CLI's own words for what Nightcap reports, which sends facts alone: its decisions, its
//! rules and its checks.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, CheckDetail, ConfigFileDetail, Decision, FileError,
    NightShiftDetail, NotificationsDetail, OnOff, OpenAtLoginDetail, Place, Reason, Rule,
    RuleIdentity, RuleSource, Schedule, TimeOfDay,
};

/// What Nightcap decided and why, as it reads after "Night Shift", with times in `zone`.
pub(crate) fn decision(decision: &Decision, zone: &TimeZone) -> String {
    let until = |end: Option<Timestamp>, state: OnOff| match end {
        Some(end) => format!(" until {}", end.to_zoned(zone.clone()).strftime("%H:%M")),
        None => format!(" until you turn it {}", state.other()),
    };
    let Some(top) = decision.reasons.first() else {
        return "left alone".to_owned();
    };
    let at = match &decision.place {
        Place::Everywhere => String::new(),
        place => format!(" at {place}"),
    };
    let mut description = match top {
        Reason::Timer(layer) | Reason::Hold(layer) => {
            format!("{}{at}{}", layer.state, until(layer.until, layer.state))
        }
        Reason::Rule(rule) => format!(
            "{} by the {} rule for {}",
            rule.says,
            source(rule.source),
            rule.identity
        ),
        Reason::Tour => "on for the tour".to_owned(),
        Reason::Schedule { .. } => "following the schedule".to_owned(),
    };
    let snoozed = decision
        .reasons
        .iter()
        .skip(1)
        .find_map(|reason| match reason {
            Reason::Rule(rule) if Some(rule.says) != decision.state => Some(rule),
            _ => None,
        });
    if let Some(rule) = snoozed {
        description.push_str(&format!(", its {} rule snoozed", source(rule.source)));
    }
    description
}

/// `app:com.figma.Desktop (config rule, off)`.
pub(crate) fn rule(rule: &Rule) -> String {
    format!(
        "{} ({} rule, {})",
        rule.identity,
        source(rule.source),
        rule.says
    )
}

/// What adding a rule for `identity` that says `says` did, given the rule Nightcap had for the
/// same app or domain just before: added one, changed the user's own, or left one that already
/// says so. A config rule that says otherwise is refused before this is said.
pub(crate) fn rule_added(identity: &RuleIdentity, says: OnOff, before: Option<&Rule>) -> String {
    match before {
        None => format!("Added {identity}, which turns Night Shift {says}."),
        Some(rule) if rule.says == says => format!(
            "The {} rule for {identity} already turns Night Shift {says}.",
            source(rule.source)
        ),
        Some(rule) => format!(
            "Changed the {} rule for {identity} to turn Night Shift {says}.",
            source(rule.source)
        ),
    }
}

fn source(source: RuleSource) -> &'static str {
    match source {
        RuleSource::Config => "config",
        RuleSource::AdHoc => "ad-hoc",
        RuleSource::Tour => "tour",
    }
}

/// What a check found, in a sentence.
pub(crate) fn check(detail: &CheckDetail) -> String {
    match detail {
        CheckDetail::NightShift(found) => night_shift(found),
        CheckDetail::Accessibility(found) => match found {
            AccessibilityDetail::Allowed => "Accessibility is allowed.",
            AccessibilityDetail::NotAllowed => {
                "Accessibility isn’t allowed, so website checking can’t read the front tab."
            }
            AccessibilityDetail::NotNeeded => {
                "Accessibility isn’t needed while website checking is off."
            }
        }
        .to_owned(),
        CheckDetail::Notifications(found) => match found {
            NotificationsDetail::Allowed => "Notifications are allowed.",
            NotificationsDetail::SwitchedOff => {
                "Notifications were allowed, but they’re switched off in System Settings."
            }
            NotificationsDetail::Off => "Notifications are off.",
        }
        .to_owned(),
        CheckDetail::OpenAtLogin(found) => match found {
            OpenAtLoginDetail::Opens => "Nightcap opens when you log in.",
            OpenAtLoginDetail::AwaitingApproval => {
                "macOS is waiting for your approval in Login Items."
            }
            OpenAtLoginDetail::Removed => {
                "Nightcap no longer opens at login: it was removed in System Settings."
            }
            OpenAtLoginDetail::Off => "Nightcap doesn’t open at login.",
        }
        .to_owned(),
        CheckDetail::ConfigFile(found) => match found {
            ConfigFileDetail::Valid { rules: 0 } => {
                "The config file is valid, with no rules.".to_owned()
            }
            ConfigFileDetail::Valid { rules: 1 } => {
                "The config file is valid, with 1 rule.".to_owned()
            }
            ConfigFileDetail::Valid { rules } => {
                format!("The config file is valid, with {rules} rules.")
            }
            ConfigFileDetail::Missing => "There’s no config file.".to_owned(),
            ConfigFileDetail::Error(error) => {
                format!("The config file is invalid: {}", file_error(error))
            }
        },
        CheckDetail::AdHocRules(found) => match found {
            AdHocRulesDetail::On { rules: 0 } => "No ad-hoc rules.".to_owned(),
            AdHocRulesDetail::On { rules: 1 } => "1 ad-hoc rule.".to_owned(),
            AdHocRulesDetail::On { rules } => format!("{rules} ad-hoc rules."),
            AdHocRulesDetail::Off(error) => {
                format!("Ad-hoc rules are off: {}.", file_error(error))
            }
        },
    }
}

/// Night Shift's check, in a sentence.
fn night_shift(found: &NightShiftDetail) -> String {
    let time = |at: TimeOfDay| format!("{:02}:{:02}", at.hour, at.minute);
    match found {
        NightShiftDetail::Available { schedule } => match schedule {
            None => "Night Shift is available.".to_owned(),
            Some(Schedule::Off) => "Night Shift is available, with no schedule.".to_owned(),
            Some(Schedule::SunsetToSunrise) => {
                "Night Shift is available, following its schedule: sunset to sunrise.".to_owned()
            }
            Some(Schedule::Custom { from, to }) => format!(
                "Night Shift is available, following its schedule: {} to {}.",
                time(*from),
                time(*to)
            ),
        },
        NightShiftDetail::Unavailable => "Night Shift isn’t available on this Mac.".to_owned(),
        NightShiftDetail::SetAside => {
            "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on."
                .to_owned()
        }
        NightShiftDetail::NoSunTimes { .. } => {
            "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. Custom hours work without them."
                .to_owned()
        }
    }
}

/// What's wrong with a file, starting from its path: "/…/config.toml: line 4: unknown field".
fn file_error(error: &FileError) -> String {
    match error {
        FileError::Unreadable { path, message } => format!("couldn't read {path}: {message}"),
        FileError::Malformed {
            path,
            line: Some(line),
            message,
        }
        | FileError::Invalid {
            path,
            line: Some(line),
            message,
        } => format!("{path}: line {line}: {message}"),
        FileError::Malformed {
            path,
            line: None,
            message,
        }
        | FileError::Invalid {
            path,
            line: None,
            message,
        } => format!("{path}: {message}"),
        FileError::Newer { path, version } => {
            format!("{path} is version {version}, from a newer Nightcap than this one")
        }
    }
}

#[cfg(test)]
mod tests {
    use nightcap_protocol::Layer;

    use super::*;

    #[test]
    fn decisions_read_after_night_shift() {
        let zone = TimeZone::get("Europe/Zurich").unwrap();
        let at = |instant: &str| instant.parse::<Timestamp>().unwrap();
        let figma_app = || Place::App("com.figma.Desktop".parse().unwrap());
        let figma = Rule {
            identity: RuleIdentity::App("com.figma.Desktop".parse().unwrap()),
            source: RuleSource::Config,
            says: OnOff::Off,
        };
        let layer = |state, until| Layer {
            state,
            until,
            with: None,
        };
        let described = |state, place, reasons| {
            decision(
                &Decision {
                    state: Some(state),
                    place,
                    reasons,
                },
                &zone,
            )
        };

        assert_eq!(
            described(
                OnOff::Off,
                Place::Everywhere,
                vec![
                    Reason::Timer(layer(OnOff::Off, Some(at("2026-10-16T22:00:00Z")))),
                    Reason::Schedule {
                        state: OnOff::On,
                        until: None
                    },
                ]
            ),
            "off until 00:00"
        );
        assert_eq!(
            described(
                OnOff::Off,
                Place::Everywhere,
                vec![Reason::Hold(layer(OnOff::Off, None))]
            ),
            "off until you turn it on"
        );
        assert_eq!(
            described(
                OnOff::On,
                figma_app(),
                vec![
                    Reason::Hold(layer(OnOff::On, Some(at("2026-10-17T05:00:00Z")))),
                    Reason::Rule(figma.clone()),
                ]
            ),
            "on at app:com.figma.Desktop until 07:00, its config rule snoozed"
        );
        assert_eq!(
            described(OnOff::Off, figma_app(), vec![Reason::Rule(figma)]),
            "off by the config rule for app:com.figma.Desktop"
        );
        assert_eq!(
            described(
                OnOff::On,
                Place::Domain("figma.com".parse().unwrap()),
                vec![Reason::Rule(Rule {
                    identity: RuleIdentity::Domain("figma.com".parse().unwrap()),
                    source: RuleSource::AdHoc,
                    says: OnOff::On,
                })]
            ),
            "on by the ad-hoc rule for domain:figma.com"
        );
        assert_eq!(
            described(OnOff::On, Place::Everywhere, vec![Reason::Tour]),
            "on for the tour"
        );
        assert_eq!(
            described(
                OnOff::On,
                Place::Everywhere,
                vec![Reason::Schedule {
                    state: OnOff::On,
                    until: None
                }]
            ),
            "following the schedule"
        );
        assert_eq!(
            decision(
                &Decision {
                    state: None,
                    place: Place::Everywhere,
                    reasons: Vec::new(),
                },
                &zone
            ),
            "left alone"
        );
    }

    #[test]
    fn a_rule_reads_with_its_source_and_what_it_says() {
        let worded = |identity: &str, source, says| {
            rule(&Rule {
                identity: identity.parse().unwrap(),
                source,
                says,
            })
        };

        assert_eq!(
            worded("app:com.figma.Desktop", RuleSource::Config, OnOff::Off),
            "app:com.figma.Desktop (config rule, off)"
        );
        assert_eq!(
            worded("domain:figma.com", RuleSource::AdHoc, OnOff::On),
            "domain:figma.com (ad-hoc rule, on)"
        );
        assert_eq!(
            worded(
                "app:io.github.choco.nightcap.TourSample",
                RuleSource::Tour,
                OnOff::Off
            ),
            "app:io.github.choco.nightcap.TourSample (tour rule, off)"
        );
    }

    #[test]
    fn night_shifts_check_says_whether_it_works_and_why_the_schedule_cant_run() {
        let worded = |found| check(&CheckDetail::NightShift(found));
        let available = |schedule| worded(NightShiftDetail::Available { schedule });

        assert_eq!(available(None), "Night Shift is available.");
        assert_eq!(
            available(Some(Schedule::Off)),
            "Night Shift is available, with no schedule."
        );
        assert_eq!(
            available(Some(Schedule::SunsetToSunrise)),
            "Night Shift is available, following its schedule: sunset to sunrise."
        );
        assert_eq!(
            available(Some(Schedule::Custom {
                from: TimeOfDay {
                    hour: 22,
                    minute: 0
                },
                to: TimeOfDay { hour: 7, minute: 5 },
            })),
            "Night Shift is available, following its schedule: 22:00 to 07:05."
        );
        assert_eq!(
            worded(NightShiftDetail::Unavailable),
            "Night Shift isn’t available on this Mac."
        );
        assert_eq!(
            worded(NightShiftDetail::SetAside),
            "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on."
        );
        assert_eq!(
            worded(NightShiftDetail::NoSunTimes {
                waited: jiff::SignedDuration::from_mins(12)
            }),
            "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. Custom hours work without them."
        );
    }

    #[test]
    fn the_grants_checks_say_whether_each_is_allowed_and_wanted() {
        assert_eq!(
            check(&CheckDetail::Accessibility(AccessibilityDetail::Allowed)),
            "Accessibility is allowed."
        );
        assert_eq!(
            check(&CheckDetail::Accessibility(AccessibilityDetail::NotAllowed)),
            "Accessibility isn’t allowed, so website checking can’t read the front tab."
        );
        assert_eq!(
            check(&CheckDetail::Accessibility(AccessibilityDetail::NotNeeded)),
            "Accessibility isn’t needed while website checking is off."
        );
        assert_eq!(
            check(&CheckDetail::Notifications(NotificationsDetail::Allowed)),
            "Notifications are allowed."
        );
        assert_eq!(
            check(&CheckDetail::Notifications(
                NotificationsDetail::SwitchedOff
            )),
            "Notifications were allowed, but they’re switched off in System Settings."
        );
        assert_eq!(
            check(&CheckDetail::Notifications(NotificationsDetail::Off)),
            "Notifications are off."
        );
        assert_eq!(
            check(&CheckDetail::OpenAtLogin(OpenAtLoginDetail::Opens)),
            "Nightcap opens when you log in."
        );
        assert_eq!(
            check(&CheckDetail::OpenAtLogin(
                OpenAtLoginDetail::AwaitingApproval
            )),
            "macOS is waiting for your approval in Login Items."
        );
        assert_eq!(
            check(&CheckDetail::OpenAtLogin(OpenAtLoginDetail::Removed)),
            "Nightcap no longer opens at login: it was removed in System Settings."
        );
        assert_eq!(
            check(&CheckDetail::OpenAtLogin(OpenAtLoginDetail::Off)),
            "Nightcap doesn’t open at login."
        );
    }

    #[test]
    fn the_files_checks_count_the_rules_or_say_what_is_wrong_and_where() {
        let config = |found| check(&CheckDetail::ConfigFile(found));
        let ad_hoc = |found| check(&CheckDetail::AdHocRules(found));
        let path = || "/Users/me/.config/nightcap/config.toml".to_owned();
        let invalid = |error| config(ConfigFileDetail::Error(error));

        assert_eq!(
            config(ConfigFileDetail::Valid { rules: 0 }),
            "The config file is valid, with no rules."
        );
        assert_eq!(
            config(ConfigFileDetail::Valid { rules: 1 }),
            "The config file is valid, with 1 rule."
        );
        assert_eq!(
            config(ConfigFileDetail::Valid { rules: 3 }),
            "The config file is valid, with 3 rules."
        );
        assert_eq!(config(ConfigFileDetail::Missing), "There’s no config file.");
        assert_eq!(
            invalid(FileError::Unreadable {
                path: path(),
                message: "Permission denied (os error 13)".to_owned(),
            }),
            "The config file is invalid: couldn't read /Users/me/.config/nightcap/config.toml: Permission denied (os error 13)"
        );
        assert_eq!(
            invalid(FileError::Malformed {
                path: path(),
                line: Some(4),
                message: "unknown field `domian`, expected `app` or `domain`".to_owned(),
            }),
            "The config file is invalid: /Users/me/.config/nightcap/config.toml: line 4: unknown field `domian`, expected `app` or `domain`"
        );
        assert_eq!(
            invalid(FileError::Malformed {
                path: path(),
                line: None,
                message: "expected a table".to_owned(),
            }),
            "The config file is invalid: /Users/me/.config/nightcap/config.toml: expected a table"
        );
        assert_eq!(
            invalid(FileError::Invalid {
                path: path(),
                line: Some(2),
                message: "night_shift.strength is 0 to 100, not 101".to_owned(),
            }),
            "The config file is invalid: /Users/me/.config/nightcap/config.toml: line 2: night_shift.strength is 0 to 100, not 101"
        );
        assert_eq!(
            invalid(FileError::Invalid {
                path: path(),
                line: None,
                message: "night_shift.strength is 0 to 100, not 101".to_owned(),
            }),
            "The config file is invalid: /Users/me/.config/nightcap/config.toml: night_shift.strength is 0 to 100, not 101"
        );
        assert_eq!(
            ad_hoc(AdHocRulesDetail::On { rules: 0 }),
            "No ad-hoc rules."
        );
        assert_eq!(ad_hoc(AdHocRulesDetail::On { rules: 1 }), "1 ad-hoc rule.");
        assert_eq!(ad_hoc(AdHocRulesDetail::On { rules: 2 }), "2 ad-hoc rules.");
        assert_eq!(
            ad_hoc(AdHocRulesDetail::Off(FileError::Newer {
                path: "/Users/me/.local/share/nightcap/rules.toml".to_owned(),
                version: 3,
            })),
            "Ad-hoc rules are off: /Users/me/.local/share/nightcap/rules.toml is version 3, from a newer Nightcap than this one."
        );
    }

    #[test]
    fn adding_a_rule_says_whether_it_was_added_changed_or_already_there() {
        let figma: RuleIdentity = "app:com.figma.Desktop".parse().unwrap();
        let there = |source, says| Rule {
            identity: "app:com.figma.desktop".parse().unwrap(),
            source,
            says,
        };

        assert_eq!(
            rule_added(&figma, OnOff::Off, None),
            "Added app:com.figma.Desktop, which turns Night Shift off."
        );
        assert_eq!(
            rule_added(
                &figma,
                OnOff::Off,
                Some(&there(RuleSource::Config, OnOff::Off))
            ),
            "The config rule for app:com.figma.Desktop already turns Night Shift off."
        );
        assert_eq!(
            rule_added(
                &figma,
                OnOff::On,
                Some(&there(RuleSource::AdHoc, OnOff::On))
            ),
            "The ad-hoc rule for app:com.figma.Desktop already turns Night Shift on."
        );
        assert_eq!(
            rule_added(
                &figma,
                OnOff::On,
                Some(&there(RuleSource::AdHoc, OnOff::Off))
            ),
            "Changed the ad-hoc rule for app:com.figma.Desktop to turn Night Shift on."
        );
    }
}
