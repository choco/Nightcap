//! What `--json` prints: the CLI's own schema for scripts, mapped from the protocol's answers so
//! the protocol can change underneath it (ADR-0010). `docs/installing.md` documents it as a
//! public interface: it changes only on purpose, and `schema` goes up when it does.

use jiff::Timestamp;
use nightcap_protocol as protocol;
use serde::Serialize;

use crate::client::Report;
use crate::wording;

/// The schema's version, which every change to what `--json` prints raises.
const SCHEMA: u32 = 1;

/// What `nightcap status --json` prints.
#[derive(Debug, Serialize)]
pub(crate) struct Status {
    schema: u32,
    nightcap: Nightcap,
    night_shift: NightShift,
    front: Front,
    tour: Option<Tour>,
    rules: Vec<ListedRule>,
    changes: Vec<Rule>,
    website_checking: WebsiteChecking,
    config: Config,
    needs_attention: Vec<Problem>,
}

impl From<&Report> for Status {
    fn from(report: &Report) -> Self {
        let (rules, changes): (Vec<_>, Vec<_>) = report
            .rules
            .iter()
            .partition(|listed| wording::is_app_or_site_rule(&listed.rule));
        Self {
            schema: SCHEMA,
            nightcap: Nightcap::from(&report.about),
            night_shift: NightShift::from(&report.night_shift),
            front: Front::from(&report.front),
            tour: report.trial.map(Tour::from),
            rules: rules.into_iter().map(ListedRule::from).collect(),
            changes: changes
                .into_iter()
                .map(|listed| Rule::from(&listed.rule))
                .collect(),
            website_checking: WebsiteChecking::from(report.preferences.website_checking),
            config: Config {
                path: report.config.path.clone(),
                error: wording::config_error(&report.config),
            },
            needs_attention: report
                .checks
                .checks
                .iter()
                .filter_map(Problem::of)
                .collect(),
        }
    }
}

/// What `nightcap rules list --json` prints: the apps' and sites' rules, as `nightcap rules`
/// lists them.
#[derive(Debug, Serialize)]
pub(crate) struct Rules {
    schema: u32,
    rules: Vec<ListedRule>,
}

