//! The CLI's own words for what Nightcap reports, which sends facts alone: its decisions, its
//! rules, its checks and the status its queries make up.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, CheckDetail, CheckState, Checks, ConfigFileDetail,
    Decision, FileError, Install, Lasting, Length, NightShift, NightShiftDetail,
    NotificationPermission, NotificationsDetail, OnOff, OpenAtLoginDetail, Place, Reason, Rule,
    RuleIdentity, Schedule, SetBy, TimeOfDay,
};

use crate::client::Report;

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
        Reason::Rule(rule) if is_app_or_site_rule(rule) => format!(
            "{} by the {} rule for {}",
            rule.says,
            source(rule.set_by),
            rule.place
        ),
        Reason::Rule(rule) => format!("{}{at}{}", rule.says, until(rule.length.end(), rule.says)),
        Reason::Tour => "on for the tour".to_owned(),
        Reason::Schedule { .. } => "following the schedule".to_owned(),
    };
    let snoozed = decision
        .reasons
        .iter()
        .skip(1)
        .find_map(|reason| match reason {
            Reason::Rule(rule)
                if is_app_or_site_rule(rule) && Some(rule.says) != decision.state =>
            {
                Some(rule)
            }
            _ => None,
        });
    if let Some(rule) = snoozed {
        description.push_str(&format!(", its {} rule snoozed", source(rule.set_by)));
    }
    description
}

/// `app:com.figma.Desktop (config rule, off)`.
pub(crate) fn rule(rule: &Rule) -> String {
    format!(
        "{} ({} rule, {})",
        rule.place,
        source(rule.set_by),
        rule.says
    )
}

/// The apps' and sites' indefinite rules, the ones `nightcap rules` lists: the config's, the
/// user's and the tour's.
pub(crate) fn app_and_site_rules(rules: &[Rule]) -> Vec<Rule> {
    rules
        .iter()
        .filter(|rule| is_app_or_site_rule(rule))
        .cloned()
        .collect()
}

fn is_app_or_site_rule(rule: &Rule) -> bool {
    rule.place != Place::Everywhere && rule.length == Length::Indefinite
}

/// What adding a rule for `identity` that says `says` did, given the rule Nightcap had for the
/// same app or domain just before: added one, changed the user's own, or left one that already
/// says so. A config rule that says otherwise is refused before this is said.
pub(crate) fn rule_added(identity: &RuleIdentity, says: OnOff, before: Option<&Rule>) -> String {
    match before {
        None => format!("Added {identity}, which turns Night Shift {says}."),
        Some(rule) if rule.says == says => format!(
            "The {} rule for {identity} already turns Night Shift {says}.",
            source(rule.set_by)
        ),
        Some(rule) => format!(
            "Changed the {} rule for {identity} to turn Night Shift {says}.",
            source(rule.set_by)
        ),
    }
}

