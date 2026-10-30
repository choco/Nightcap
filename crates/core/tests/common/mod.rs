//! What the integration tests share: the scenarios they start from, the shell's reports, the
//! CLI's queries, and the rules and checks they expect.

#![allow(
    dead_code,
    reason = "each test crate uses only some of these, so `expect` would fail in most"
)]
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "clippy.toml lets tests unwrap and panic, but not the helpers they share"
)]

use jiff::{SignedDuration, Timestamp};
use nightcap_core::{Effect, Fact, File, Outcome, Scenario, Staged, SwitchedBy};
use nightcap_protocol::{
    Answer, Check, CheckKind, Command, Front, Lasting, Length, ListedRule, NotificationPermission,
    OnOff, Payload, Place, PlaceInFront, Query, Refusal, Request, Response, Rule, RuleIdentity,
    Rules, Schedule, SetBy, TimeOfDay,
};

/// The instant `instant` names, such as "2026-10-16T23:00:00+02:00".
pub(crate) fn at(instant: &str) -> Timestamp {
    instant.parse().unwrap()
}

/// Noon in Zurich, with no files and nothing reported yet.
pub(crate) fn noon() -> Scenario {
    Scenario::at(at("2026-10-16T12:00:00+02:00"))
}

/// 23:00 in Zurich, inside the custom hours.
pub(crate) fn late() -> Scenario {
    Scenario::at(at("2026-10-16T23:00:00+02:00"))
}

/// At 20:00, with Night Shift off by the custom hours until 22:00.
pub(crate) fn off_by_the_schedule() -> Scenario {
    Scenario::at(at("2026-10-16T20:00:00+02:00")).then(reported(false, custom_hours()))
}

/// At 23:00, with Night Shift on by the custom hours until 07:00.
pub(crate) fn on_by_the_schedule() -> Scenario {
    late().then(reported(true, custom_hours()))
}

/// Starts at 23:00 with Night Shift on by the custom hours, then brings Figma, which a config
/// rule forces off, to the front, and reports the result.
pub(crate) fn forced() -> Staged {
    let staged = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .then(reported(true, custom_hours()))
        .stage()
        .unwrap();
    assert_eq!(
        activate(&staged, "com.figma.Desktop"),
        vec![Effect::SetNightShift { enabled: false }]
    );
    echo(&staged, false);
    staged
}

/// At 23:00 with Night Shift on by the custom hours, website checking on and a config rule for
/// figma.com.
pub(crate) fn with_figma_domain() -> Scenario {
    late()
        .config("websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n")
        .then(accessibility_granted())
        .then(reported(true, custom_hours()))
}

/// Safari in front on figma.com, Night Shift forced off and reported off.
pub(crate) fn on_figma() -> Scenario {
    with_figma_domain()
        .then(activate_browser_event("com.apple.Safari"))
        .then(host_event(1, "www.figma.com"))
        .then(written(false, custom_hours(), "2026-10-16T23:00:00+02:00"))
}

/// At noon, off by the custom hours, with Books in front.
pub(crate) fn in_books_at_noon() -> Scenario {
    noon()
        .then(reported(false, custom_hours()))
        .then(activate_event("com.apple.iBooksX"))
}

/// Sends a command as the CLI does, with its effects and the response.
pub(crate) fn send(staged: &Staged, command: Command) -> (Vec<Effect>, Response) {
    let (outcome, response) = staged.request(Request::Command(command));
    (outcome.unwrap().effects, response)
}

/// What `queries` answer over the socket, in the order asked, as the CLI asks them in one
/// request; asking changes nothing.
pub(crate) fn ask(staged: &Staged, queries: Vec<Query>) -> Vec<Answer> {
    let (outcome, response) = staged.request(Request::Queries(queries));
    assert_eq!(outcome, None);
    let Response::Ok(Payload::Answers(answers)) = response else {
        panic!("expected answers, got {response:?}");
    };
    answers
}

/// What the rules query and what's in front answer, in one request.
fn rules_and_front(staged: &Staged) -> (Rules, Front) {
    let Ok([Answer::Rules(rules), Answer::Front(front)]) =
        <[Answer; 2]>::try_from(ask(staged, vec![Query::Rules, Query::Front]))
    else {
        panic!("expected the rules and what's in front");
    };
    (rules, *front)
}

/// The apps' and sites' indefinite rules, as `nightcap rules` lists them.
pub(crate) fn rules(staged: &Staged) -> Vec<Rule> {
    let (rules, _) = rules_and_front(staged);
    rules.rules.into_iter().map(|listed| listed.rule).collect()
}

