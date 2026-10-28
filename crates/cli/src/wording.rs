//! The CLI's own words for what Nightcap reports, which sends facts alone: its decisions, its
//! rules, its checks, its refusals and the status its queries make up.

use jiff::tz::TimeZone;
use jiff::{Timestamp, civil};
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, CheckDetail, Checks, ConfigFileDetail, Decision,
    FileError, Hello, IdentityError, Install, IoFailure, Lasting, Length, ListedRule, NightShift,
    NightShiftDetail, NotSelfManaged, NotificationPermission, NotificationsDetail, OnOff,
    OpenAtLoginDetail, PROTOCOL_VERSION, Place, Problem, Reason, Refusal, Rule, RuleAdded,
    RuleIdentity, Schedule, SetBy, TimeOfDay, TrialState, ValueType,
};

use crate::client::Report;

/// What Nightcap decided and why, as it reads after "Night Shift", with times in `zone`.
fn decision(decision: &Decision, zone: &TimeZone) -> String {
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
    match top {
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
    }
}

/// ", its config rule snoozed" while the rule at the app or site that decides is snoozed, as
/// the rules answer serves it, else nothing.
fn snoozed(place: &Place, rules: &[ListedRule]) -> String {
    rules
        .iter()
        .find(|listed| {
            listed.snooze.is_some()
                && listed.rule.place == *place
                && is_app_or_site_rule(&listed.rule)
        })
        .map_or_else(String::new, |listed| {
            format!(", its {} rule snoozed", source(listed.rule.set_by))
        })
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

pub(crate) fn is_app_or_site_rule(rule: &Rule) -> bool {
    rule.place != Place::Everywhere && rule.length == Length::Indefinite
}

/// What adding a rule for `identity` that says `says` did, as Nightcap answers it: added one,
/// the tour's lasting until it ends, changed the user's own or the tour's, or left one that
/// already says so. A config rule that says otherwise is refused instead.
pub(crate) fn rule_added(identity: &RuleIdentity, says: OnOff, added: RuleAdded) -> String {
    match added {
        RuleAdded::New {
            set_by: SetBy::Trial,
        } => format!("Added {identity}, which turns Night Shift {says} until the tour ends."),
        RuleAdded::New { .. } => format!("Added {identity}, which turns Night Shift {says}."),
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

/// What a check that needs attention found, in a sentence.
pub(crate) fn check(detail: &CheckDetail) -> String {
    match detail {
        CheckDetail::NightShift(NightShiftDetail::Unavailable) => {
            "Night Shift isn’t available on this Mac.".to_owned()
        }
        CheckDetail::NightShift(NightShiftDetail::SetAside) => {
            "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on."
                .to_owned()
        }
        CheckDetail::NightShift(NightShiftDetail::NoSunTimes) => {
            "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. Custom hours work without them."
                .to_owned()
        }
        CheckDetail::Accessibility(AccessibilityDetail::NotAllowed) => {
            "Accessibility isn’t allowed, so website checking can’t read the front tab.".to_owned()
        }
        CheckDetail::Notifications(NotificationsDetail::SwitchedOff) => {
            "Notifications were allowed, but they’re switched off in System Settings.".to_owned()
        }
        CheckDetail::OpenAtLogin(OpenAtLoginDetail::AwaitingApproval) => {
            "macOS is waiting for your approval in Login Items.".to_owned()
        }
        CheckDetail::OpenAtLogin(OpenAtLoginDetail::Removed) => {
            "Nightcap no longer opens at login: it was removed in System Settings.".to_owned()
        }
        CheckDetail::ConfigFile(ConfigFileDetail::Error(error)) => {
            format!("The config file is invalid: {}", file_error(error))
        }
        CheckDetail::AdHocRules(AdHocRulesDetail::Off(error)) => {
            format!("Ad-hoc rules are off: {}.", file_error(error))
        }
        // The core finds these with their check ok or off, which needs no attention.
        CheckDetail::NightShift(NightShiftDetail::Available { .. })
        | CheckDetail::Accessibility(AccessibilityDetail::Allowed | AccessibilityDetail::NotNeeded)
        | CheckDetail::Notifications(NotificationsDetail::Allowed | NotificationsDetail::Off)
        | CheckDetail::OpenAtLogin(OpenAtLoginDetail::Opens | OpenAtLoginDetail::Off)
        | CheckDetail::ConfigFile(ConfigFileDetail::Valid { .. } | ConfigFileDetail::Missing)
        | CheckDetail::AdHocRules(AdHocRulesDetail::On { .. }) => unexpected_reply(),
    }
}

/// What's wrong with a file, starting from its path: "/…/config.toml: line 4, column 12 isn't
/// valid TOML".
fn file_error(error: &FileError) -> String {
    match error {
        FileError::Unreadable { path, failure } => match io_failure(*failure) {
            Some(why) => format!("couldn't read {path}: {why}"),
            None => format!("couldn't read {path}"),
        },
        FileError::Malformed { path, at: Some(at) } => format!(
            "{path}: line {}, column {} isn't valid TOML",
            at.line, at.column
        ),
        FileError::Malformed { path, at: None } => format!("{path} isn't valid TOML"),
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

/// Why macOS couldn't read or write a file, as it reads after the file's path and a colon, or
/// None for a reason only the log gives.
fn io_failure(failure: IoFailure) -> Option<&'static str> {
    match failure {
        IoFailure::NotFound => Some("it, or a folder on its path, isn't there"),
        IoFailure::PermissionDenied => Some("permission denied"),
        IoFailure::IsAFolder => Some("it's a folder"),
        IoFailure::NotAFolder => Some("one of the folders on its path is a file"),
        IoFailure::NotUtf8 => Some("it isn't UTF-8 text"),
        IoFailure::Other => None,
    }
}

/// What makes an entry or a value in a file invalid, starting from the key or entry, as the file
/// writes it.
fn problem(problem: &Problem) -> String {
    match problem {
        Problem::UnknownKey {
            table,
            key,
            expected,
        } => match expected.as_slice() {
            [] => format!("`{}` isn't a key here", dotted(table, key)),
            _ => format!(
                "`{}` isn't a key here; use {}",
                dotted(table, key),
                either(expected)
            ),
        },
        Problem::WrongType {
            table,
            key,
            found,
            expected,
        } => format!(
            "`{}` is {}, not {}",
            dotted(table, key),
            value_type(*expected),
            value_type(*found)
        ),
        Problem::MissingKey { table, key } => format!("`{}` is missing", dotted(table, key)),
        Problem::NotABundleId { entry } => not_a_bundle_id(entry),
        Problem::NotADomain { entry } => not_a_domain(entry),
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
    }
}

/// A key with the keys that lead to its table: "rules.off.app".
fn dotted(table: &[String], key: &str) -> String {
    table
        .iter()
        .map(String::as_str)
        .chain([key])
        .collect::<Vec<_>>()
        .join(".")
}

/// Keys to pick one of: "`app` or `domain`", or "`a`, `b` or `c`".
fn either(keys: &[String]) -> String {
    let quoted: Vec<_> = keys.iter().map(|key| format!("`{key}`")).collect();
    match quoted.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, others)) => format!("{} or {last}", others.join(", ")),
        None => String::new(),
    }
}

/// What a value is, as a problem names it: "a list of strings".
fn value_type(value: ValueType) -> &'static str {
    match value {
        ValueType::String => "a string",
        ValueType::Strings => "a list of strings",
        ValueType::List => "a list that isn't all strings",
        ValueType::Integer => "an integer",
        ValueType::Float => "a float",
        ValueType::Boolean => "true or false",
        ValueType::Datetime => "a date or a time",
        ValueType::Table => "a table",
    }
}

