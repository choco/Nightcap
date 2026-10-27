//! What the integration tests share: the scenarios they start from, the shell's reports, the
//! CLI's queries, and ways to find the menu's rows and items.

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
use nightcap_core::{
    Effect, Fact, File, Fix, FixButton, MenuEntry, MenuItem, Outcome, PlaceRow, RowImage, RowItem,
    Scenario, SettingsCheck, Staged,
};
use nightcap_protocol::{
    CheckKind, Command, ErrorCode, Layer, Length, NotificationPermission, OnOff, Payload, Place,
    PlaceLayers, Query, Request, Response, Rule, RuleIdentity, RuleSource, Schedule, Status,
    TimeOfDay,
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
    staged.report(reported(false, custom_hours()));
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
        .then(reported(false, custom_hours()))
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
    (outcome.effects, response)
}

/// The status, as `nightcap status` asks for it; asking changes nothing.
pub(crate) fn status(staged: &Staged) -> Status {
    let (outcome, response) = staged.request(Request::Query(Query::Status));
    assert_eq!(outcome, Outcome::default());
    let Response::Ok(Payload::Status(status)) = response else {
        panic!("expected a status, got {response:?}");
    };
    *status
}

/// Every rule, as `nightcap rules` lists them.
pub(crate) fn rules(staged: &Staged) -> Vec<Rule> {
    let (_, response) = staged.request(Request::Query(Query::Rules));
    let Response::Ok(Payload::Rules(rules)) = response else {
        panic!("expected rules, got {response:?}");
    };
    rules
}

pub(crate) fn refusal(response: Response) -> ErrorCode {
    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    error.code
}

/// The ad-hoc rules `rules.toml` keeps, as a core started on it alone would load them.
pub(crate) fn kept_ad_hoc(staged: &Staged) -> Vec<Rule> {
    let alone = Scenario {
        rules: staged.file(File::Rules),
        ..noon()
    };
    rules(&alone.stage().unwrap())
}

/// Night Shift as CoreBrightness reports it, with sunset to sunrise not set aside.
pub(crate) fn reported(enabled: bool, schedule: Schedule) -> Fact {
    Fact::StatusChanged {
        enabled,
        schedule,
        sun_schedule_set_aside: false,
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

/// The launch's outcome, leaving out onboarding, which only its own tests look at.
pub(crate) fn launch(staged: &Staged) -> Outcome {
    let mut outcome = staged.report(Fact::Launched);
    outcome
        .effects
        .retain(|effect| !matches!(effect, Effect::ShowOnboarding { .. }));
    outcome
}

pub(crate) fn activate(staged: &Staged, bundle_id: &str) -> Vec<Effect> {
    staged.report(activate_event(bundle_id)).effects
}

/// An app that isn't a browser comes to the front.
pub(crate) fn activate_event(bundle_id: &str) -> Fact {
    Fact::AppActivated {
        bundle_id: Some(bundle_id.to_owned()),
        name: None,
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
        name: None,
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

/// Whether an outcome tells the user anything.
pub(crate) fn tells(outcome: &Outcome) -> bool {
    !outcome.events.is_empty()
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

pub(crate) fn app_rule(bundle_id: &str) -> Rule {
    Rule {
        identity: RuleIdentity::App(bundle_id.parse().unwrap()),
        source: RuleSource::Config,
        says: OnOff::Off,
    }
}

pub(crate) fn domain_rule(domain: &str) -> Rule {
    Rule {
        identity: RuleIdentity::Domain(domain.parse().unwrap()),
        source: RuleSource::Config,
        says: OnOff::Off,
    }
}

pub(crate) fn ad_hoc(identity: RuleIdentity) -> Rule {
    Rule {
        identity,
        source: RuleSource::AdHoc,
        says: OnOff::Off,
    }
}

/// Adds an ad-hoc rule that says off.
pub(crate) fn add_rule(identity: RuleIdentity) -> Command {
    Command::AddRule {
        identity,
        says: OnOff::Off,
    }
}

pub(crate) fn set(place: Place, state: OnOff, length: Length) -> Command {
    Command::Set {
        place,
        state,
        length,
    }
}

pub(crate) fn minutes(count: i64) -> Length {
    Length::For(SignedDuration::from_mins(count))
}

pub(crate) fn layer(state: OnOff, until: Option<&str>) -> Layer {
    Layer {
        state,
        until: until.map(at),
        with: None,
    }
}

/// A place's layers as the status lists them.
pub(crate) fn layered(place: Place, hold: Option<Layer>, timer: Option<Layer>) -> PlaceLayers {
    PlaceLayers { place, hold, timer }
}

/// The menu's place rows, top down.
pub(crate) fn rows(staged: &Staged) -> Vec<PlaceRow> {
    staged
        .core()
        .menu()
        .entries
        .into_iter()
        .filter_map(|entry| match entry {
            MenuEntry::Row { row } => Some(row),
            _ => None,
        })
        .collect()
}

pub(crate) fn everywhere_row(staged: &Staged) -> Option<PlaceRow> {
    rows(staged)
        .into_iter()
        .find(|row| row.image == RowImage::Everywhere)
}

pub(crate) fn app_row(staged: &Staged) -> Option<PlaceRow> {
    rows(staged)
        .into_iter()
        .find(|row| matches!(row.image, RowImage::App { .. }))
}

pub(crate) fn site_row(staged: &Staged) -> Option<PlaceRow> {
    rows(staged)
        .into_iter()
        .find(|row| row.image == RowImage::Site)
}

/// A row's items as they read: their titles, "Custom…" for picking a time, and "—" between
/// groups.
pub(crate) fn titles(row: &PlaceRow) -> Vec<String> {
    row.items
        .iter()
        .map(|item| match item {
            RowItem::Choice { item } => item.title.clone(),
            RowItem::PickTime { .. } => "Custom…".to_owned(),
            RowItem::Separator => "—".to_owned(),
        })
        .collect()
}

/// The item titled `title` in a row, which must have one.
pub(crate) fn choice_titled(row: &PlaceRow, title: &str) -> MenuItem {
    row.items
        .iter()
        .find_map(|item| match item {
            RowItem::Choice { item } if item.title == title => Some(item.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {title:?} in {:?}", titles(row)))
}

/// The check of `kind` as Settings shows it.
pub(crate) fn settings_check(staged: &Staged, kind: CheckKind) -> Option<SettingsCheck> {
    staged
        .core()
        .settings()
        .checks
        .into_iter()
        .find(|check| check.kind == kind)
}

pub(crate) fn button(title: &str, fix: Fix) -> Option<FixButton> {
    Some(FixButton {
        title: title.to_owned(),
        fix,
    })
}