/// Every rule a reset takes off: the temporary ones, and everywhere's indefinite one.
pub(crate) fn resettable(staged: &Staged) -> Vec<Rule> {
    let (rules, front) = rules_and_front(staged);
    (rules.temporary.into_iter())
        .chain(front.everywhere.indefinite)
        .collect()
}

/// A rule as the rules query lists it while nothing snoozes it.
pub(crate) fn listed(rule: Rule) -> ListedRule {
    ListedRule { rule, snooze: None }
}

/// How `place` stands while in front.
pub(crate) fn standing(staged: &Staged, place: &Place) -> PlaceInFront {
    let front = staged.core().snapshot().front();
    front
        .places
        .into_iter()
        .chain([front.everywhere])
        .find(|at| at.place == *place)
        .unwrap()
}

/// Why the core refused a request, which it must have.
pub(crate) fn refusal(response: Response) -> Refusal {
    let Response::Refused(refusal) = response else {
        panic!("expected a refusal, got {response:?}");
    };
    refusal
}

/// The ad-hoc rules `rules.toml` keeps, as a core started on it alone would load them.
pub(crate) fn kept_ad_hoc(staged: &Staged) -> Vec<Rule> {
    let alone = Scenario {
        rules: staged.file(File::Rules),
        ..noon()
    };
    rules(&alone.stage().unwrap())
}

/// Night Shift as CoreBrightness reports it with no write standing, as the schedule leaves it,
/// with sunset to sunrise not set aside.
pub(crate) fn reported(enabled: bool, schedule: Schedule) -> Fact {
    Fact::StatusChanged {
        enabled,
        schedule,
        sun_schedule_set_aside: false,
        written_at: None,
        switched_by: SwitchedBy::Schedule,
    }
}

/// Night Shift as CoreBrightness reports it after a write at `instant`, by another app such as
/// Control Center, with sunset to sunrise not set aside.
pub(crate) fn written(enabled: bool, schedule: Schedule, instant: &str) -> Fact {
    Fact::StatusChanged {
        enabled,
        schedule,
        sun_schedule_set_aside: false,
        written_at: Some(at(instant).into()),
        switched_by: SwitchedBy::Outside,
    }
}

/// 22:00 to 07:00, local time.
pub(crate) fn custom_hours() -> Schedule {
    Schedule::Custom {
        from: TimeOfDay {
            hour: 22,
            minute: 0,
        },
        to: TimeOfDay { hour: 7, minute: 0 },
    }
}

pub(crate) fn report(staged: &Staged, enabled: bool) -> Vec<Effect> {
    staged.report(reported(enabled, custom_hours())).effects
}

/// Night Shift on the custom hours as CoreBrightness reports it once Nightcap's own write
/// lands, marked as written now.
pub(crate) fn echo(staged: &Staged, enabled: bool) -> Vec<Effect> {
    let landed = Fact::StatusChanged {
        enabled,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
        written_at: Some(staged.now().into()),
        switched_by: SwitchedBy::Nightcap,
    };
    staged.report(landed).effects
}

/// Night Shift off with no schedule, at a strength of 60, as the shell reports it before
/// `Launched`.
pub(crate) fn report_night_shift(staged: &Staged, schedule: Schedule, strength: u8) {
    staged.report(reported(false, schedule));
    staged.report(Fact::StrengthChanged { value: strength });
}

/// The sun times CoreBrightness reported in Zurich around the night of 15 October 2026.
pub(crate) fn report_zurich_sun(staged: &Staged) {
    staged.report(Fact::SunTimesChanged {
        sunrises: [
            "2026-10-15T05:49:20Z",
            "2026-10-16T05:50:47Z",
            "2026-10-17T05:52:14Z",
        ]
        .map(|sunrise| at(sunrise).into())
        .to_vec(),
        sunsets: [
            "2026-10-15T16:33:25Z",
            "2026-10-16T16:31:32Z",
            "2026-10-17T16:29:41Z",
        ]
        .map(|sunset| at(sunset).into())
        .to_vec(),
    });
}

/// The shell's launch, and its outcome.
pub(crate) fn launch(staged: &Staged) -> Outcome {
    staged.report(Fact::Launched)
}

pub(crate) fn activate(staged: &Staged, bundle_id: &str) -> Vec<Effect> {
    staged.report(activate_event(bundle_id)).effects
}

/// An app that isn't a browser comes to the front.
pub(crate) fn activate_event(bundle_id: &str) -> Fact {
    Fact::AppActivated {
        bundle_id: Some(bundle_id.to_owned()),
        is_browser: false,
    }
}