/// Why Nightcap refused what the CLI asked, as it reads after "error: ".
pub(crate) fn refusal(refusal: &Refusal) -> String {
    match refusal {
        Refusal::NightShiftUnavailable => "Night Shift isn't available on this Mac".to_owned(),
        Refusal::StrengthUnavailable => {
            "Night Shift's strength can't be read on this Mac".to_owned()
        }
        Refusal::TooLong { length } => format!("{length:#} is too long"),
        Refusal::Passed => "that time has passed".to_owned(),
        Refusal::NoSchedule => "there's no schedule to wait for".to_owned(),
        Refusal::ConfigSays { identity, says } => {
            format!("{identity} comes from the config, which says {says}; change it there")
        }
        Refusal::ConfigRule { identity } => {
            format!("{identity} comes from the config; change it there")
        }
        Refusal::NoSuchRule { identity } => format!("{identity} isn't a rule"),
        Refusal::AdHocRulesOff(error) => format!("ad-hoc rules are off: {}", file_error(error)),
        Refusal::AdHocRulesNotSaved { path, failure } => match io_failure(*failure) {
            Some(why) => format!("couldn't save {path}: {why}"),
            None => format!("couldn't save {path}"),
        },
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
        // The CLI never sends what these refuse: clap keeps its strengths 0 to 100 and its
        // lengths longer than zero, it sets no indefinite rule, schedule or login item, and it
        // accepts no drift.
        Refusal::StrengthOutOfRange { .. }
        | Refusal::NotATimeOfDay(_)
        | Refusal::TooShort
        | Refusal::IndefiniteIsAdded
        | Refusal::NoLoginItem(_)
        | Refusal::NothingToAccept { .. } => unexpected_reply(),
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

pub(crate) fn unexpected_reply() -> String {
    "Nightcap sent a reply this CLI doesn't expect".to_owned()
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
fn length_text(length: &Lasting, zone: &TimeZone) -> String {
    match length {
        Lasting::For(duration) => format!("for {duration:#}"),
        Lasting::Until(end) => format!("until {}", moment(*end, zone)),
        Lasting::UntilScheduleTurns => "until the schedule turns it back".to_owned(),
        Lasting::Indefinite => "until you turn it back".to_owned(),
    }
}

/// Why text the CLI was given names no app, site or rule, as it reads after clap's "invalid
/// value".
pub(crate) fn identity_error(error: &IdentityError) -> String {
    match error {
        IdentityError::BundleId(text) => not_a_bundle_id(text),
        IdentityError::Domain(text) => not_a_domain(text),
        IdentityError::RuleIdentity(text) => format!(
            "{text:?} names no app or site: write it like app:com.figma.Desktop or domain:figma.com"
        ),
    }
}

fn not_a_bundle_id(text: &str) -> String {
    format!(
        "{text:?} isn't a bundle ID like com.figma.Desktop: only letters, digits, hyphens and periods"
    )
}

fn not_a_domain(text: &str) -> String {
    format!("{text:?} isn't a domain like figma.com: no scheme, path, port or wildcard")
}

/// Why what `--for` was given isn't a length.
pub(crate) fn not_a_length(text: &str) -> String {
    format!("{text:?} isn't a length like 30m, 1h or 1h30m")
}

pub(crate) fn not_longer_than_zero() -> String {
    "a length is longer than zero".to_owned()
}

/// Why what `--until` was given isn't a time of day.
pub(crate) fn not_a_time_of_day(text: &str) -> String {
    format!("{text:?} isn't a time of day like 23:30")
}

/// Why what `--wait` was given isn't a wait.
pub(crate) fn not_a_wait(text: &str) -> String {
    format!("{text:?} isn't a wait like 5s or 1m")
}

/// Why an `on` or `off` at an app or a site needs a length: only a rule is there for good.
pub(crate) fn needs_a_length(place: &Place) -> String {
    format!(
        "an app or a site needs a length: --for, --until or --until-schedule; for good, add a rule with `nightcap rules add {place}`"
    )
}

/// What a change to a place that doesn't decide set: "Set app:com.figma.Desktop on for 1h.",
/// with times in `zone`.
pub(crate) fn set(place: &Place, state: OnOff, length: &Lasting, zone: &TimeZone) -> String {
    format!("Set {place} {state} {}.", length_text(length, zone))
}

/// What Night Shift does after a command, as the core decided it, with the deciding rule's snooze
/// as `rules` serve it and times in `zone`.
pub(crate) fn decided(decision: &Decision, rules: &[ListedRule], zone: &TimeZone) -> String {
    format!(
        "Night Shift {}{}",
        self::decision(decision, zone),
        snoozed(&decision.place, rules)
    )
}

/// What a change to the rules did, as `done` says it, then what Night Shift does after it, with
/// the deciding rule's snooze as `rules` serve it and times in `zone`.
pub(crate) fn rules_changed(
    done: &str,
    decision: &Decision,
    rules: &[ListedRule],
    zone: &TimeZone,
) -> String {
    format!("{done} {}.", decided(decision, rules, zone))
}

pub(crate) fn rule_removed(identity: &RuleIdentity) -> String {
    format!("Removed {identity}.")
}

/// `nightcap rules list` with no rule to list.
pub(crate) fn no_rules() -> String {
    "No rules.".to_owned()
}

/// What a reload did, with the rules it leads to and times in `zone`.
pub(crate) fn reloaded(rules: &[ListedRule], zone: &TimeZone) -> String {
    format!(
        "Reloaded the config. Rules: {}",
        self::rules(&app_and_site_rules(rules), zone)
    )
}

pub(crate) fn strength(strength: u8) -> String {
    format!("Strength: {strength}")
}

pub(crate) fn strength_set(strength: u8) -> String {
    format!("Strength set to {strength}.")
}

pub(crate) fn checking_for_updates() -> String {
    "Checking for updates; Nightcap shows what it finds.".to_owned()
}

pub(crate) fn website_checking_set(state: OnOff) -> String {
    format!("Website checking is {state}.")
}

/// Why the config can't be used, starting from its path, while its check says it can't.
pub(crate) fn config_error(checks: &Checks) -> Option<String> {
    checks.checks.iter().find_map(|check| match &check.detail {
        CheckDetail::ConfigFile(ConfigFileDetail::Error(error)) => Some(file_error(error)),
        _ => None,
    })
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
        trial,
    } = report;
    let install = match about.install {
        Install::SelfManaged => "self-managed",
        Install::NotSelfManaged(NotSelfManaged::ManagedInstall) => "managed by its installer",
        Install::NotSelfManaged(NotSelfManaged::DevBuild) => "a dev build",
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
    lines.extend(trial.map(tour));
    let decided = format!(
        "{}{}",
        decision(&front.decision, zone),
        snoozed(&front.decision.place, every_rule)
    );
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

/// The line for a trial that runs, which only the app's tour begins (ADR-0016): what changes
/// during it lasts until it ends.
fn tour(trial: TrialState) -> String {
    let stands_in = if trial.stands_in {
        ", with Night Shift on for it"
    } else {
        ""
    };
    format!("Tour: running in the app{stands_in}, so rules and changes you make last until it ends")
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
fn rules(rules: &[ListedRule], zone: &TimeZone) -> String {
    if rules.is_empty() {
        return "none".to_owned();
    }
    rules
        .iter()
        .map(|listed| rule(listed, zone))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Each place's temporary rule, and everywhere's indefinite one after it, a line per place, with
/// times in `zone`: "app:com.figma.Desktop: on until 07:00", "Everywhere: on until 07:00, then off
/// until you turn it on".
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

/// One end of custom hours, as the clock reads it: "07:00".
fn time_of_day(time: TimeOfDay) -> String {
    format!("{:02}:{:02}", time.hour, time.minute)
}

#[cfg(test)]
mod tests {
    use nightcap_protocol::Position;

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
            "on at app:com.figma.Desktop until 07:00"
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
    fn a_problem_says_what_stopped_working_or_cant_work() {
        for (found, said) in [
            (
                CheckDetail::NightShift(NightShiftDetail::Unavailable),
                "Night Shift isn’t available on this Mac.",
            ),
            (
                CheckDetail::NightShift(NightShiftDetail::SetAside),
                "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on.",
            ),
            (
                CheckDetail::NightShift(NightShiftDetail::NoSunTimes),
                "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. Custom hours work without them.",
            ),
            (
                CheckDetail::Accessibility(AccessibilityDetail::NotAllowed),
                "Accessibility isn’t allowed, so website checking can’t read the front tab.",
            ),
            (
                CheckDetail::Notifications(NotificationsDetail::SwitchedOff),
                "Notifications were allowed, but they’re switched off in System Settings.",
            ),
            (
                CheckDetail::OpenAtLogin(OpenAtLoginDetail::AwaitingApproval),
                "macOS is waiting for your approval in Login Items.",
            ),
            (
                CheckDetail::OpenAtLogin(OpenAtLoginDetail::Removed),
                "Nightcap no longer opens at login: it was removed in System Settings.",
            ),
        ] {
            assert_eq!(check(&found), said, "{found:?}");
        }
    }

    #[test]
    fn the_files_checks_say_what_is_wrong_and_where() {
        let config = |found| check(&CheckDetail::ConfigFile(found));
        let ad_hoc = |found| check(&CheckDetail::AdHocRules(found));
        let path = || "/Users/me/.config/nightcap/config.toml".to_owned();
        let invalid = |error| config(ConfigFileDetail::Error(error));

        for (failure, said) in [
            (
                IoFailure::NotFound,
                "The config file is invalid: couldn't read /Users/me/.config/nightcap/config.toml: it, or a folder on its path, isn't there",
            ),
            (
                IoFailure::Other,
                "The config file is invalid: couldn't read /Users/me/.config/nightcap/config.toml",
            ),
        ] {
            assert_eq!(
                invalid(FileError::Unreadable {
                    path: path(),
                    failure
                }),
                said,
                "{failure:?}"
            );
        }
        assert_eq!(
            invalid(FileError::Malformed {
                path: path(),
                at: Some(Position {
                    line: 4,
                    column: 12
                }),
            }),
            "The config file is invalid: /Users/me/.config/nightcap/config.toml: line 4, column 12 isn't valid TOML"
        );
        assert_eq!(
            invalid(FileError::Malformed {
                path: path(),
                at: None
            }),
            "The config file is invalid: /Users/me/.config/nightcap/config.toml isn't valid TOML"
        );
        assert_eq!(
            invalid(FileError::Invalid {
                path: path(),
                line: 4,
                problem: Problem::UnknownKey {
                    table: vec!["rules".to_owned(), "off".to_owned()],
                    key: "domian".to_owned(),
                    expected: vec!["app".to_owned(), "domain".to_owned()],
                },
            }),
            "The config file is invalid: /Users/me/.config/nightcap/config.toml: line 4: `rules.off.domian` isn't a key here; use `app` or `domain`"
        );
        assert_eq!(
            ad_hoc(AdHocRulesDetail::Off(FileError::Newer {
                path: "/Users/me/.local/share/nightcap/rules.toml".to_owned(),
                version: 3,
            })),
            "Ad-hoc rules are off: /Users/me/.local/share/nightcap/rules.toml is version 3, from a newer Nightcap than this one."
        );
    }

    #[test]
    fn a_files_problem_names_the_key_entry_or_value_as_written() {
        let keys = |keys: &[&str]| keys.iter().map(|&key| key.to_owned()).collect::<Vec<_>>();
        for (found, said) in [
            (
                Problem::UnknownKey {
                    table: keys(&["rules", "off"]),
                    key: "domian".to_owned(),
                    expected: keys(&["app", "domain"]),
                },
                "`rules.off.domian` isn't a key here; use `app` or `domain`",
            ),
            (
                Problem::UnknownKey {
                    table: Vec::new(),
                    key: "website".to_owned(),
                    expected: keys(&["websites", "lengths", "night_shift", "rules"]),
                },
                "`website` isn't a key here; use `websites`, `lengths`, `night_shift` or `rules`",
            ),
            (
                Problem::UnknownKey {
                    table: keys(&["rules"]),
                    key: "of".to_owned(),
                    expected: keys(&["off"]),
                },
                "`rules.of` isn't a key here; use `off`",
            ),
            (
                Problem::UnknownKey {
                    table: Vec::new(),
                    key: "x".to_owned(),
                    expected: Vec::new(),
                },
                "`x` isn't a key here",
            ),
            (
                Problem::WrongType {
                    table: keys(&["rules", "off"]),
                    key: "app".to_owned(),
                    found: ValueType::String,
                    expected: ValueType::Strings,
                },
                "`rules.off.app` is a list of strings, not a string",
            ),
        ] {
            assert_eq!(problem(&found), said, "{found:?}");
        }
    }

    #[test]
    fn adding_a_rule_says_whether_it_was_added_changed_or_already_there() {
        let figma: RuleIdentity = "app:com.figma.Desktop".parse().unwrap();

        assert_eq!(
            rule_added(&figma, OnOff::Off, RuleAdded::New { set_by: SetBy::You }),
            "Added app:com.figma.Desktop, which turns Night Shift off."
        );
        assert_eq!(
            rule_added(
                &figma,
                OnOff::On,
                RuleAdded::New {
                    set_by: SetBy::Trial
                }
            ),
            "Added app:com.figma.Desktop, which turns Night Shift on until the tour ends."
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
        let config = || "/Users/me/.config/nightcap/config.toml".to_owned();
        let rules = || "/Users/me/.local/share/nightcap/rules.toml".to_owned();

        for (why, said) in [
            (
                Refusal::TooLong {
                    length: jiff::SignedDuration::from_mins(90),
                },
                "1h 30m is too long",
            ),
            (
                Refusal::ConfigSays {
                    identity: "app:com.figma.Desktop".parse().unwrap(),
                    says: OnOff::Off,
                },
                "app:com.figma.Desktop comes from the config, which says off; change it there",
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
                    failure: IoFailure::IsAFolder,
                },
                "couldn't save /Users/me/.local/share/nightcap/rules.toml: it's a folder",
            ),
            (
                Refusal::AdHocRulesNotSaved {
                    path: rules(),
                    failure: IoFailure::Other,
                },
                "couldn't save /Users/me/.local/share/nightcap/rules.toml",
            ),
            (
                Refusal::InvalidConfig(FileError::Invalid {
                    path: config(),
                    line: 2,
                    problem: Problem::WrongType {
                        table: vec!["rules".to_owned(), "off".to_owned()],
                        key: "app".to_owned(),
                        found: ValueType::String,
                        expected: ValueType::Strings,
                    },
                }),
                "/Users/me/.config/nightcap/config.toml: line 2: `rules.off.app` is a list of strings, not a string",
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

    // Staged cores exist only in debug builds.
    #[cfg(debug_assertions)]
    mod status {
        use jiff::SignedDuration;
        use nightcap_core::{AppCommand, Fact, Scenario};
        use nightcap_protocol::Command;

        use super::*;
        use crate::client::tests::staged_report;

        /// At 23:00 in a self-managed release, Safari in front on www.figma.com, which a config
        /// rule keeps off while Night Shift, on by the custom hours at a strength of 60, has yet
        /// to turn off; with everywhere on until 23:30, an ad-hoc rule for Mail, and
        /// notifications switched off after being allowed.
        fn on_figma() -> Scenario {
            Scenario::at("2026-10-16T23:00:00+02:00".parse().unwrap())
                .installed(Install::SelfManaged)
                .config(
                    "websites = true\n[night_shift]\nstrength = 40\n[rules.off]\ndomain = [\"figma.com\"]\n",
                )
                .then(Fact::StatusChanged {
                    enabled: true,
                    schedule: Schedule::Custom {
                        from: TimeOfDay {
                            hour: 22,
                            minute: 0,
                        },
                        to: TimeOfDay { hour: 7, minute: 0 },
                    },
                    sun_schedule_set_aside: false,
                })
                .then(Fact::StrengthChanged { value: 60 })
                .then(Fact::PermissionsChanged {
                    accessibility: true,
                    notifications: NotificationPermission::Granted,
                })
                .then(Fact::PermissionsChanged {
                    accessibility: true,
                    notifications: NotificationPermission::Denied,
                })
                .then(Command::AddRule {
                    identity: "app:com.apple.mail".parse().unwrap(),
                    says: OnOff::On,
                })
                .then(Command::Set {
                    place: Place::Everywhere,
                    state: OnOff::On,
                    length: Lasting::For(SignedDuration::from_mins(30)),
                })
                .then(Fact::AppActivated {
                    bundle_id: Some("com.apple.Safari".to_owned()),
                    is_browser: true,
                })
                .then(Fact::HostChanged {
                    generation: 1,
                    host: Some("www.figma.com".to_owned()),
                })
        }

        fn zurich() -> TimeZone {
            TimeZone::get("Europe/Zurich").unwrap()
        }

        /// What `nightcap status` shows for `scenario`, in Zurich.
        fn shown(scenario: Scenario) -> Vec<String> {
            status(&staged_report(scenario), &zurich())
        }

        #[test]
        fn status_reads_a_line_at_a_time_with_times_in_the_zone() {
            assert_eq!(
                shown(on_figma()),
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
        fn status_says_the_deciding_rule_is_snoozed_as_the_rules_answer_serves_it() {
            let mut report = staged_report(on_figma().then(Command::Set {
                place: Place::Domain("figma.com".parse().unwrap()),
                state: OnOff::On,
                length: Lasting::UntilScheduleTurns,
            }));
            let night_shift_line = |report: &Report| status(report, &zurich()).swap_remove(1);

            assert_eq!(
                night_shift_line(&report),
                "Night Shift: on at domain:figma.com until 07:00, its config rule snoozed"
            );
            for listed in &mut report.rules {
                listed.snooze = None;
            }
            assert_eq!(
                night_shift_line(&report),
                "Night Shift: on at domain:figma.com until 07:00"
            );
        }

        #[test]
        fn status_says_the_tour_runs_and_whether_night_shift_is_on_for_it() {
            let second_line = |scenario| shown(scenario).swap_remove(1);

            assert_eq!(
                second_line(on_figma().then(AppCommand::BeginTrial { stands_in: false })),
                "Tour: running in the app, so rules and changes you make last until it ends"
            );
            assert_eq!(
                second_line(on_figma().then(AppCommand::BeginTrial { stands_in: true })),
                "Tour: running in the app, with Night Shift on for it, so rules and changes you make last until it ends"
            );
            assert_eq!(
                second_line(on_figma()),
                "Night Shift: on, off by the config rule for domain:figma.com"
            );
        }

        #[test]
        fn status_says_whether_the_install_manages_itself() {
            let first_line = |install| shown(on_figma().installed(install)).swap_remove(0);

            assert_eq!(
                first_line(Install::SelfManaged),
                "Nightcap 1.2.0, self-managed"
            );
            assert_eq!(
                first_line(Install::NotSelfManaged(NotSelfManaged::ManagedInstall)),
                "Nightcap 1.2.0, managed by its installer"
            );
            assert_eq!(
                first_line(Install::NotSelfManaged(NotSelfManaged::DevBuild)),
                "Nightcap 1.2.0, a dev build"
            );
        }
    }
}