fn source(set_by: SetBy) -> &'static str {
    match set_by {
        SetBy::Config => "config",
        SetBy::You => "ad-hoc",
        SetBy::Tour => "tour",
        SetBy::OutsideChange => "adopted",
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

fn schedule_text(schedule: Schedule) -> String {
    match schedule {
        Schedule::Off => "off".to_owned(),
        Schedule::SunsetToSunrise => "sunset to sunrise".to_owned(),
        Schedule::Custom { from, to } => format!("{} to {}", time_of_day(from), time_of_day(to)),
    }
}

/// The config's value, when it sets one that differs from the Mac's (ADR-0014).
fn from_config<T: PartialEq + Copy>(
    config: Option<T>,
    mac: Option<T>,
    text: impl Fn(T) -> String,
) -> String {
    match config {
        Some(value) if Some(value) != mac => format!(" (the config says {})", text(value)),
        _ => String::new(),
    }
}

/// How long a change lasts, as `on` and `off` were told.
pub(crate) fn length_text(length: &Lasting, zone: &TimeZone) -> String {
    match length {
        Lasting::For(duration) => format!("for {duration:#}"),
        Lasting::Until(end) => format!("until {}", end.to_zoned(zone.clone()).strftime("%H:%M")),
        Lasting::UntilScheduleTurns => "until the schedule turns it back".to_owned(),
        Lasting::Indefinite => "until you turn it back".to_owned(),
    }
}

pub(crate) fn print_status(report: &Report) {
    let Report {
        about,
        night_shift,
        front,
        rules: every_rule,
        config,
        preferences,
        checks,
    } = report;
    let install = match about.install {
        Install::Managed => "managed by its installer",
        Install::SelfManaged => "self-managed",
    };
    let state = match night_shift.state {
        NightShift::Unavailable => "unavailable",
        NightShift::On => "on",
        NightShift::Off => "off",
    };
    let schedule = night_shift
        .schedule
        .map_or("unknown".to_owned(), schedule_text);
    let scheduled = match (night_shift.schedule, night_shift.scheduled) {
        (Some(Schedule::Off), _) | (None, _) => String::new(),
        (Some(_), None) => ", waiting for sun times".to_owned(),
        (Some(_), Some(scheduled)) => {
            format!(
                ", {}{}",
                scheduled,
                until(night_shift.next_turn.map(|turn| turn.at))
            )
        }
    };
    let accessibility = if checks.permissions.accessibility {
        "granted"
    } else {
        "not granted"
    };
    let notifications = match checks.permissions.notifications {
        NotificationPermission::NotRequested => "not requested",
        NotificationPermission::Denied => "denied",
        NotificationPermission::Granted => "granted",
    };
    println!("Nightcap {}, {install}", about.version);
    let decided = decision(&front.decision, &TimeZone::system());
    // A decision on a rule starts with the state it asks for, which needs saying again
    // only while Night Shift isn't there yet.
    let says_state = !matches!(
        front.decision.reasons.first(),
        None | Some(Reason::Schedule { .. })
    );
    let observed = match night_shift.state {
        NightShift::Unavailable => None,
        NightShift::On => Some(OnOff::On),
        NightShift::Off => Some(OnOff::Off),
    };
    if says_state
        && front
            .decision
            .state
            .is_some_and(|state| Some(state) == observed)
    {
        println!("Night Shift: {decided}");
    } else {
        println!("Night Shift: {state}, {decided}");
    }
    println!(
        "Schedule: {schedule}{scheduled}{}",
        from_config(config.schedule, night_shift.schedule, schedule_text)
    );
    if let Some(strength) = night_shift.strength {
        println!(
            "Strength: {strength}{}",
            from_config(config.strength, night_shift.strength, |value| value
                .to_string())
        );
    }
    let host = match (&front.host, front.watching) {
        (Some(host), _) => format!(", on {host}"),
        (None, true) => ", host unknown".to_owned(),
        (None, false) => String::new(),
    };
    println!(
        "Frontmost app: {}{host}",
        front.app.as_ref().map_or("unknown", |app| app.as_str())
    );
    println!("Rules: {}", rules(&app_and_site_rules(every_rule)));
    let website_checking = preferences.website_checking;
    println!(
        "Website checking: {}{}",
        website_checking.state,
        if website_checking.from_config {
            " (set in the config)"
        } else {
            ""
        }
    );
    for line in places_rules(every_rule) {
        println!("{line}");
    }
    println!("Accessibility: {accessibility}");
    println!("Notifications: {notifications}");
    let problems = needs_attention(checks);
    if !problems.is_empty() {
        println!("Needs attention:");
        for problem in problems {
            println!("  {problem}");
        }
    }
}

/// Each drifted or broken check's line.
fn needs_attention(checks: &Checks) -> Vec<String> {
    checks
        .checks
        .iter()
        .filter(|check| matches!(check.state, CheckState::Drifted | CheckState::Broken))
        .map(|check| self::check(&check.detail))
        .collect()
}

pub(crate) fn rules(rules: &[Rule]) -> String {
    if rules.is_empty() {
        return "none".to_owned();
    }
    rules.iter().map(rule).collect::<Vec<_>>().join(", ")
}

/// Each place's rules but an app's or site's indefinite one, shortest first, a line per place:
/// "app:com.figma.Desktop: on until 07:00".
fn places_rules(rules: &[Rule]) -> Vec<String> {
    let mut places: Vec<(&Place, Vec<String>)> = Vec::new();
    for rule in rules.iter().filter(|rule| !is_app_or_site_rule(rule)) {
        let part = match rule.length.end() {
            Some(end) => format!("{}{}", rule.says, until(Some(end))),
            None => format!("{} until you turn it {}", rule.says, rule.says.other()),
        };
        match places.iter_mut().find(|(place, _)| **place == rule.place) {
            Some((_, parts)) => parts.push(part),
            None => places.push((&rule.place, vec![part])),
        }
    }
    places
        .into_iter()
        .map(|(place, parts)| {
            let place = match place {
                Place::Everywhere => "Everywhere".to_owned(),
                place => place.to_string(),
            };
            format!("{place}: {}", parts.join(", then "))
        })
        .collect()
}

/// " until 22:00" in local time, or nothing when no schedule transition is coming.
fn until(transition: Option<Timestamp>) -> String {
    transition.map_or_else(String::new, |transition| {
        format!(
            " until {}",
            transition.to_zoned(TimeZone::system()).strftime("%H:%M")
        )
    })
}

fn time_of_day(time: TimeOfDay) -> String {
    format!("{:02}:{:02}", time.hour, time.minute)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indefinite(place: &str, says: OnOff, set_by: SetBy) -> Rule {
        Rule {
            place: Place::from(place.parse::<RuleIdentity>().unwrap()),
            says,
            length: Length::Indefinite,
            set_by,
        }
    }

    #[test]
    fn decisions_read_after_night_shift() {
        let zone = TimeZone::get("Europe/Zurich").unwrap();
        let at = |instant: &str| instant.parse::<Timestamp>().unwrap();
        let figma_app = || Place::App("com.figma.Desktop".parse().unwrap());
        let figma = indefinite("app:com.figma.Desktop", OnOff::Off, SetBy::Config);
        let yours = |place, says, length| Rule {
            place,
            says,
            length,
            set_by: SetBy::You,
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
                    Reason::Rule(yours(
                        Place::Everywhere,
                        OnOff::Off,
                        Length::Until {
                            until: at("2026-10-16T22:00:00Z")
                        }
                    )),
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
                vec![Reason::Rule(yours(
                    Place::Everywhere,
                    OnOff::Off,
                    Length::Indefinite
                ))]
            ),
            "off until you turn it on"
        );
        assert_eq!(
            described(
                OnOff::On,
                figma_app(),
                vec![
                    Reason::Rule(yours(
                        figma_app(),
                        OnOff::On,
                        Length::UntilScheduleTurns {
                            until: Some(at("2026-10-17T05:00:00Z"))
                        }
                    )),
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
                vec![Reason::Rule(indefinite(
                    "domain:figma.com",
                    OnOff::On,
                    SetBy::You
                ))]
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
        let worded = |place: &str, set_by, says| rule(&indefinite(place, says, set_by));

        assert_eq!(
            worded("app:com.figma.Desktop", SetBy::Config, OnOff::Off),
            "app:com.figma.Desktop (config rule, off)"
        );
        assert_eq!(
            worded("domain:figma.com", SetBy::You, OnOff::On),
            "domain:figma.com (ad-hoc rule, on)"
        );
        assert_eq!(
            worded(
                "app:io.github.choco.nightcap.TourSample",
                SetBy::Tour,
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
        let there = |set_by, says| indefinite("app:com.figma.desktop", says, set_by);

        assert_eq!(
            rule_added(&figma, OnOff::Off, None),
            "Added app:com.figma.Desktop, which turns Night Shift off."
        );
        assert_eq!(
            rule_added(&figma, OnOff::Off, Some(&there(SetBy::Config, OnOff::Off))),
            "The config rule for app:com.figma.Desktop already turns Night Shift off."
        );
        assert_eq!(
            rule_added(&figma, OnOff::On, Some(&there(SetBy::You, OnOff::On))),
            "The ad-hoc rule for app:com.figma.Desktop already turns Night Shift on."
        );
        assert_eq!(
            rule_added(&figma, OnOff::On, Some(&there(SetBy::You, OnOff::Off))),
            "Changed the ad-hoc rule for app:com.figma.Desktop to turn Night Shift on."
        );
    }

    #[test]
    fn needs_attention_lists_each_problem_as_the_checks_carry_it() {
        let checks: Checks = serde_json::from_str(
            r#"{"checks":[{"kind":"accessibility","detail":"not_allowed","state":"drifted"},{"kind":"notifications","detail":"allowed","state":"ok"},{"kind":"config_file","detail":{"error":{"malformed":{"path":"/Users/me/.config/nightcap/config.toml","line":4,"message":"unknown field `domian`, expected `app` or `domain`"}}},"state":"broken"}],"permissions":{"accessibility":false,"notifications":"granted"}}"#,
        )
        .unwrap();

        assert_eq!(
            needs_attention(&checks),
            [
                "Accessibility isn’t allowed, so website checking can’t read the front tab.",
                "The config file is invalid: /Users/me/.config/nightcap/config.toml: line 4: unknown field `domian`, expected `app` or `domain`",
            ]
        );
    }
}