impl From<&[protocol::ListedRule]> for Rules {
    fn from(rules: &[protocol::ListedRule]) -> Self {
        Self {
            schema: SCHEMA,
            rules: rules
                .iter()
                .filter(|listed| wording::is_app_or_site_rule(&listed.rule))
                .map(ListedRule::from)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct Nightcap {
    version: String,
    install: Install,
}

impl From<&protocol::About> for Nightcap {
    fn from(about: &protocol::About) -> Self {
        Self {
            version: about.version.clone(),
            install: Install::from(about),
        }
    }
}

/// Who keeps Nightcap up to date and opens it at login (ADR-0011).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Install {
    /// A release that updates itself, and opens itself at login if you want.
    SelfManaged,
    /// An installer's, which updates and starts it.
    Managed,
    /// A dev build, which does neither.
    DevBuild,
}

impl From<&protocol::About> for Install {
    fn from(about: &protocol::About) -> Self {
        // A self-managed install that doesn't manage itself is a dev build.
        match (about.manages_itself, about.install) {
            (true, _) => Self::SelfManaged,
            (false, protocol::Install::Managed) => Self::Managed,
            (false, protocol::Install::SelfManaged) => Self::DevBuild,
        }
    }
}

#[derive(Debug, Serialize)]
struct NightShift {
    state: Observed,
    strength: Option<u8>,
    /// As the config writes it: "off", "sunset-to-sunrise" or custom hours, "22:00-07:00".
    schedule: Option<String>,
    scheduled: Option<State>,
    next_on: Option<Timestamp>,
    next_off: Option<Timestamp>,
}

impl From<&protocol::NightShiftState> for NightShift {
    fn from(night_shift: &protocol::NightShiftState) -> Self {
        Self {
            state: Observed::from(night_shift.state),
            strength: night_shift.strength,
            schedule: night_shift.schedule.map(schedule),
            scheduled: night_shift.scheduled.map(State::from),
            next_on: night_shift.next_on,
            next_off: night_shift.next_off,
        }
    }
}

fn schedule(schedule: protocol::Schedule) -> String {
    let time = |time: protocol::TimeOfDay| format!("{:02}:{:02}", time.hour, time.minute);
    match schedule {
        protocol::Schedule::Off => "off".to_owned(),
        protocol::Schedule::SunsetToSunrise => "sunset-to-sunrise".to_owned(),
        protocol::Schedule::Custom { from, to } => format!("{}-{}", time(from), time(to)),
    }
}

/// Night Shift as last observed.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Observed {
    On,
    Off,
    Unavailable,
}

impl From<protocol::NightShift> for Observed {
    fn from(night_shift: protocol::NightShift) -> Self {
        match night_shift {
            protocol::NightShift::On => Self::On,
            protocol::NightShift::Off => Self::Off,
            protocol::NightShift::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum State {
    On,
    Off,
}

impl From<protocol::OnOff> for State {
    fn from(state: protocol::OnOff) -> Self {
        match state {
            protocol::OnOff::On => Self::On,
            protocol::OnOff::Off => Self::Off,
        }
    }
}

/// What's in front, and what decides Night Shift there.
#[derive(Debug, Serialize)]
struct Front {
    app: Option<String>,
    site: Option<String>,
    decision: Decision,
}

impl From<&protocol::Front> for Front {
    fn from(front: &protocol::Front) -> Self {
        Self {
            app: front.app.as_ref().map(|app| app.as_str().to_owned()),
            site: front.site.as_ref().map(|site| site.as_str().to_owned()),
            decision: Decision::from(&front.decision),
        }
    }
}

#[derive(Debug, Serialize)]
struct Decision {
    state: Option<State>,
    place: String,
    reason: Option<Reason>,
}

impl From<&protocol::Decision> for Decision {
    fn from(decision: &protocol::Decision) -> Self {
        Self {
            state: decision.state.map(State::from),
            place: place(&decision.place),
            reason: decision.reasons.first().map(Reason::from),
        }
    }
}

/// What a decision rests on first: its place's shortest rule, the tour holding Night Shift on,
/// or the schedule.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Reason {
    Rule(Rule),
    Tour,
    Schedule { until: Option<Timestamp> },
}

impl From<&protocol::Reason> for Reason {
    fn from(reason: &protocol::Reason) -> Self {
        match reason {
            protocol::Reason::Rule(rule) => Self::Rule(Rule::from(rule)),
            protocol::Reason::StandIn => Self::Tour,
            protocol::Reason::Schedule { until, .. } => Self::Schedule { until: *until },
        }
    }
}

#[derive(Debug, Serialize)]
struct Rule {
    place: String,
    says: State,
    set_by: SetBy,
    length: Length,
    /// When it ends, where it has an end in sight.
    until: Option<Timestamp>,
}

impl From<&protocol::Rule> for Rule {
    fn from(rule: &protocol::Rule) -> Self {
        Self {
            place: place(&rule.place),
            says: State::from(rule.says),
            set_by: SetBy::from(rule.set_by),
            length: Length::from(rule.length),
            until: rule.length.end(),
        }
    }
}

/// An app's or a site's rule, with the change over it that snoozes it.
#[derive(Debug, Serialize)]
struct ListedRule {
    #[serde(flatten)]
    rule: Rule,
    snooze: Option<Rule>,
}

impl From<&protocol::ListedRule> for ListedRule {
    fn from(listed: &protocol::ListedRule) -> Self {
        Self {
            rule: Rule::from(&listed.rule),
            snooze: listed.snooze.as_ref().map(Rule::from),
        }
    }
}

/// `everywhere`, `app:com.figma.Desktop` or `domain:figma.com`, as the CLI's arguments take a
/// place.
fn place(place: &protocol::Place) -> String {
    match place {
        protocol::Place::Everywhere => "everywhere".to_owned(),
        protocol::Place::App(bundle_id) => format!("app:{}", bundle_id.as_str()),
        protocol::Place::Domain(domain) => format!("domain:{}", domain.as_str()),
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum SetBy {
    Config,
    You,
    /// You, during the app's tour, until it ends (ADR-0016).
    Tour,
    OutsideChange,
}

impl From<protocol::SetBy> for SetBy {
    fn from(set_by: protocol::SetBy) -> Self {
        match set_by {
            protocol::SetBy::Config => Self::Config,
            protocol::SetBy::You => Self::You,
            protocol::SetBy::Trial => Self::Tour,
            protocol::SetBy::OutsideChange => Self::OutsideChange,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Length {
    Indefinite,
    UntilSchedule,
    UntilTime,
}

impl From<protocol::Length> for Length {
    fn from(length: protocol::Length) -> Self {
        match length {
            protocol::Length::Indefinite => Self::Indefinite,
            protocol::Length::UntilScheduleTurns { .. } => Self::UntilSchedule,
            protocol::Length::Until { .. } => Self::UntilTime,
        }
    }
}

/// The app's tour, while it runs (ADR-0016).
#[derive(Debug, Serialize)]
struct Tour {
    /// It holds Night Shift on in place of the schedule.
    stands_in: bool,
}

impl From<protocol::TrialState> for Tour {
    fn from(trial: protocol::TrialState) -> Self {
        Self {
            stands_in: trial.stands_in,
        }
    }
}

#[derive(Debug, Serialize)]
struct WebsiteChecking {
    state: State,
    from_config: bool,
}

impl From<protocol::WebsiteChecking> for WebsiteChecking {
    fn from(website_checking: protocol::WebsiteChecking) -> Self {
        Self {
            state: State::from(website_checking.state),
            from_config: website_checking.from_config,
        }
    }
}

#[derive(Debug, Serialize)]
struct Config {
    path: String,
    error: Option<String>,
}

/// A check that needs attention, and what it found in the CLI's words.
#[derive(Debug, Serialize)]
struct Problem {
    check: Check,
    state: ProblemState,
    message: String,
}

impl Problem {
    /// The check's problem, if it has one.
    fn of(check: &protocol::Check) -> Option<Self> {
        let state = match check.state {
            protocol::CheckState::Drifted => ProblemState::Drifted,
            protocol::CheckState::Broken => ProblemState::Broken,
            protocol::CheckState::Ok | protocol::CheckState::Off => return None,
        };
        Some(Self {
            check: Check::from(check.detail.kind()),
            state,
            message: wording::check(&check.detail),
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Check {
    NightShift,
    Accessibility,
    Notifications,
    OpenAtLogin,
    ConfigFile,
    AdHocRules,
}

impl From<protocol::CheckKind> for Check {
    fn from(kind: protocol::CheckKind) -> Self {
        match kind {
            protocol::CheckKind::NightShift => Self::NightShift,
            protocol::CheckKind::Accessibility => Self::Accessibility,
            protocol::CheckKind::Notifications => Self::Notifications,
            protocol::CheckKind::OpenAtLogin => Self::OpenAtLogin,
            protocol::CheckKind::ConfigFile => Self::ConfigFile,
            protocol::CheckKind::AdHocRules => Self::AdHocRules,
        }
    }
}

/// How a check needs attention.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProblemState {
    /// Something you set up stopped working: restoring it or accepting the change settles it.
    Drifted,
    /// It can only be fixed.
    Broken,
}

// Each document is pinned, so a change to the schema changes one, and `SCHEMA` with it. Staged
// cores exist only in debug builds.
#[cfg(test)]
#[cfg(debug_assertions)]
mod tests {
    use nightcap_core::{AppCommand, Fact, Scenario};
    use nightcap_protocol::{
        Command, Lasting, NotificationPermission, OnOff, Place, Response, Schedule, TimeOfDay,
    };

    use super::*;
    use crate::client;

    /// Where the pinned documents put the config, wherever the tests' XDG directories are.
    const CONFIG: &str = "/Users/me/.config/nightcap/config.toml";

    /// Night Shift as CoreBrightness reports it, by custom hours from 22:00 to 07:00.
    fn reported(enabled: bool) -> Fact {
        Fact::StatusChanged {
            enabled,
            schedule: Schedule::Custom {
                from: TimeOfDay {
                    hour: 22,
                    minute: 0,
                },
                to: TimeOfDay { hour: 7, minute: 0 },
            },
            sun_schedule_set_aside: false,
        }
    }

    /// 23:00 in Zurich, with Night Shift on by the custom hours at a strength of 60.
    fn on_by_the_schedule() -> Scenario {
        Scenario::at("2026-10-16T23:00:00+02:00".parse().unwrap())
            .then(reported(true))
            .then(Fact::StrengthChanged { value: 60 })
    }

    /// Then Figma, which a config rule keeps off, in front, and Night Shift reported off.
    fn forced() -> Scenario {
        on_by_the_schedule()
            .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
            .then(Fact::AppActivated {
                bundle_id: Some("com.figma.Desktop".to_owned()),
                is_browser: false,
            })
            .then(reported(false))
    }

    /// What `--json` prints for `scenario`, as `print` maps the answers its staged core gives,
    /// with the config where `CONFIG` puts it and the version 1.2.0. It's indented here, to
    /// read; the CLI prints it on one line.
    fn printed<T: Serialize>(scenario: Scenario, print: impl Fn(&Report) -> T) -> String {
        let staged = scenario.stage().unwrap();
        let Ok(mut report) = client::report(|request| match staged.request(request.clone()) {
            (_, Response::Ok(payload)) => Ok(payload),
            (_, response) => panic!("expected answers, got {response:?}"),
        }) else {
            panic!("the status reads its answers");
        };
        report.about.version = "1.2.0".to_owned();
        serde_json::to_string_pretty(&print(&report))
            .unwrap()
            .replace(&report.config.path, CONFIG)
    }

    fn status(scenario: Scenario) -> String {
        printed(scenario, |report| Status::from(report))
    }

    #[test]
    fn on_by_the_schedule_the_schedule_decides_until_its_next_turn() {
        assert_eq!(
            status(on_by_the_schedule()),
            r#"{
  "schema": 1,
  "nightcap": {
    "version": "1.2.0",
    "install": "managed"
  },
  "night_shift": {
    "state": "on",
    "strength": 60,
    "schedule": "22:00-07:00",
    "scheduled": "on",
    "next_on": "2026-10-17T20:00:00Z",
    "next_off": "2026-10-17T05:00:00Z"
  },
  "front": {
    "app": null,
    "site": null,
    "decision": {
      "state": "on",
      "place": "everywhere",
      "reason": {
        "kind": "schedule",
        "until": "2026-10-17T05:00:00Z"
      }
    }
  },
  "tour": null,
  "rules": [],
  "changes": [],
  "website_checking": {
    "state": "off",
    "from_config": false
  },
  "config": {
    "path": "/Users/me/.config/nightcap/config.toml",
    "error": null
  },
  "needs_attention": []
}"#
        );
    }

    #[test]
    fn a_config_rule_forcing_an_app_off_decides_while_it_is_in_front() {
        assert_eq!(
            status(forced()),
            r#"{
  "schema": 1,
  "nightcap": {
    "version": "1.2.0",
    "install": "managed"
  },
  "night_shift": {
    "state": "off",
    "strength": 60,
    "schedule": "22:00-07:00",
    "scheduled": "on",
    "next_on": "2026-10-17T20:00:00Z",
    "next_off": "2026-10-17T05:00:00Z"
  },
  "front": {
    "app": "com.figma.Desktop",
    "site": null,
    "decision": {
      "state": "off",
      "place": "app:com.figma.Desktop",
      "reason": {
        "kind": "rule",
        "place": "app:com.figma.Desktop",
        "says": "off",
        "set_by": "config",
        "length": "indefinite",
        "until": null
      }
    }
  },
  "tour": null,
  "rules": [
    {
      "place": "app:com.figma.Desktop",
      "says": "off",
      "set_by": "config",
      "length": "indefinite",
      "until": null,
      "snooze": null
    }
  ],
  "changes": [],
  "website_checking": {
    "state": "off",
    "from_config": false
  },
  "config": {
    "path": "/Users/me/.config/nightcap/config.toml",
    "error": null
  },
  "needs_attention": []
}"#
        );
    }

    #[test]
    fn a_snoozed_rule_carries_the_change_that_snoozes_it() {
        let snoozed = forced()
            .then(Command::Set {
                place: Place::App("com.figma.Desktop".parse().unwrap()),
                state: OnOff::On,
                length: Lasting::UntilScheduleTurns,
            })
            .then(reported(true));

        assert_eq!(
            status(snoozed),
            r#"{
  "schema": 1,
  "nightcap": {
    "version": "1.2.0",
    "install": "managed"
  },
  "night_shift": {
    "state": "on",
    "strength": 60,
    "schedule": "22:00-07:00",
    "scheduled": "on",
    "next_on": "2026-10-17T20:00:00Z",
    "next_off": "2026-10-17T05:00:00Z"
  },
  "front": {
    "app": "com.figma.Desktop",
    "site": null,
    "decision": {
      "state": "on",
      "place": "app:com.figma.Desktop",
      "reason": {
        "kind": "rule",
        "place": "app:com.figma.Desktop",
        "says": "on",
        "set_by": "you",
        "length": "until_schedule",
        "until": "2026-10-17T05:00:00Z"
      }
    }
  },
  "tour": null,
  "rules": [
    {
      "place": "app:com.figma.Desktop",
      "says": "off",
      "set_by": "config",
      "length": "indefinite",
      "until": null,
      "snooze": {
        "place": "app:com.figma.Desktop",
        "says": "on",
        "set_by": "you",
        "length": "until_schedule",
        "until": "2026-10-17T05:00:00Z"
      }
    }
  ],
  "changes": [
    {
      "place": "app:com.figma.Desktop",
      "says": "on",
      "set_by": "you",
      "length": "until_schedule",
      "until": "2026-10-17T05:00:00Z"
    }
  ],
  "website_checking": {
    "state": "off",
    "from_config": false
  },
  "config": {
    "path": "/Users/me/.config/nightcap/config.toml",
    "error": null
  },
  "needs_attention": []
}"#
        );
    }

    #[test]
    fn the_checks_that_need_attention_are_listed_with_the_configs_error() {
        let invalid = on_by_the_schedule()
            .config("[night_shift]\nstrength = 101\n")
            .then(Fact::PermissionsChanged {
                accessibility: false,
                notifications: NotificationPermission::Granted,
            })
            .then(Command::Websites(OnOff::On));

        assert_eq!(
            status(invalid),
            r#"{
  "schema": 1,
  "nightcap": {
    "version": "1.2.0",
    "install": "managed"
  },
  "night_shift": {
    "state": "on",
    "strength": 60,
    "schedule": "22:00-07:00",
    "scheduled": "on",
    "next_on": "2026-10-17T20:00:00Z",
    "next_off": "2026-10-17T05:00:00Z"
  },
  "front": {
    "app": null,
    "site": null,
    "decision": {
      "state": "on",
      "place": "everywhere",
      "reason": {
        "kind": "schedule",
        "until": "2026-10-17T05:00:00Z"
      }
    }
  },
  "tour": null,
  "rules": [],
  "changes": [],
  "website_checking": {
    "state": "on",
    "from_config": false
  },
  "config": {
    "path": "/Users/me/.config/nightcap/config.toml",
    "error": "/Users/me/.config/nightcap/config.toml: line 2: night_shift.strength is 0 to 100, not 101"
  },
  "needs_attention": [
    {
      "check": "accessibility",
      "state": "drifted",
      "message": "Accessibility isn’t allowed, so website checking can’t read the front tab."
    },
    {
      "check": "config_file",
      "state": "broken",
      "message": "The config file is invalid: /Users/me/.config/nightcap/config.toml: line 2: night_shift.strength is 0 to 100, not 101"
    }
  ]
}"#
        );
    }

    #[test]
    fn the_tour_says_it_runs_and_decides_while_it_stands_in() {
        let tour = Scenario::at("2026-10-16T12:00:00+02:00".parse().unwrap())
            .then(reported(false))
            .then(AppCommand::BeginTrial { stands_in: true })
            .then(reported(true));

        assert_eq!(
            status(tour),
            r#"{
  "schema": 1,
  "nightcap": {
    "version": "1.2.0",
    "install": "managed"
  },
  "night_shift": {
    "state": "on",
    "strength": null,
    "schedule": "22:00-07:00",
    "scheduled": "off",
    "next_on": "2026-10-16T20:00:00Z",
    "next_off": "2026-10-17T05:00:00Z"
  },
  "front": {
    "app": null,
    "site": null,
    "decision": {
      "state": "on",
      "place": "everywhere",
      "reason": {
        "kind": "tour"
      }
    }
  },
  "tour": {
    "stands_in": true
  },
  "rules": [],
  "changes": [],
  "website_checking": {
    "state": "off",
    "from_config": false
  },
  "config": {
    "path": "/Users/me/.config/nightcap/config.toml",
    "error": null
  },
  "needs_attention": []
}"#
        );
    }

    #[test]
    fn the_rules_list_has_the_apps_and_sites_rules_with_their_snoozes() {
        let snoozed = forced()
            .then(Command::AddRule {
                identity: "domain:figma.com".parse().unwrap(),
                says: OnOff::Off,
            })
            .then(Command::Set {
                place: Place::App("com.figma.Desktop".parse().unwrap()),
                state: OnOff::On,
                length: Lasting::For(jiff::SignedDuration::from_mins(30)),
            });

        assert_eq!(
            printed(snoozed, |report| Rules::from(&report.rules[..])),
            r#"{
  "schema": 1,
  "rules": [
    {
      "place": "app:com.figma.Desktop",
      "says": "off",
      "set_by": "config",
      "length": "indefinite",
      "until": null,
      "snooze": {
        "place": "app:com.figma.Desktop",
        "says": "on",
        "set_by": "you",
        "length": "until_time",
        "until": "2026-10-16T21:30:00Z"
      }
    },
    {
      "place": "domain:figma.com",
      "says": "off",
      "set_by": "you",
      "length": "indefinite",
      "until": null,
      "snooze": null
    }
  ]
}"#
        );
    }
}
