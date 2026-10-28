//! The CLI's own words for what Nightcap reports, which sends facts alone: its decisions, its
//! rules, its checks, its refusals and the status its queries make up.

use jiff::tz::TimeZone;
use jiff::{Timestamp, civil};
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, CheckDetail, CheckKind, Checks, ConfigFileDetail,
    Decision, FileError, Hello, Install, Lasting, Length, ListedRule, NightShift, NightShiftDetail,
    NotSelfManaged, NotificationPermission, NotificationsDetail, OnOff, OpenAtLoginDetail,
    PROTOCOL_VERSION, Place, Problem, Reason, Refusal, Rule, RuleAdded, RuleIdentity, Schedule,
    SetBy, TimeOfDay,
};

use crate::client::Report;

/// What Nightcap decided and why, as it reads after "Night Shift", with times in `zone`.
pub(crate) fn decision(decision: &Decision, zone: &TimeZone) -> String {
    let until = |end: Option<Timestamp>, state: OnOff| match end {
        Some(end) => format!(" until {}", moment(end, zone)),
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
        // Only the app's tour begins a trial, which the CLI's user knows by the tour's name.
        Reason::StandIn => "on for the tour".to_owned(),
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

/// `app:com.figma.Desktop (config rule, off)`, and while it's snoozed
/// `app:com.figma.Desktop (config rule, off, snoozed until 07:00)`, with times in `zone`.
pub(crate) fn rule(listed: &ListedRule, zone: &TimeZone) -> String {
    let ListedRule { rule, snooze } = listed;
    let snoozed = match snooze.as_ref().map(|snooze| snooze.length.end()) {
        None => String::new(),
        Some(Some(end)) => format!(", snoozed{}", until(Some(end), zone)),
        Some(None) => format!(", snoozed until you turn it {}", rule.says),
    };
    format!(
        "{} ({} rule, {}{snoozed})",
        rule.place,
        source(rule.set_by),
        rule.says
    )
}

/// The apps' and sites' indefinite rules, the ones `nightcap rules` lists: the config's, the
/// user's and the tour's.
pub(crate) fn app_and_site_rules(rules: &[ListedRule]) -> Vec<ListedRule> {
    rules
        .iter()
        .filter(|listed| is_app_or_site_rule(&listed.rule))
        .cloned()
        .collect()
}

fn is_app_or_site_rule(rule: &Rule) -> bool {
    rule.place != Place::Everywhere && rule.length == Length::Indefinite
}

/// What adding a rule for `identity` that says `says` did, as Nightcap answers it: added one,
/// changed the user's own or the tour's, or left one that already says so. A config rule that
/// says otherwise is refused instead.
pub(crate) fn rule_added(identity: &RuleIdentity, says: OnOff, added: RuleAdded) -> String {
    match added {
        RuleAdded::New => format!("Added {identity}, which turns Night Shift {says}."),
        RuleAdded::AlreadySays { set_by } => format!(
            "The {} rule for {identity} already turns Night Shift {says}.",
            source(set_by)
        ),
        RuleAdded::Changed { set_by } => format!(
            "Changed the {} rule for {identity} to turn Night Shift {says}.",
            source(set_by)
        ),
    }
}

/// Who set a rule, as it reads before "rule". A trial's rule is the tour's, which lasts until the
/// tour in the app ends: the tour is the only trial.
fn source(set_by: SetBy) -> &'static str {
    match set_by {
        SetBy::Config => "config",
        SetBy::You => "ad-hoc",
        SetBy::Trial => "tour",
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
    match found {
        NightShiftDetail::Available { schedule } => match schedule {
            None => "Night Shift is available.".to_owned(),
            Some(Schedule::Off) => "Night Shift is available, with no schedule.".to_owned(),
            Some(Schedule::SunsetToSunrise) => {
                "Night Shift is available, following its schedule: sunset to sunrise.".to_owned()
            }
            Some(Schedule::Custom { from, to }) => format!(
                "Night Shift is available, following its schedule: {} to {}.",
                time_of_day(*from),
                time_of_day(*to)
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
        } => format!("{path}: line {line}: {message}"),
        FileError::Malformed {
            path,
            line: None,
            message,
        } => format!("{path}: {message}"),
        FileError::Invalid {
            path,
            line,
            problem,
        } => format!("{path}: line {line}: {}", self::problem(problem)),
        FileError::Newer { path, version } => {
            format!("{path} is version {version}, from a newer Nightcap than this one")
        }
    }
}

/// What makes an entry or a value in a file invalid, starting from the key or entry, as the file
/// writes it.
fn problem(problem: &Problem) -> String {
    match problem {
        Problem::UnsaidRules => {
            "rules are listed by what they say, under [rules.off] and [rules.on]".to_owned()
        }
        Problem::NotABundleId { entry } => format!(
            "{entry:?} isn't a bundle ID like com.figma.Desktop: only letters, digits, hyphens and periods"
        ),
        Problem::NotADomain { entry } => {
            format!("{entry:?} isn't a domain like figma.com: no scheme, path, port or wildcard")
        }
        Problem::BothWays { identity } => {
            format!("{identity} is in both rules.off and rules.on: an app or domain has one rule")
        }
        Problem::StrengthOutOfRange { strength } => {
            format!("night_shift.strength is 0 to 100, not {strength}")
        }
        Problem::NotASchedule { schedule } => format!(
            "night_shift.schedule is \"off\", \"sunset-to-sunrise\" or custom hours such as \"22:00-07:00\", not {schedule:?}"
        ),
        Problem::NotLengths { lengths } => format!(
            "lengths are like \"30m\", \"1h\" or \"1h30m\", from a minute to a day, not {lengths:?}"
        ),
        Problem::LengthsUnderMenu => {
            "lengths is a key of its own, before any table, not one under [menu]".to_owned()
        }
    }
}

/// Why Nightcap refused what the CLI asked, as it reads after "error: ".
pub(crate) fn refusal(refusal: &Refusal) -> String {
    match refusal {
        Refusal::NightShiftUnavailable => "Night Shift isn't available on this Mac".to_owned(),
        Refusal::StrengthUnavailable => {
            "Night Shift's strength can't be read on this Mac".to_owned()
        }
        Refusal::StrengthOutOfRange { strength } => {
            format!("a strength is 0 to 100, not {strength}")
        }
        Refusal::NotATimeOfDay(time) => format!(
            "custom hours are times of day, from 00:00 to 23:59, not {}",
            time_of_day(*time)
        ),
        Refusal::TooShort => "a length is longer than zero".to_owned(),
        Refusal::TooLong { length } => format!("{length:#} is too long"),
        Refusal::Passed => "that time has passed".to_owned(),
        Refusal::NoSchedule => "there's no schedule to wait for".to_owned(),
        Refusal::IndefiniteIsAdded => {
            "only everywhere's indefinite rule is set: an app's or a site's is added as a rule"
                .to_owned()
        }
        Refusal::ConfigSays { identity, says } => {
            format!("{identity} comes from the config, which says {says}; change it there")
        }
        Refusal::ConfigRule { identity } => {
            format!("{identity} comes from the config; change it there")
        }
        Refusal::NoSuchRule { identity } => format!("{identity} isn't a rule"),
        Refusal::AdHocRulesOff(error) => format!("ad-hoc rules are off: {}", file_error(error)),
        Refusal::AdHocRulesNotSaved { path, message } => {
            format!("couldn't save {path}: {message}")
        }
        Refusal::InvalidConfig(error) => file_error(error),
        Refusal::WebsiteCheckingInConfig => {
            "website checking is set by `websites` in the config".to_owned()
        }
        Refusal::NoUpdater(NotSelfManaged::DevBuild) => {
            "dev builds don't update themselves; rebuild instead".to_owned()
        }
        Refusal::NoUpdater(NotSelfManaged::ManagedInstall) => {
            "this install is managed by its installer, which keeps it up to date".to_owned()
        }
        Refusal::NoLoginItem(NotSelfManaged::DevBuild) => {
            "dev builds don't open themselves at login".to_owned()
        }
        Refusal::NoLoginItem(NotSelfManaged::ManagedInstall) => {
            "this install doesn't open itself at login: its installer starts it".to_owned()
        }
        Refusal::NothingToAccept { check } => match check {
            CheckKind::NightShift => {
                "Night Shift's schedule is as you set it up, so there's nothing to accept"
            }
            CheckKind::Accessibility => {
                "Accessibility is as you set it up, so there's nothing to accept"
            }
            CheckKind::Notifications => {
                "notifications are as you set them up, so there's nothing to accept"
            }
            CheckKind::OpenAtLogin => {
                "opening at login is as you set it up, so there's nothing to accept"
            }
            CheckKind::ConfigFile => "problems with the config can only be fixed, not accepted",
            CheckKind::AdHocRules => "problems with rules.toml can only be fixed, not accepted",
        }
        .to_owned(),
    }
}

/// Nightcap speaks another protocol than this CLI, as told by its hello where the CLI could
/// read it.
pub(crate) fn mismatch(server: Option<&Hello>) -> String {
    match server {
        Some(server) => format!(
            "the running Nightcap is {} (protocol {}) and this CLI speaks protocol {PROTOCOL_VERSION}; restart Nightcap",
            server.version, server.protocol
        ),
        None => format!(
            "the running Nightcap speaks another protocol than this CLI's {PROTOCOL_VERSION}; restart Nightcap"
        ),
    }
}

/// Nightcap couldn't read what this CLI sent, for serde's reason.
pub(crate) fn malformed(message: &str) -> String {
    format!("Nightcap couldn't read the request: {message}")
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
        Lasting::Until(end) => format!("until {}", moment(*end, zone)),
        Lasting::UntilScheduleTurns => "until the schedule turns it back".to_owned(),
        Lasting::Indefinite => "until you turn it back".to_owned(),
    }
}

/// What `nightcap status` shows, a line at a time, with times in `zone`.
pub(crate) fn status(report: &Report, zone: &TimeZone) -> Vec<String> {
    let Report {
        about,
        night_shift,
        front,
        rules: every_rule,
        config,
        preferences,
        checks,
    } = report;
    // A self-managed install that doesn't manage itself is a dev build (ADR-0011).
    let install = match (about.manages_itself, about.install) {
        (true, _) => "self-managed",
        (false, Install::Managed) => "managed by its installer",
        (false, Install::SelfManaged) => "a dev build",
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
                until(night_shift.next_transition(), zone)
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
    let mut lines = vec![format!("Nightcap {}, {install}", about.version)];
    let decided = decision(&front.decision, zone);
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
        lines.push(format!("Night Shift: {decided}"));
    } else {
        lines.push(format!("Night Shift: {state}, {decided}"));
    }
    lines.push(format!(
        "Schedule: {schedule}{scheduled}{}",
        from_config(config.schedule, night_shift.schedule, schedule_text)
    ));
    if let Some(strength) = night_shift.strength {
        lines.push(format!(
            "Strength: {strength}{}",
            from_config(config.strength, night_shift.strength, |value| value
                .to_string())
        ));
    }
    let host = match (&front.host, front.watching) {
        (Some(host), _) => format!(", on {host}"),
        (None, true) => ", host unknown".to_owned(),
        (None, false) => String::new(),
    };
    lines.push(format!(
        "Frontmost app: {}{host}",
        front.app.as_ref().map_or("unknown", |app| app.as_str())
    ));
    lines.push(format!(
        "Rules: {}",
        rules(&app_and_site_rules(every_rule), zone)
    ));
    let website_checking = preferences.website_checking;
    lines.push(format!(
        "Website checking: {}{}",
        website_checking.state,
        if website_checking.from_config {
            " (set in the config)"
        } else {
            ""
        }
    ));
    lines.extend(places_rules(every_rule, zone));
    lines.push(format!("Accessibility: {accessibility}"));
    lines.push(format!("Notifications: {notifications}"));
    let problems = needs_attention(checks);
    if !problems.is_empty() {
        lines.push("Needs attention:".to_owned());
        lines.extend(problems.into_iter().map(|problem| format!("  {problem}")));
    }
    lines
}

/// Each problem's line.
fn needs_attention(checks: &Checks) -> Vec<String> {
    checks
        .checks
        .iter()
        .filter(|check| check.state.is_problem())
        .map(|check| self::check(&check.detail))
        .collect()
}

/// Rules as `rule` words them, with times in `zone`, or "none".
pub(crate) fn rules(rules: &[ListedRule], zone: &TimeZone) -> String {
    if rules.is_empty() {
        return "none".to_owned();
    }
    rules
        .iter()
        .map(|listed| rule(listed, zone))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Each place's rules but an app's or site's indefinite one, shortest first, a line per place,
/// with times in `zone`: "app:com.figma.Desktop: on until 07:00".
fn places_rules(rules: &[ListedRule], zone: &TimeZone) -> Vec<String> {
    let mut places: Vec<(&Place, Vec<String>)> = Vec::new();
    for rule in rules
        .iter()
        .map(|listed| &listed.rule)
        .filter(|rule| !is_app_or_site_rule(rule))
    {
        let part = match rule.length.end() {
            Some(end) => format!("{}{}", rule.says, until(Some(end), zone)),
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

/// " until 22:00" in `zone`, or nothing when no schedule transition is coming.
fn until(transition: Option<Timestamp>, zone: &TimeZone) -> String {
    transition.map_or_else(String::new, |transition| {
        format!(" until {}", moment(transition, zone))
    })
}

/// A time on the clock, as the CLI writes every time: two-digit hours on a 24-hour clock, then
/// the minutes, "07:00".
fn clock(time: civil::Time) -> String {
    time.strftime("%H:%M").to_string()
}

/// A moment as the clock in `zone` reads it.
fn moment(moment: Timestamp, zone: &TimeZone) -> String {
    clock(moment.to_zoned(zone.clone()).time())
}

/// One end of custom hours, which the core answers on the clock; a refused one can be past it,
/// and then its numbers are written as they are.
fn time_of_day(time: TimeOfDay) -> String {
    let on_the_clock = i8::try_from(time.hour)
        .ok()
        .zip(i8::try_from(time.minute).ok())
        .and_then(|(hour, minute)| civil::Time::new(hour, minute, 0, 0).ok());
    match on_the_clock {
        Some(time) => clock(time),
        None => format!("{:02}:{:02}", time.hour, time.minute),
    }
}

#[cfg(test)]
mod tests {
    use nightcap_protocol::{
        About, Check, CheckState, ConfigState, Front, NightShiftState, Permissions, PlaceInFront,
        Preferences, WebsiteChecking,
    };

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
            described(OnOff::On, Place::Everywhere, vec![Reason::StandIn]),
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
        let zone = TimeZone::get("Europe/Zurich").unwrap();
        let worded = |place: &str, set_by, says| {
            rule(
                &ListedRule {
                    rule: indefinite(place, says, set_by),
                    snooze: None,
                },
                &zone,
            )
        };

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
                "app:io.github.choco.nightcap.tour-sample",
                SetBy::Trial,
                OnOff::Off
            ),
            "app:io.github.choco.nightcap.tour-sample (tour rule, off)"
        );
    }

    #[test]
    fn a_snoozed_rule_reads_until_its_snooze_ends() {
        let zone = TimeZone::get("Europe/Zurich").unwrap();
        let snoozed = |until| {
            rule(
                &ListedRule {
                    rule: indefinite("app:com.figma.Desktop", OnOff::Off, SetBy::Config),
                    snooze: Some(Rule {
                        place: Place::App("com.figma.Desktop".parse().unwrap()),
                        says: OnOff::On,
                        length: Length::UntilScheduleTurns { until },
                        set_by: SetBy::OutsideChange,
                    }),
                },
                &zone,
            )
        };

        assert_eq!(
            snoozed(Some("2026-10-17T05:00:00Z".parse().unwrap())),
            "app:com.figma.Desktop (config rule, off, snoozed until 07:00)"
        );
        assert_eq!(
            snoozed(None),
            "app:com.figma.Desktop (config rule, off, snoozed until you turn it off)"
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
                line: 2,
                problem: Problem::StrengthOutOfRange { strength: 101 },
            }),
            "The config file is invalid: /Users/me/.config/nightcap/config.toml: line 2: night_shift.strength is 0 to 100, not 101"
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
        assert_eq!(
            ad_hoc(AdHocRulesDetail::Off(FileError::Invalid {
                path: "/Users/me/.local/share/nightcap/rules.toml".to_owned(),
                line: 3,
                problem: Problem::NotABundleId {
                    entry: "com.figma.Desktop ".to_owned(),
                },
            })),
            "Ad-hoc rules are off: /Users/me/.local/share/nightcap/rules.toml: line 3: \"com.figma.Desktop \" isn't a bundle ID like com.figma.Desktop: only letters, digits, hyphens and periods."
        );
    }

    #[test]
    fn a_files_problem_names_the_entry_or_value_as_written() {
        for (found, said) in [
            (
                Problem::UnsaidRules,
                "rules are listed by what they say, under [rules.off] and [rules.on]",
            ),
            (
                Problem::NotABundleId {
                    entry: String::new(),
                },
                "\"\" isn't a bundle ID like com.figma.Desktop: only letters, digits, hyphens and periods",
            ),
            (
                Problem::NotADomain {
                    entry: "https://figma.com".to_owned(),
                },
                "\"https://figma.com\" isn't a domain like figma.com: no scheme, path, port or wildcard",
            ),
            (
                Problem::BothWays {
                    identity: "app:com.figma.Desktop".parse().unwrap(),
                },
                "app:com.figma.Desktop is in both rules.off and rules.on: an app or domain has one rule",
            ),
            (
                Problem::StrengthOutOfRange { strength: 101 },
                "night_shift.strength is 0 to 100, not 101",
            ),
            (
                Problem::NotASchedule {
                    schedule: "dusk".to_owned(),
                },
                "night_shift.schedule is \"off\", \"sunset-to-sunrise\" or custom hours such as \"22:00-07:00\", not \"dusk\"",
            ),
            (
                Problem::NotLengths {
                    lengths: vec!["45m".to_owned(), "forever".to_owned()],
                },
                "lengths are like \"30m\", \"1h\" or \"1h30m\", from a minute to a day, not [\"45m\", \"forever\"]",
            ),
            (
                Problem::LengthsUnderMenu,
                "lengths is a key of its own, before any table, not one under [menu]",
            ),
        ] {
            assert_eq!(problem(&found), said, "{found:?}");
        }
    }

    #[test]
    fn adding_a_rule_says_whether_it_was_added_changed_or_already_there() {
        let figma: RuleIdentity = "app:com.figma.Desktop".parse().unwrap();

        assert_eq!(
            rule_added(&figma, OnOff::Off, RuleAdded::New),
            "Added app:com.figma.Desktop, which turns Night Shift off."
        );
        assert_eq!(
            rule_added(
                &figma,
                OnOff::Off,
                RuleAdded::AlreadySays {
                    set_by: SetBy::Config
                }
            ),
            "The config rule for app:com.figma.Desktop already turns Night Shift off."
        );
        assert_eq!(
            rule_added(
                &figma,
                OnOff::On,
                RuleAdded::AlreadySays { set_by: SetBy::You }
            ),
            "The ad-hoc rule for app:com.figma.Desktop already turns Night Shift on."
        );
        assert_eq!(
            rule_added(&figma, OnOff::On, RuleAdded::Changed { set_by: SetBy::You }),
            "Changed the ad-hoc rule for app:com.figma.Desktop to turn Night Shift on."
        );
        assert_eq!(
            rule_added(
                &figma,
                OnOff::Off,
                RuleAdded::Changed {
                    set_by: SetBy::Trial
                }
            ),
            "Changed the tour rule for app:com.figma.Desktop to turn Night Shift off."
        );
    }

    #[test]
    fn a_refusal_says_why_with_what_it_names() {
        let figma: RuleIdentity = "app:com.figma.Desktop".parse().unwrap();
        let config = || "/Users/me/.config/nightcap/config.toml".to_owned();
        let rules = || "/Users/me/.local/share/nightcap/rules.toml".to_owned();

        for (why, said) in [
            (
                Refusal::NightShiftUnavailable,
                "Night Shift isn't available on this Mac",
            ),
            (
                Refusal::StrengthUnavailable,
                "Night Shift's strength can't be read on this Mac",
            ),
            (
                Refusal::StrengthOutOfRange { strength: 101 },
                "a strength is 0 to 100, not 101",
            ),
            (
                Refusal::NotATimeOfDay(TimeOfDay {
                    hour: 24,
                    minute: 5,
                }),
                "custom hours are times of day, from 00:00 to 23:59, not 24:05",
            ),
            (Refusal::TooShort, "a length is longer than zero"),
            (
                Refusal::TooLong {
                    length: jiff::SignedDuration::from_mins(90),
                },
                "1h 30m is too long",
            ),
            (Refusal::Passed, "that time has passed"),
            (Refusal::NoSchedule, "there's no schedule to wait for"),
            (
                Refusal::IndefiniteIsAdded,
                "only everywhere's indefinite rule is set: an app's or a site's is added as a rule",
            ),
            (
                Refusal::ConfigSays {
                    identity: figma.clone(),
                    says: OnOff::Off,
                },
                "app:com.figma.Desktop comes from the config, which says off; change it there",
            ),
            (
                Refusal::ConfigRule {
                    identity: figma.clone(),
                },
                "app:com.figma.Desktop comes from the config; change it there",
            ),
            (
                Refusal::NoSuchRule {
                    identity: "domain:figma.com".parse().unwrap(),
                },
                "domain:figma.com isn't a rule",
            ),
            (
                Refusal::AdHocRulesOff(FileError::Newer {
                    path: rules(),
                    version: 3,
                }),
                "ad-hoc rules are off: /Users/me/.local/share/nightcap/rules.toml is version 3, from a newer Nightcap than this one",
            ),
            (
                Refusal::AdHocRulesNotSaved {
                    path: rules(),
                    message: "Is a directory (os error 21)".to_owned(),
                },
                "couldn't save /Users/me/.local/share/nightcap/rules.toml: Is a directory (os error 21)",
            ),
            (
                Refusal::InvalidConfig(FileError::Malformed {
                    path: config(),
                    line: Some(2),
                    message: "invalid type: string \"com.figma.Desktop\", expected a sequence"
                        .to_owned(),
                }),
                "/Users/me/.config/nightcap/config.toml: line 2: invalid type: string \"com.figma.Desktop\", expected a sequence",
            ),
            (
                Refusal::WebsiteCheckingInConfig,
                "website checking is set by `websites` in the config",
            ),
            (
                Refusal::NoUpdater(NotSelfManaged::DevBuild),
                "dev builds don't update themselves; rebuild instead",
            ),
            (
                Refusal::NoUpdater(NotSelfManaged::ManagedInstall),
                "this install is managed by its installer, which keeps it up to date",
            ),
            (
                Refusal::NoLoginItem(NotSelfManaged::DevBuild),
                "dev builds don't open themselves at login",
            ),
            (
                Refusal::NoLoginItem(NotSelfManaged::ManagedInstall),
                "this install doesn't open itself at login: its installer starts it",
            ),
            (
                Refusal::NothingToAccept {
                    check: CheckKind::Notifications,
                },
                "notifications are as you set them up, so there's nothing to accept",
            ),
            (
                Refusal::NothingToAccept {
                    check: CheckKind::NightShift,
                },
                "Night Shift's schedule is as you set it up, so there's nothing to accept",
            ),
        ] {
            assert_eq!(refusal(&why), said, "{why:?}");
        }
    }

    #[test]
    fn a_protocol_mismatch_names_the_running_nightcap_where_it_can() {
        let server = Hello {
            protocol: 6,
            version: "1.2.0".to_owned(),
        };

        assert_eq!(
            mismatch(Some(&server)),
            format!(
                "the running Nightcap is 1.2.0 (protocol 6) and this CLI speaks protocol {PROTOCOL_VERSION}; restart Nightcap"
            )
        );
        assert_eq!(
            mismatch(None),
            format!(
                "the running Nightcap speaks another protocol than this CLI's {PROTOCOL_VERSION}; restart Nightcap"
            )
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

    /// Safari in front on figma.com, which a config rule keeps off while Night Shift is on by the
    /// custom hours, with everywhere on until 23:30 and notifications switched off.
    fn on_figma() -> Report {
        let at = |instant: &str| instant.parse::<Timestamp>().unwrap();
        let figma = indefinite("domain:figma.com", OnOff::Off, SetBy::Config);
        let late = Rule {
            place: Place::Everywhere,
            says: OnOff::On,
            length: Length::Until {
                until: at("2026-10-16T21:30:00Z"),
            },
            set_by: SetBy::You,
        };
        let in_front = |place: Place| PlaceInFront {
            place,
            gives: OnOff::On,
            rules: Vec::new(),
            base: None,
            inherits: None,
        };
        Report {
            about: About {
                version: "1.2.0".to_owned(),
                install: Install::SelfManaged,
                manages_itself: true,
                legacy_onboarding: 1,
            },
            night_shift: NightShiftState {
                state: NightShift::On,
                strength: Some(60),
                schedule: Some(Schedule::Custom {
                    from: TimeOfDay {
                        hour: 22,
                        minute: 0,
                    },
                    to: TimeOfDay { hour: 7, minute: 0 },
                }),
                set_aside: false,
                scheduled: Some(OnOff::On),
                next_on: None,
                next_off: Some(at("2026-10-17T05:00:00Z")),
            },
            front: Front {
                app: Some("com.apple.Safari".parse().unwrap()),
                watching: true,
                host: Some("www.figma.com".to_owned()),
                site: Some("figma.com".parse().unwrap()),
                decision: Decision {
                    state: Some(OnOff::Off),
                    place: figma.place.clone(),
                    reasons: vec![Reason::Rule(figma.clone())],
                },
                places: Vec::new(),
                everywhere: in_front(Place::Everywhere),
            },
            rules: [
                figma,
                indefinite("app:com.apple.mail", OnOff::On, SetBy::You),
                late,
            ]
            .map(|rule| ListedRule { rule, snooze: None })
            .to_vec(),
            config: ConfigState {
                path: "/Users/me/.config/nightcap/config.toml".to_owned(),
                found: true,
                error: None,
                strength: Some(40),
                schedule: None,
                websites: Some(OnOff::On),
                lengths: Vec::new(),
                ad_hoc_error: None,
            },
            preferences: Preferences {
                website_checking: WebsiteChecking {
                    state: OnOff::On,
                    from_config: true,
                },
                notifications_wanted: true,
                open_at_login: false,
            },
            checks: Checks {
                checks: vec![Check {
                    detail: CheckDetail::Notifications(NotificationsDetail::SwitchedOff),
                    state: CheckState::Drifted,
                }],
                permissions: Permissions {
                    accessibility: true,
                    notifications: NotificationPermission::Denied,
                    login_item: None,
                },
            },
        }
    }

    #[test]
    fn status_reads_a_line_at_a_time_with_times_in_the_zone() {
        let zone = TimeZone::get("Europe/Zurich").unwrap();

        assert_eq!(
            status(&on_figma(), &zone),
            [
                "Nightcap 1.2.0, self-managed",
                "Night Shift: on, off by the config rule for domain:figma.com",
                "Schedule: 22:00 to 07:00, on until 07:00",
                "Strength: 60 (the config says 40)",
                "Frontmost app: com.apple.Safari, on www.figma.com",
                "Rules: domain:figma.com (config rule, off), app:com.apple.mail (ad-hoc rule, on)",
                "Website checking: on (set in the config)",
                "Everywhere: on until 23:30",
                "Accessibility: granted",
                "Notifications: denied",
                "Needs attention:",
                "  Notifications were allowed, but they’re switched off in System Settings.",
            ]
        );
    }

    #[test]
    fn status_says_whether_the_install_manages_itself() {
        let zone = TimeZone::get("Europe/Zurich").unwrap();
        let first_line = |install, manages_itself| {
            let mut report = on_figma();
            report.about.install = install;
            report.about.manages_itself = manages_itself;
            status(&report, &zone).swap_remove(0)
        };

        assert_eq!(
            first_line(Install::SelfManaged, true),
            "Nightcap 1.2.0, self-managed"
        );
        assert_eq!(
            first_line(Install::Managed, false),
            "Nightcap 1.2.0, managed by its installer"
        );
        assert_eq!(
            first_line(Install::SelfManaged, false),
            "Nightcap 1.2.0, a dev build"
        );
    }
}