pub(crate) fn activate_browser(staged: &Staged, bundle_id: &str) -> Vec<Effect> {
    staged.report(activate_browser_event(bundle_id)).effects
}

/// A browser comes to the front.
pub(crate) fn activate_browser_event(bundle_id: &str) -> Fact {
    Fact::AppActivated {
        bundle_id: Some(bundle_id.to_owned()),
        is_browser: true,
    }
}

pub(crate) fn report_host(staged: &Staged, generation: u64, host: &str) -> Vec<Effect> {
    staged.report(host_event(generation, host)).effects
}

/// The watched browser's host, read under `generation`.
pub(crate) fn host_event(generation: u64, host: &str) -> Fact {
    Fact::HostChanged {
        generation,
        host: Some(host.to_owned()),
    }
}

pub(crate) fn accessibility_granted() -> Fact {
    Fact::PermissionsChanged {
        accessibility: true,
        notifications: NotificationPermission::Granted,
    }
}

pub(crate) fn permissions(accessibility: bool, notifications: NotificationPermission) -> Fact {
    Fact::PermissionsChanged {
        accessibility,
        notifications,
    }
}

pub(crate) fn watch(bundle_id: &str, generation: u64) -> Effect {
    Effect::WatchBrowser {
        bundle_id: bundle_id.to_owned(),
        generation,
    }
}

pub(crate) fn figma() -> RuleIdentity {
    RuleIdentity::App("com.figma.Desktop".parse().unwrap())
}

pub(crate) fn mail() -> RuleIdentity {
    RuleIdentity::App("com.apple.mail".parse().unwrap())
}

pub(crate) fn books() -> RuleIdentity {
    RuleIdentity::App("com.apple.iBooksX".parse().unwrap())
}

pub(crate) fn domain(name: &str) -> RuleIdentity {
    RuleIdentity::Domain(name.parse().unwrap())
}

pub(crate) fn figma_app() -> Place {
    Place::App("com.figma.Desktop".parse().unwrap())
}

/// A config rule for an app that says off.
pub(crate) fn app_rule(bundle_id: &str) -> Rule {
    Rule {
        place: Place::App(bundle_id.parse().unwrap()),
        says: OnOff::Off,
        length: Length::Indefinite,
        set_by: SetBy::Config,
    }
}

/// A config rule for a site that says off.
pub(crate) fn domain_rule(domain: &str) -> Rule {
    Rule {
        place: Place::Domain(domain.parse().unwrap()),
        says: OnOff::Off,
        length: Length::Indefinite,
        set_by: SetBy::Config,
    }
}

/// An ad-hoc rule that says off.
pub(crate) fn ad_hoc(identity: RuleIdentity) -> Rule {
    Rule {
        place: Place::from(identity),
        says: OnOff::Off,
        length: Length::Indefinite,
        set_by: SetBy::You,
    }
}

/// A rule of the user's.
pub(crate) fn yours(place: Place, says: OnOff, length: Length) -> Rule {
    Rule {
        place,
        says,
        length,
        set_by: SetBy::You,
    }
}

/// A rule an outside change set, until the schedule turns at `until`, if it has a turn.
pub(crate) fn adopted(place: Place, says: OnOff, until: Option<&str>) -> Rule {
    Rule {
        place,
        says,
        length: Length::UntilScheduleTurns {
            until: until.map(at),
        },
        set_by: SetBy::OutsideChange,
    }
}

/// Until `instant`.
pub(crate) fn until(instant: &str) -> Length {
    Length::Until { until: at(instant) }
}

/// Until the schedule turns, at `instant`.
pub(crate) fn until_turn(instant: &str) -> Length {
    Length::UntilScheduleTurns {
        until: Some(at(instant)),
    }
}

/// Adds an ad-hoc rule that says off.
pub(crate) fn add_rule(identity: RuleIdentity) -> Command {
    Command::AddRule {
        identity,
        says: OnOff::Off,
    }
}

pub(crate) fn set(place: Place, state: OnOff, length: Lasting) -> Command {
    Command::Set {
        place,
        state,
        length,
    }
}

pub(crate) fn minutes(count: i64) -> Lasting {
    Lasting::For(SignedDuration::from_mins(count))
}

/// The check of `kind`, as the checks query answers it.
pub(crate) fn check(staged: &Staged, kind: CheckKind) -> Option<Check> {
    staged
        .core()
        .snapshot()
        .checks()
        .checks
        .into_iter()
        .find(|check| check.detail.kind() == kind)
}
