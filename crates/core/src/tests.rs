use std::sync::Arc;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    Check, CheckKind, CheckState, Command, Decision, ErrorCode, ExportFormat, Install, Instance,
    Layer, Length, NightShift, NotificationPermission, OnOff, Payload, Permissions, Place,
    PlaceLayers, Reason, Request, Response, Rule, RuleIdentity, RuleSource, Schedule, TimeOfDay,
    WebsiteChecking,
};

use crate::config::NightShiftConfig;
use crate::file::{File, Files, Memory, Paths};
use crate::harness::Harness;
use crate::state::{Persisted, Store};
use crate::{
    Core, Effect, Event, Fix, FixButton, LoginItem, MenuEntry, MenuIcon, MenuItem, MenuPath,
    NightShiftItem, OnboardingNightShift, OnboardingSteps, PlaceRow, ProblemLine, RowImage,
    RowItem, RuleAction, Scenario, SettingsCheck, SettingsLayers, SettingsRule, SettingsSwitch,
    TourApp, TourFacts, TourPlace, TourSite, TourTitles, adhoc, normalize_domain, tour_titles,
};

fn at(instant: &str) -> Timestamp {
    instant.parse().unwrap()
}

/// Noon in Zurich, with no files and nothing reported yet.
fn noon() -> Scenario {
    Scenario::at("2026-10-16T12:00:00+02:00")
}

/// 23:00 in Zurich, inside the custom hours.
fn late() -> Scenario {
    Scenario::at("2026-10-16T23:00:00+02:00")
}

/// Night Shift as CoreBrightness reports it, with sunset to sunrise not set aside.
fn reported(enabled: bool, schedule: Schedule) -> Event {
    Event::StatusChanged {
        enabled,
        schedule,
        sun_schedule_set_aside: false,
    }
}

/// 22:00 to 07:00, local time.
fn custom_hours() -> Schedule {
    Schedule::Custom {
        from: TimeOfDay {
            hour: 22,
            minute: 0,
        },
        to: TimeOfDay { hour: 7, minute: 0 },
    }
}

/// Files in memory, for the stores alone.
fn in_memory() -> Arc<Memory> {
    Arc::new(Memory::new(Paths::xdg(Instance::Release).unwrap()))
}

/// Files in memory with `text` in `file`.
fn holding(file: File, text: &str) -> Arc<Memory> {
    let files = in_memory();
    files.put(file, Some(text.as_bytes().to_owned()));
    files
}

/// The ad-hoc rules `rules.toml` keeps, as the next start would load them.
fn kept_ad_hoc(core: &Harness) -> Vec<(RuleIdentity, OnOff)> {
    let files = in_memory();
    files.put(File::Rules, core.file(File::Rules).map(String::into_bytes));
    adhoc::Store::new(files).load().unwrap()
}

fn app_rule(bundle_id: &str) -> Rule {
    Rule {
        identity: RuleIdentity::App(bundle_id.parse().unwrap()),
        source: RuleSource::Config,
        says: OnOff::Off,
    }
}

/// Starts at 23:00 with Night Shift on by the custom hours, then brings Figma, which a config
/// rule forces off, to the front, and reports the result.
fn forced() -> Harness {
    let core = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .then(reported(true, custom_hours()))
        .start();
    assert_eq!(
        activate(&core, "com.figma.Desktop"),
        vec![Effect::SetNightShift { enabled: false }]
    );
    core.handle(reported(false, custom_hours()));
    core
}

/// The launch's effects, leaving out onboarding, which only its own tests look at.
fn launch(core: &Core) -> Vec<Effect> {
    core.handle(Event::Launched)
        .into_iter()
        .filter(|effect| !matches!(effect, Effect::ShowOnboarding { .. }))
        .collect()
}

fn ad_hoc(identity: RuleIdentity) -> Rule {
    Rule {
        identity,
        source: RuleSource::AdHoc,
        says: OnOff::Off,
    }
}

/// Adds an ad-hoc rule that says off.
fn add_rule(identity: RuleIdentity) -> Command {
    Command::AddRule {
        identity,
        says: OnOff::Off,
    }
}

fn figma() -> RuleIdentity {
    RuleIdentity::App("com.figma.Desktop".parse().unwrap())
}

fn activate(core: &Core, bundle_id: &str) -> Vec<Effect> {
    core.handle(activate_event(bundle_id))
}

/// An app that isn't a browser comes to the front.
fn activate_event(bundle_id: &str) -> Event {
    Event::AppActivated {
        bundle_id: Some(bundle_id.to_owned()),
        name: None,
        is_browser: false,
    }
}

fn activate_browser(core: &Core, bundle_id: &str) -> Vec<Effect> {
    core.handle(activate_browser_event(bundle_id))
}

/// A browser comes to the front.
fn activate_browser_event(bundle_id: &str) -> Event {
    Event::AppActivated {
        bundle_id: Some(bundle_id.to_owned()),
        name: None,
        is_browser: true,
    }
}

fn report_host(core: &Core, generation: u64, host: &str) -> Vec<Effect> {
    core.handle(host_event(generation, host))
}

/// The watched browser's host, read under `generation`.
fn host_event(generation: u64, host: &str) -> Event {
    Event::HostChanged {
        generation,
        host: Some(host.to_owned()),
    }
}

fn watch(bundle_id: &str, generation: u64) -> Effect {
    Effect::WatchBrowser {
        bundle_id: bundle_id.to_owned(),
        generation,
    }
}

/// Website checking is useless without Accessibility, so tests about it start granted.
fn grant_accessibility(core: &Core) {
    core.handle(accessibility_granted());
}

fn accessibility_granted() -> Event {
    Event::PermissionsChanged {
        accessibility: true,
        notifications: NotificationPermission::Granted,
    }
}

fn domain_rule(domain: &str) -> Rule {
    Rule {
        identity: RuleIdentity::Domain(domain.parse().unwrap()),
        source: RuleSource::Config,
        says: OnOff::Off,
    }
}

/// At 23:00 with Night Shift on by the custom hours, website checking on and a config rule for
/// figma.com.
fn with_figma_domain() -> Scenario {
    late()
        .config("websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n")
        .then(accessibility_granted())
        .then(reported(true, custom_hours()))
}

#[test]
fn the_status_reports_night_shift_as_last_observed() {
    let core = noon().start();

    core.handle(reported(true, Schedule::Off));

    assert_eq!(core.status().night_shift, NightShift::On);
}

#[test]
fn the_status_reports_the_strength_as_last_read() {
    let core = noon().start();
    assert_eq!(core.status().strength, None);

    core.handle(Event::StrengthChanged { value: 60 });

    assert_eq!(core.status().strength, Some(60));
}

/// Night Shift reported off with no schedule, at a strength of 60.
fn at_strength_60() -> Harness {
    noon()
        .then(reported(false, Schedule::Off))
        .then(Event::StrengthChanged { value: 60 })
        .start()
}

#[test]
fn strength_sets_it_and_answers_with_the_decision() {
    let core = at_strength_60();
    let decision = core.status().decision;

    let (effects, response) = core.send(Command::Strength(40));

    assert_eq!(
        effects,
        vec![Effect::SetStrength {
            value: 40,
            commit: true
        }]
    );
    assert_eq!(response, Response::Ok(Payload::Decision(decision)));
}

#[test]
fn previewing_a_strength_writes_it_without_saving_it() {
    let core = at_strength_60();

    let (effects, _) = core.send(Command::PreviewStrength(80));

    assert_eq!(
        effects,
        vec![Effect::SetStrength {
            value: 80,
            commit: false
        }]
    );
}

#[test]
fn strength_while_night_shift_is_unavailable_is_refused() {
    let core = noon().start();

    let (effects, response) = core.send(Command::Strength(40));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::NightShiftUnavailable);
}

#[test]
fn strength_before_the_shell_could_read_it_is_refused() {
    let core = noon().then(reported(false, Schedule::Off)).start();

    let (effects, response) = core.send(Command::PreviewStrength(40));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::StrengthUnavailable);
}

#[test]
fn a_strength_above_100_is_refused() {
    let core = at_strength_60();

    let (effects, response) = core.send(Command::Strength(101));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::BadRequest);
}

#[test]
fn set_schedule_writes_it_and_answers_with_the_decision() {
    let core = at_strength_60();
    let decision = core.status().decision;

    let (effects, response) = core.send(Command::SetSchedule(custom_hours()));

    assert_eq!(
        effects,
        vec![Effect::SetSchedule {
            schedule: custom_hours()
        }]
    );
    assert_eq!(response, Response::Ok(Payload::Decision(decision)));
}

#[test]
fn switching_the_schedule_off_while_night_shift_is_on_keeps_it_on() {
    let core = late().then(reported(true, custom_hours())).start();

    core.send(Command::SetSchedule(Schedule::Off));
    // macOS switches Night Shift off along with the schedule.
    let effects = core.handle(reported(false, Schedule::Off));

    assert!(effects.contains(&Effect::SetNightShift { enabled: true }));
    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, None)),
            None
        )]
    );
}

#[test]
fn set_schedule_while_night_shift_is_unavailable_is_refused() {
    let core = noon().start();

    let (effects, response) = core.send(Command::SetSchedule(Schedule::Off));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::NightShiftUnavailable);
}

#[test]
fn custom_hours_that_arent_times_of_day_are_refused() {
    let core = at_strength_60();
    let hours = Schedule::Custom {
        from: TimeOfDay {
            hour: 24,
            minute: 0,
        },
        to: TimeOfDay { hour: 7, minute: 0 },
    };

    let (effects, response) = core.send(Command::SetSchedule(hours));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::BadRequest);
}

#[test]
fn custom_hours_the_shell_reports_out_of_range_leave_the_schedule_unknown() {
    let core = late().then(reported(true, custom_hours())).start();
    let hours = Schedule::Custom {
        from: TimeOfDay {
            hour: 25,
            minute: 0,
        },
        to: TimeOfDay {
            hour: 7,
            minute: 60,
        },
    };

    core.handle(reported(true, hours));
    core.set_clock("2026-10-17T07:30:00+02:00");
    core.handle(Event::TimerFired);

    let status = core.status();
    assert_eq!(status.night_shift, NightShift::On);
    assert_eq!(status.schedule, None);
    assert_eq!(status.next_transition, None);
    assert_eq!(core.menu().night_shift.reason, None);
}

/// Night Shift off with no schedule, at a strength of 60, as the shell reports it before
/// `Launched`.
fn report_night_shift(core: &Core, schedule: Schedule, strength: u8) {
    core.handle(reported(false, schedule));
    core.handle(Event::StrengthChanged { value: strength });
}

#[test]
fn the_configs_night_shift_values_are_applied_at_launch() {
    let core = noon()
        .config("[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n")
        .start();
    report_night_shift(&core, Schedule::Off, 60);

    assert_eq!(
        launch(&core),
        vec![
            Effect::SetStrength {
                value: 40,
                commit: true
            },
            Effect::SetSchedule {
                schedule: Schedule::SunsetToSunrise
            },
        ]
    );
}

#[test]
fn the_configs_night_shift_values_apply_once_and_a_change_made_since_stays() {
    let core = noon()
        .config("[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n")
        .start();
    report_night_shift(&core, Schedule::Off, 60);
    launch(&core);

    // The user moved the strength in System Settings, then Nightcap started again.
    let core = core.restart_at("2026-10-16T12:05:00+02:00");
    report_night_shift(&core, Schedule::SunsetToSunrise, 70);

    assert_eq!(launch(&core), Vec::new());
}

#[test]
fn the_status_shows_the_configs_night_shift_values_beside_the_macs() {
    let core = noon()
        .config("[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n")
        .start();
    report_night_shift(&core, Schedule::Off, 60);

    let status = core.status();

    assert_eq!(
        (status.strength, status.config_strength),
        (Some(60), Some(40))
    );
    assert_eq!(
        (status.schedule, status.config_schedule),
        (Some(Schedule::Off), Some(Schedule::SunsetToSunrise))
    );
}

#[test]
fn reloading_applies_only_the_night_shift_value_that_changed() {
    let core = noon()
        .config("[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n")
        .start();
    report_night_shift(&core, Schedule::Off, 60);
    launch(&core);

    core.write_config("[night_shift]\nstrength = 50\nschedule = \"sunset-to-sunrise\"\n");
    let (effects, _) = core.request(Request::Reload);

    assert_eq!(
        effects,
        vec![Effect::SetStrength {
            value: 50,
            commit: true
        }]
    );
}

#[test]
fn a_night_shift_value_taken_out_of_the_config_stays_and_applies_again_when_put_back() {
    let core = noon().config("[night_shift]\nstrength = 40\n").start();
    report_night_shift(&core, Schedule::Off, 60);
    launch(&core);

    core.write_config("");
    let (taken_out, _) = core.request(Request::Reload);
    core.write_config("[night_shift]\nstrength = 40\n");
    let (put_back, _) = core.request(Request::Reload);

    assert_eq!(taken_out, Vec::new());
    assert_eq!(
        put_back,
        vec![Effect::SetStrength {
            value: 40,
            commit: true
        }]
    );
}

#[test]
fn a_night_shift_value_the_config_cant_have_makes_it_invalid() {
    let core = noon().start();
    report_night_shift(&core, Schedule::Off, 60);

    for night_shift in [
        "strength = 101",
        "schedule = \"dusk\"",
        "schedule = \"22:00-7am\"",
        "schedule = \"24:00-07:00\"",
    ] {
        core.write_config(&format!("[night_shift]\n{night_shift}\n"));
        let (effects, response) = core.request(Request::Reload);
        assert_eq!(effects, Vec::new(), "{night_shift}");
        assert_eq!(refusal(response), ErrorCode::InvalidConfig, "{night_shift}");
    }
}

/// The answer to a command that leaves Night Shift on at 23:00 by the custom hours.
fn following_the_custom_hours_until_morning() -> Response {
    Response::Ok(Payload::Decision(Decision {
        state: Some(OnOff::On),
        place: Place::Everywhere,
        reasons: vec![Reason::Schedule {
            state: OnOff::On,
            until: Some(at("2026-10-17T07:00:00+02:00")),
        }],
    }))
}

#[test]
fn a_command_while_night_shift_is_unavailable_is_refused() {
    let core = noon().start();

    let (effects, response) = core.send(Command::On);

    assert_eq!(effects, Vec::new());
    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    assert_eq!(error.code, ErrorCode::NightShiftUnavailable);
}

/// Everywhere held `state` until the user turns it back, over no schedule.
fn held(state: OnOff) -> Response {
    Response::Ok(Payload::Decision(Decision {
        state: Some(state),
        place: Place::Everywhere,
        reasons: vec![
            Reason::Hold(layer(state, None)),
            Reason::Schedule {
                state: OnOff::Off,
                until: None,
            },
        ],
    }))
}

/// Everywhere off by having no schedule, with nothing over it.
fn off_with_no_schedule() -> Response {
    Response::Ok(Payload::Decision(Decision {
        state: Some(OnOff::Off),
        place: Place::Everywhere,
        reasons: vec![Reason::Schedule {
            state: OnOff::Off,
            until: None,
        }],
    }))
}

fn layer(state: OnOff, until: Option<&str>) -> Layer {
    Layer {
        state,
        until: until.map(at),
        with: None,
    }
}

/// A place's layers as the status lists them.
fn layered(place: Place, hold: Option<Layer>, timer: Option<Layer>) -> PlaceLayers {
    PlaceLayers { place, hold, timer }
}

fn set(place: Place, state: OnOff, length: Length) -> Command {
    Command::Set {
        place,
        state,
        length,
    }
}

fn minutes(count: i64) -> Length {
    Length::For(SignedDuration::from_mins(count))
}

#[test]
fn on_turns_night_shift_on_everywhere_until_turned_back() {
    let core = noon().start();
    core.handle(reported(false, Schedule::Off));

    let (effects, response) = core.send(Command::On);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, held(OnOff::On));
}

#[test]
fn off_takes_off_everywheres_hold_where_that_is_enough() {
    let core = noon().start();
    // Found on with no schedule, which the core takes as the user's own on.
    core.handle(reported(true, Schedule::Off));

    let (effects, response) = core.send(Command::Off);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(response, off_with_no_schedule());
}

#[test]
fn on_while_everywhere_says_on_writes_nothing() {
    let core = noon().start();
    core.handle(reported(true, Schedule::Off));

    let (effects, response) = core.send(Command::On);

    assert_eq!(effects, Vec::new());
    assert_eq!(response, held(OnOff::On));
}

#[test]
fn toggle_turns_everywhere_the_other_way() {
    let core = noon().start();
    core.handle(reported(true, Schedule::Off));

    let (effects, response) = core.send(Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(response, off_with_no_schedule());
}

#[test]
fn two_flips_leave_nothing_behind() {
    let core = on_by_the_schedule().start();
    core.send(Command::Toggle);
    core.handle(reported(false, custom_hours()));

    let (effects, response) = core.send(Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn on_while_the_schedule_says_on_holds_nothing() {
    let core = on_by_the_schedule().start();

    let (effects, response) = core.send(Command::On);

    assert_eq!(effects, Vec::new());
    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn a_hold_with_no_end_outlasts_sunrise_and_sunset() {
    let core = off_by_the_schedule().start();
    core.send(Command::On);
    core.handle(reported(true, custom_hours()));

    core.set_clock("2026-10-17T07:00:00+02:00");
    core.handle(Event::TimerFired);
    // macOS performs its own transition, and Nightcap puts Night Shift back on.
    let effects = core.handle(reported(false, custom_hours()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, None)),
            None
        )]
    );
}

#[test]
fn an_external_change_holds_everywhere_until_the_schedule_turns_it_back() {
    let core = off_by_the_schedule().start();

    let effects = core.handle(reported(true, custom_hours()));

    assert_eq!(effects, Vec::new());
    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn an_external_change_back_takes_off_what_the_switch_held() {
    let core = off_by_the_schedule().start();
    core.send(Command::On);
    core.handle(reported(true, custom_hours()));

    let effects = core.handle(reported(false, custom_hours()));

    assert_eq!(effects, Vec::new());
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn an_edited_schedule_ends_the_holds_waiting_on_it() {
    let core = on_by_the_schedule().start();
    core.send(set(
        Place::Everywhere,
        OnOff::Off,
        Length::UntilScheduleTurns,
    ));
    core.handle(reported(false, custom_hours()));

    // Moving the start to 21:00 makes macOS apply the schedule again, so Night Shift comes on.
    let effects = core.handle(reported(
        true,
        Schedule::Custom {
            from: TimeOfDay {
                hour: 21,
                minute: 0,
            },
            to: TimeOfDay { hour: 7, minute: 0 },
        },
    ));

    assert_eq!(effects, Vec::new());
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn an_edited_schedule_keeps_timers_and_holds_with_no_end() {
    let core = on_by_the_schedule().start();
    core.send(Command::Off);
    core.send(set(Place::Everywhere, OnOff::On, minutes(30)));

    core.handle(reported(true, Schedule::SunsetToSunrise));

    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            Some(layer(OnOff::On, Some("2026-10-16T23:30:00+02:00")))
        )]
    );
}

#[test]
fn a_hold_until_the_schedule_turns_ends_at_its_turn_even_asleep() {
    let core = off_by_the_schedule().start();
    core.send(set(
        Place::Everywhere,
        OnOff::On,
        Length::UntilScheduleTurns,
    ));

    core.set_clock("2026-10-17T08:00:00+02:00");
    let effects = core.handle(Event::Woke);

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T22:00:00+02:00").into()
        }]
    );
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn a_timer_gives_back_what_was_under_it() {
    let core = on_by_the_schedule().start();

    let (effects, _) = core.send(set(Place::Everywhere, OnOff::Off, minutes(30)));
    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-16T23:30:00+02:00").into()
            },
        ]
    );
    core.handle(reported(false, custom_hours()));

    core.set_clock("2026-10-16T23:30:00+02:00");
    assert_eq!(
        core.handle(Event::TimerFired),
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn a_new_timer_replaces_the_places_timer_and_keeps_its_hold() {
    let core = on_by_the_schedule().start();
    core.send(Command::Off);
    core.send(set(Place::Everywhere, OnOff::On, minutes(30)));

    core.send(set(Place::Everywhere, OnOff::On, minutes(120)));

    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            Some(layer(OnOff::On, Some("2026-10-17T01:00:00+02:00")))
        )]
    );
}

#[test]
fn until_the_schedule_turns_ends_at_sunrise_for_on_and_at_sunset_for_off() {
    let core = off_by_the_schedule().start();

    core.send(set(
        Place::Everywhere,
        OnOff::On,
        Length::UntilScheduleTurns,
    ));
    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );

    core.set_clock("2026-10-16T23:00:00+02:00");
    core.send(set(
        Place::Everywhere,
        OnOff::Off,
        Length::UntilScheduleTurns,
    ));
    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, Some("2026-10-17T22:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn a_new_hold_replaces_the_places_hold_and_ends_its_timer() {
    let core = on_by_the_schedule().start();
    core.send(Command::Off);
    core.send(set(Place::Everywhere, OnOff::On, minutes(30)));

    core.send(set(
        Place::Everywhere,
        OnOff::Off,
        Length::UntilScheduleTurns,
    ));

    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, Some("2026-10-17T22:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn until_a_picked_time_is_a_timer_that_ends_then() {
    let core = on_by_the_schedule().start();

    let (effects, _) = core.send(set(
        Place::Everywhere,
        OnOff::Off,
        Length::Until(at("2026-10-17T00:15:00+02:00")),
    ));

    assert!(effects.contains(&Effect::ScheduleWakeup {
        at: at("2026-10-17T00:15:00+02:00").into()
    }));
    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            None,
            Some(layer(OnOff::Off, Some("2026-10-17T00:15:00+02:00")))
        )]
    );
}

#[test]
fn lengths_that_cant_be_are_refused() {
    let core = late().start();
    core.handle(reported(true, Schedule::Off));

    for command in [
        set(Place::Everywhere, OnOff::Off, minutes(0)),
        set(
            Place::Everywhere,
            OnOff::Off,
            Length::Until(at("2026-10-16T22:00:00+02:00")),
        ),
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
        set(
            Place::App("com.figma.Desktop".parse().unwrap()),
            OnOff::Off,
            Length::UntilTurnedBack,
        ),
    ] {
        let (effects, response) = core.send(command.clone());
        assert_eq!(effects, Vec::new(), "{command:?}");
        assert_eq!(refusal(response), ErrorCode::BadRequest, "{command:?}");
    }
}

#[test]
fn the_status_reports_the_schedule_its_scheduled_state_and_the_next_transition() {
    let core = off_by_the_schedule().start();

    let status = core.status();
    assert_eq!(status.schedule, Some(custom_hours()));
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(
        status.next_transition,
        Some(at("2026-10-16T22:00:00+02:00"))
    );
}

#[test]
fn the_core_asks_to_be_woken_at_the_next_schedule_transition() {
    let core = Scenario::at("2026-10-16T20:00:00+02:00").start();

    let effects = core.handle(reported(false, custom_hours()));

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+02:00").into()
        }]
    );
}

#[test]
fn a_timer_that_fires_before_its_transition_is_set_again() {
    let core = off_by_the_schedule().start();

    core.set_clock("2026-10-16T21:59:59+02:00");

    assert_eq!(
        core.handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+02:00").into()
        }]
    );
}

#[test]
fn a_change_just_before_a_schedule_transition_is_that_transition() {
    let core = off_by_the_schedule().start();
    core.send(set(
        Place::Everywhere,
        OnOff::Off,
        Length::UntilScheduleTurns,
    ));

    core.set_clock("2026-10-16T21:59:00+02:00");
    let effects = core.handle(reported(true, custom_hours()));

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
    let status = core.status();
    assert_eq!(status.layers, Vec::new());
    assert_eq!(status.scheduled_state, Some(OnOff::On));
}

#[test]
fn a_change_against_the_coming_transition_is_still_an_external_change() {
    let core = off_by_the_schedule().start();
    core.send(Command::On);
    core.handle(reported(true, custom_hours()));

    core.set_clock("2026-10-16T21:59:00+02:00");
    core.handle(reported(false, custom_hours()));

    let status = core.status();
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(
        status.next_transition,
        Some(at("2026-10-16T22:00:00+02:00"))
    );
}

#[test]
fn a_time_zone_change_moves_the_custom_hours() {
    let core = off_by_the_schedule().start();

    let effects = core.handle(Event::TimeZoneChanged {
        iana_id: "Europe/London".to_owned(),
    });

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+01:00").into()
        }]
    );
}

/// The sun times CoreBrightness reported in Zurich around the night of 15 October 2026.
fn report_zurich_sun(core: &Core) {
    core.handle(Event::SunTimesChanged {
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

#[test]
fn sunset_to_sunrise_follows_the_reported_sun_times() {
    let core = Scenario::at("2026-10-16T01:40:00+02:00").start();

    report_zurich_sun(&core);
    let effects = core.handle(reported(true, Schedule::SunsetToSunrise));

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T05:50:47Z").into()
        }]
    );
    let status = core.status();
    assert_eq!(status.scheduled_state, Some(OnOff::On));
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn sun_times_the_shell_reports_out_of_range_count_as_none() {
    let core = Scenario::at("2026-10-16T01:40:00+02:00").start();
    report_zurich_sun(&core);
    core.handle(reported(true, Schedule::SunsetToSunrise));

    let past_jiff = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(400_000_000_000);
    core.handle(Event::SunTimesChanged {
        sunrises: vec![past_jiff],
        sunsets: Vec::new(),
    });

    let status = core.status();
    assert_eq!(status.scheduled_state, None);
    assert_eq!(status.next_transition, None);
}

#[test]
fn until_corebrightness_has_the_sun_times_the_shell_reads_them_again_each_minute() {
    let core = Scenario::at("2026-10-16T01:40:20+02:00").start();

    let effects = core.handle(reported(true, Schedule::SunsetToSunrise));
    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T01:41:00+02:00").into()
        }]
    );
    core.set_clock("2026-10-16T01:40:50+02:00");
    assert_eq!(core.handle(Event::Woke), Vec::new());

    core.set_clock("2026-10-16T01:41:00+02:00");
    assert_eq!(
        core.handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T01:42:00+02:00").into()
        }]
    );

    core.set_clock("2026-10-16T01:42:00+02:00");
    report_zurich_sun(&core);
    assert_eq!(
        core.handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T05:50:47Z").into()
        }]
    );
    assert_eq!(core.status().scheduled_state, Some(OnOff::On));
}

#[test]
fn turning_the_schedule_off_cancels_the_wakeup() {
    let core = off_by_the_schedule().start();

    let effects = core.handle(reported(false, Schedule::Off));

    assert_eq!(effects, vec![Effect::CancelWakeup]);
    let status = core.status();
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(status.next_transition, None);
}

#[test]
fn a_config_rule_turns_night_shift_off_while_its_app_is_frontmost() {
    let core = on_by_the_schedule()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();

    let effects = activate(&core, "com.figma.Desktop");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    let status = core.status();
    assert_eq!(status.frontmost_app.as_deref(), Some("com.figma.Desktop"));
    assert_eq!(
        status.decision,
        Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }
    );
}

#[test]
fn leaving_the_app_hands_night_shift_back_to_the_schedule() {
    let core = on_by_the_schedule()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    activate(&core, "com.figma.Desktop");
    core.handle(reported(false, custom_hours()));

    let effects = activate(&core, "com.apple.Terminal");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
}

#[test]
fn an_invalid_config_at_launch_runs_without_config_rules_and_says_so() {
    let core = noon()
        .config("[rules.off]\napps = [\"com.figma.Desktop\"]\n")
        .start();

    let effects = launch(&core);

    let [Effect::Notify { body, .. }] = effects.as_slice() else {
        panic!("expected one notification, got {effects:?}");
    };
    assert!(body.contains("apps"), "{body}");
    let status = core.status();
    assert_eq!(status.rules, Vec::new());
    assert!(
        status
            .config_error
            .is_some_and(|error| error.contains("apps"))
    );
}

#[test]
fn reload_applies_a_changed_config() {
    let core = on_by_the_schedule().start();
    activate(&core, "com.figma.Desktop");

    core.write_config("[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (effects, response) = core.request(Request::Reload);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    let Response::Ok(Payload::Status(status)) = response else {
        panic!("expected a status, got {response:?}");
    };
    assert_eq!(status.rules, vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn reloading_an_invalid_config_is_refused_and_keeps_the_rules_in_force() {
    let core = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();

    core.write_config("[rules.off]\napp = \"com.figma.Desktop\"\n");
    let (effects, response) = core.request(Request::Reload);

    assert_eq!(effects, Vec::new());
    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert_eq!(
        error.message,
        format!(
            "{}: line 2: invalid type: string \"com.figma.Desktop\", expected a sequence",
            core.settings().config_path
        )
    );
    let status = core.status();
    assert_eq!(status.rules, vec![app_rule("com.figma.Desktop")]);
    assert_eq!(status.config_error, Some(error.message));
}

#[test]
fn a_command_answers_with_the_resulting_decision() {
    let core = on_by_the_schedule().start();

    let (_, response) = core.send(Command::Off);

    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: Place::Everywhere,
            reasons: vec![
                Reason::Hold(layer(OnOff::Off, None)),
                Reason::Schedule {
                    state: OnOff::On,
                    until: Some(at("2026-10-17T07:00:00+02:00")),
                },
            ],
        }))
    );
}

#[test]
fn macos_performing_a_transition_just_after_the_core_applied_it_is_not_an_external_change() {
    let core = Scenario::at("2026-10-16T21:00:00+02:00")
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    core.handle(reported(false, custom_hours()));
    activate(&core, "com.figma.Desktop");
    core.set_clock("2026-10-16T22:00:00+02:00");
    core.handle(Event::TimerFired);

    core.set_clock("2026-10-16T22:01:00+02:00");
    let effects = core.handle(reported(true, custom_hours()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn after_waking_macos_catching_up_on_a_missed_transition_is_not_an_external_change() {
    let core = Scenario::at("2026-10-16T21:00:00+02:00")
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    core.handle(reported(false, custom_hours()));
    activate(&core, "com.figma.Desktop");
    core.set_clock("2026-10-16T23:00:00+02:00");
    core.handle(Event::Woke);

    core.set_clock("2026-10-16T23:03:00+02:00");
    let effects = core.handle(reported(true, custom_hours()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(core.status().layers, Vec::new());
}

fn figma_app() -> Place {
    Place::App("com.figma.Desktop".parse().unwrap())
}

#[test]
fn in_front_of_a_rule_the_switch_changes_everywhere_and_the_rule_keeps_its_say() {
    let core = forced();

    let (effects, response) = core.send(Command::Off);

    assert_eq!(effects, Vec::new());
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }))
    );
    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            None
        )]
    );
    assert_eq!(activate(&core, "com.apple.Terminal"), Vec::new());
}

#[test]
fn setting_a_rules_place_the_other_way_snoozes_the_rule_for_that_length() {
    let core = forced();

    let (effects, response) = core.send(set(figma_app(), OnOff::On, Length::UntilScheduleTurns));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::On),
            place: figma_app(),
            reasons: vec![
                Reason::Hold(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
                Reason::Rule(app_rule("com.figma.Desktop")),
            ],
        }))
    );
}

#[test]
fn a_snoozed_rule_comes_back_when_its_snooze_ends() {
    let core = forced();
    core.send(set(figma_app(), OnOff::On, minutes(30)));
    core.handle(reported(true, custom_hours()));

    core.set_clock("2026-10-16T23:30:00+02:00");
    let effects = core.handle(Event::TimerFired);

    assert!(effects.contains(&Effect::SetNightShift { enabled: false }));
    let status = core.status();
    assert_eq!(status.layers, Vec::new());
    assert_eq!(status.decision.place, figma_app());
}

#[test]
fn a_snooze_stays_with_its_rule_when_another_app_comes_to_the_front() {
    let core = forced();
    core.send(set(figma_app(), OnOff::On, Length::UntilScheduleTurns));
    core.handle(reported(true, custom_hours()));

    assert_eq!(activate(&core, "com.apple.Terminal"), Vec::new());
    assert_eq!(activate(&core, "com.figma.Desktop"), Vec::new());
    assert_eq!(core.status().decision.state, Some(OnOff::On));
}

#[test]
fn an_external_on_in_front_of_a_rule_snoozes_it_until_the_schedule_turns_and_says_so() {
    let core = forced();

    let effects = core.handle(reported(true, custom_hours()));

    assert_eq!(
        effects,
        vec![Effect::Notify {
            title: "Night Shift turned on outside Nightcap".to_owned(),
            body: "Snoozed the com.figma.Desktop rule until 07:00.".to_owned(),
            opens: None,
        }]
    );
    assert_eq!(
        core.status().layers,
        vec![layered(
            figma_app(),
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn with_no_schedule_the_night_shift_item_says_so() {
    let core = noon().start();
    core.handle(reported(false, Schedule::Off));

    assert_eq!(
        core.menu().night_shift.reason.as_deref(),
        Some("No schedule")
    );
}

#[test]
fn with_no_schedule_an_external_change_holds_until_turned_back() {
    let core = noon().start();
    core.handle(reported(false, Schedule::Off));

    core.handle(reported(true, Schedule::Off));

    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, None)),
            None
        )]
    );
}

#[test]
fn flipping_back_from_outside_takes_off_both_linked_holds() {
    let core = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    core.handle(reported(false, custom_hours()));
    activate(&core, "com.figma.Desktop");

    let effects = core.handle(reported(true, custom_hours()));

    assert_eq!(
        effects,
        vec![Effect::Notify {
            title: "Night Shift turned on outside Nightcap".to_owned(),
            body: "Snoozed the com.figma.Desktop rule until 07:00.".to_owned(),
            opens: None,
        }]
    );
    let morning = Some(at("2026-10-17T07:00:00+02:00"));
    let linked = |with| {
        Some(Layer {
            state: OnOff::On,
            until: morning,
            with: Some(with),
        })
    };
    assert_eq!(
        core.status().layers,
        vec![
            layered(figma_app(), linked(Place::Everywhere), None),
            layered(Place::Everywhere, linked(figma_app()), None),
        ]
    );

    core.handle(reported(false, custom_hours()));

    assert_eq!(core.status().layers, Vec::new());
    assert_eq!(activate(&core, "com.apple.mail"), Vec::new());
}

#[test]
fn a_hold_set_over_a_linked_one_leaves_its_partner_unlinked() {
    let core = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    report(&core, false);
    activate(&core, "com.figma.Desktop");
    report(&core, true);

    core.send(set(figma_app(), OnOff::Off, Length::UntilScheduleTurns));

    assert_eq!(
        core.status().layers,
        vec![
            layered(
                figma_app(),
                Some(layer(OnOff::Off, Some("2026-10-16T22:00:00+02:00"))),
                None
            ),
            layered(
                Place::Everywhere,
                Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
                None
            ),
        ]
    );
}

#[test]
fn the_switch_first_takes_off_everywheres_layers_that_say_otherwise() {
    let core = on_by_the_schedule().start();
    core.send(set(Place::Everywhere, OnOff::Off, minutes(60)));

    let (_, response) = core.send(Command::On);

    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn turning_night_shift_off_everywhere_leaves_a_snoozed_rule_warm() {
    let core = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    report(&core, true);
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });
    let figma = Place::App("com.figma.Desktop".parse().unwrap());
    core.send(set(figma.clone(), OnOff::On, minutes(30)));
    report(&core, true);

    let (effects, response) = core.send(Command::Off);

    assert_eq!(effects, Vec::new());
    let Response::Ok(Payload::Decision(decision)) = response else {
        panic!("{response:?}");
    };
    assert_eq!((decision.state, decision.place), (Some(OnOff::On), figma));
}

#[test]
fn reset_puts_a_snoozed_rule_back_early() {
    let core = forced();
    core.send(set(figma_app(), OnOff::On, Length::UntilScheduleTurns));

    let (_, response) = core.send(Command::Reset(figma_app()));

    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }))
    );
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn adding_the_rule_a_snoozed_app_already_has_ends_the_snooze() {
    let core = forced();
    core.send(set(figma_app(), OnOff::On, minutes(30)));
    core.handle(reported(true, custom_hours()));

    let (effects, response) = core.send(add_rule(figma()));

    assert!(effects.contains(&Effect::SetNightShift { enabled: false }));
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }))
    );
    assert_eq!(core.status().layers, Vec::new());
}

/// Snoozes the Figma rule until 07:00, then turns Night Shift off everywhere for an hour.
fn snooze_figma_and_break(core: &Harness) {
    core.send(set(figma_app(), OnOff::On, Length::UntilScheduleTurns));
    core.send(set(Place::Everywhere, OnOff::Off, minutes(60)));
}

#[test]
fn holds_and_timers_survive_a_restart() {
    let core = forced();
    snooze_figma_and_break(&core);

    let core = core.restart_at("2026-10-16T23:30:00+02:00");

    assert_eq!(
        launch(&core),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T00:00:00+02:00").into()
        }]
    );
    assert_eq!(
        core.status().layers,
        vec![
            layered(
                figma_app(),
                Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
                None
            ),
            layered(
                Place::Everywhere,
                None,
                Some(layer(OnOff::Off, Some("2026-10-17T00:00:00+02:00")))
            ),
        ]
    );
}

#[test]
fn holds_and_timers_that_ended_while_nightcap_was_stopped_are_dropped_on_start() {
    let core = forced();
    snooze_figma_and_break(&core);

    // Past both the timer's end and the 07:00 schedule transition.
    let core = core.restart_at("2026-10-17T08:00:00+02:00");

    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn on_the_way_out_only_everywheres_hold_still_applies() {
    let core = on_by_the_schedule().start();
    core.send(Command::Off);
    core.handle(reported(false, custom_hours()));
    core.send(set(Place::Everywhere, OnOff::On, minutes(30)));
    core.handle(reported(true, custom_hours()));

    assert_eq!(
        core.handle(Event::Terminating),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn terminating_releases_every_force() {
    let core = forced();

    assert_eq!(
        core.handle(Event::Terminating),
        vec![Effect::SetNightShift { enabled: true }]
    );
}

#[test]
fn terminating_watches_no_browser_its_own_app_rule_kept_unwatched() {
    let core = late()
        .config("websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n")
        .then(accessibility_granted())
        .then(reported(true, custom_hours()))
        .start();
    activate_browser(&core, "com.apple.Safari");
    core.handle(reported(false, custom_hours()));

    assert_eq!(
        core.handle(Event::Terminating),
        vec![Effect::SetNightShift { enabled: true }]
    );
    assert!(!core.status().watching);
}

#[test]
fn terminating_stops_watching_and_releases_without_waiting_for_the_host() {
    let core = late()
        .config("websites = true\n[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .then(accessibility_granted())
        .then(reported(true, custom_hours()))
        .then(activate_event("com.figma.Desktop"))
        .then(reported(false, custom_hours()))
        .start();
    assert_eq!(
        activate_browser(&core, "com.apple.Safari"),
        vec![watch("com.apple.Safari", 1)]
    );

    assert_eq!(
        core.handle(Event::Terminating),
        vec![
            Effect::StopWatching,
            Effect::SetNightShift { enabled: true }
        ]
    );
}

#[test]
fn after_a_crash_night_shift_left_off_by_a_force_is_not_an_external_change() {
    let crashed = forced();

    let core = crashed.restart_at("2026-10-16T23:10:00+02:00");
    activate(&core, "com.apple.Terminal");
    let effects = core.handle(reported(false, custom_hours()));

    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn after_a_clean_exit_night_shift_found_off_is_an_external_change() {
    let stopped = forced();
    stopped.handle(Event::Terminating);

    let core = stopped.restart_at("2026-10-16T23:10:00+02:00");
    activate(&core, "com.apple.Terminal");
    core.handle(reported(false, custom_hours()));

    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, Some("2026-10-17T22:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn after_a_clean_exit_a_force_still_in_place_comes_back() {
    let stopped = forced();
    stopped.handle(Event::Terminating);

    let core = stopped.restart_at("2026-10-16T23:10:00+02:00");
    activate(&core, "com.figma.Desktop");
    // Night Shift as the release on exit left it.
    let effects = core.handle(reported(true, custom_hours()));

    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn update_in_a_self_managed_release_starts_sparkles_check() {
    let core = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .start();

    let (effects, response) = core.request(Request::Update);

    assert_eq!(effects, vec![Effect::CheckForUpdates]);
    assert_eq!(response, Response::Ok(Payload::UpdateCheckStarted));
}

#[test]
fn update_is_refused_where_nightcap_doesnt_update_itself() {
    for (instance, install) in [
        (Instance::Release, Install::Managed),
        (Instance::Dev, Install::SelfManaged),
    ] {
        let core = noon().installed(instance, install).start();

        let (effects, response) = core.request(Request::Update);

        assert_eq!(effects, Vec::new());
        let Response::Error(error) = response else {
            panic!("expected an error, got {response:?}");
        };
        assert_eq!(error.code, ErrorCode::NoUpdater);
    }
}

/// Starts at 23:00 with Night Shift on by the custom hours, then turns it off for an hour.
fn disabled() -> Harness {
    let core = late().then(reported(true, custom_hours())).start();
    core.send(set(Place::Everywhere, OnOff::Off, minutes(60)));
    core.handle(reported(false, custom_hours()));
    core
}

#[test]
fn while_another_user_is_active_nightcap_writes_nothing() {
    let core = disabled();
    core.handle(Event::SessionChanged { active: false });

    core.set_clock("2026-10-17T00:00:00+02:00");

    assert_eq!(
        core.handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
}

#[test]
fn status_callbacks_while_another_user_is_active_are_ignored() {
    let core = forced();
    core.handle(Event::SessionChanged { active: false });

    let effects = core.handle(reported(true, custom_hours()));

    assert_eq!(effects, Vec::new());
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn returning_to_the_session_puts_night_shift_where_nightcap_wants_it() {
    let core = disabled();
    core.handle(Event::SessionChanged { active: false });
    core.set_clock("2026-10-17T00:00:00+02:00");
    core.handle(Event::TimerFired);

    assert_eq!(
        core.handle(Event::SessionChanged { active: true }),
        Vec::new()
    );
    // The shell's fresh read: still off, as the timer left it.
    let effects = core.handle(reported(false, custom_hours()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn a_domain_rule_turns_night_shift_off_on_its_site() {
    let core = with_figma_domain().start();

    assert_eq!(
        activate_browser(&core, "com.apple.Safari"),
        vec![watch("com.apple.Safari", 1)]
    );
    assert_eq!(
        report_host(&core, 1, "www.figma.com"),
        vec![Effect::SetNightShift { enabled: false }]
    );

    let status = core.status();
    assert!(status.watching);
    assert_eq!(status.host.as_deref(), Some("www.figma.com"));
    assert_eq!(
        status.decision.reasons,
        vec![Reason::Rule(domain_rule("figma.com"))]
    );
}

/// Safari in front on figma.com, Night Shift forced off and reported off.
fn on_figma() -> Scenario {
    with_figma_domain()
        .then(activate_browser_event("com.apple.Safari"))
        .then(host_event(1, "www.figma.com"))
        .then(reported(false, custom_hours()))
}

#[test]
fn a_host_from_an_older_watch_is_dropped() {
    let core = with_figma_domain().start();
    activate_browser(&core, "com.apple.Safari");
    assert_eq!(
        activate_browser(&core, "com.google.Chrome"),
        vec![watch("com.google.Chrome", 2)]
    );

    assert_eq!(report_host(&core, 1, "www.figma.com"), Vec::new());

    assert_eq!(core.status().host, None);
}

#[test]
fn leaving_the_browser_stops_watching_and_lifts_the_force() {
    let core = on_figma().start();

    assert_eq!(
        activate(&core, "com.apple.Terminal"),
        vec![
            Effect::StopWatching,
            Effect::SetNightShift { enabled: true }
        ]
    );
}

#[test]
fn with_website_checking_on_the_front_browser_is_watched_without_any_domain_rule() {
    let core = late().config("websites = true\n").start();
    grant_accessibility(&core);
    core.handle(reported(true, custom_hours()));

    assert_eq!(
        activate_browser(&core, "com.apple.Safari"),
        vec![watch("com.apple.Safari", 1)]
    );
}

#[test]
fn a_new_watch_leaves_night_shift_alone_until_its_first_read() {
    let core = on_figma().start();

    // Without a host yet, Chrome alone wouldn't force Night Shift off, but switching it on until
    // the first read would only flash it.
    assert_eq!(
        activate_browser(&core, "com.google.Chrome"),
        vec![watch("com.google.Chrome", 2)]
    );
    assert_eq!(report_host(&core, 2, "www.figma.com"), Vec::new());
}

#[test]
fn an_unreadable_host_leaves_only_app_rules() {
    let core = on_figma().start();

    let effects = core.handle(Event::HostChanged {
        generation: 1,
        host: None,
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    let status = core.status();
    assert!(status.watching);
    assert_eq!(status.host, None);
}

#[test]
fn domains_in_the_config_are_normalized_and_checked() {
    let core = noon()
        .config("[rules.off]\ndomain = [\"Bücher.DE.\"]\n")
        .start();

    assert_eq!(core.status().rules, vec![domain_rule("xn--bcher-kva.de")]);
    for entry in [
        "https://figma.com",
        "figma.com/files",
        "figma.com:443",
        "*.figma.com",
        "",
    ] {
        core.write_config(&format!("[rules.off]\ndomain = [{entry:?}]\n"));
        let (_, response) = core.request(Request::Reload);
        let Response::Error(error) = response else {
            panic!("{entry:?} was accepted: {response:?}");
        };
        assert_eq!(error.code, ErrorCode::InvalidConfig, "{entry:?}");
    }
}

#[test]
fn hosts_compare_in_normal_form() {
    let core = with_figma_domain().start();
    activate_browser(&core, "com.apple.Safari");

    assert_eq!(
        report_host(&core, 1, "WWW.FIGMA.COM."),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn an_added_rule_applies_at_once_and_survives_a_restart() {
    let core = on_by_the_schedule().start();
    activate(&core, "com.figma.Desktop");

    let (effects, response) = core.send(add_rule(figma()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(ad_hoc(figma()))],
        }))
    );
    let core = core.restart_at("2026-10-16T23:10:00+02:00");
    assert_eq!(core.rules(), vec![ad_hoc(figma())]);
}

fn refusal(response: Response) -> ErrorCode {
    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    error.code
}

#[test]
fn an_added_domain_is_normalized() {
    let core = noon().start();
    let add = r#"{"command":{"add_rule":{"identity":"domain:Figma.COM","says":"off"}}}"#;

    core.request(serde_json::from_str(add).unwrap());

    assert_eq!(
        core.rules(),
        vec![ad_hoc(RuleIdentity::Domain("figma.com".parse().unwrap()))]
    );
}

#[test]
fn removing_an_ad_hoc_rule_lifts_it() {
    let core = on_by_the_schedule().start();
    activate(&core, "com.figma.Desktop");
    core.send(add_rule(figma()));
    core.handle(reported(false, custom_hours()));

    let (effects, _) = core.send(Command::RemoveRule(figma()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(core.rules(), Vec::new());
}

#[test]
fn a_rule_that_doesnt_exist_is_refused() {
    let core = noon().start();

    let (_, removed) = core.send(Command::RemoveRule(figma()));

    assert_eq!(refusal(removed), ErrorCode::NoSuchRule);
}

#[test]
fn ad_hoc_rules_from_a_newer_build_are_left_alone() {
    let newer = "version = 3\n[rules.off]\napp = [\"com.figma.Desktop\"]\n";
    let core = noon().rules(newer).start();

    let (_, response) = core.send(add_rule(figma()));

    assert_eq!(refusal(response), ErrorCode::AdHocRulesOff);
    assert!(core.status().ad_hoc_error.is_some());
    assert_eq!(core.rules(), Vec::new());
    assert_eq!(core.file(File::Rules).as_deref(), Some(newer));
}

#[test]
fn ad_hoc_rules_with_an_app_no_rule_could_name_are_left_alone() {
    let invalid = "version = 2\n[rules.off]\napp = [\"com.figma.Desktop \"]\n";
    let core = noon().rules(invalid).start();

    let status = core.status();

    assert!(
        status
            .ad_hoc_error
            .is_some_and(|error| error.contains("\"com.figma.Desktop \" isn't a bundle ID")),
    );
    assert_eq!(status.rules, Vec::new());
    assert_eq!(core.file(File::Rules).as_deref(), Some(invalid));
}

#[test]
fn an_app_whose_bundle_id_no_rule_could_name_counts_as_having_none() {
    let core = on_by_the_schedule().start();

    activate(&core, "unity.Team Cherry.Hollow Knight");

    assert_eq!(core.status().frontmost_app, None);
    let app_rows = core.menu().entries.into_iter().filter(|entry| {
        matches!(entry, MenuEntry::Row { row } if matches!(row.image, RowImage::App { .. }))
    });
    assert_eq!(app_rows.count(), 0);
}

#[test]
fn the_rules_query_lists_every_rule_with_its_source() {
    let core = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();

    core.send(add_rule(RuleIdentity::Domain("figma.com".parse().unwrap())));

    assert_eq!(
        core.rules(),
        vec![
            app_rule("com.figma.Desktop"),
            ad_hoc(RuleIdentity::Domain("figma.com".parse().unwrap())),
        ]
    );
}

#[test]
fn a_browser_whose_place_says_off_is_watched_while_a_site_in_it_could_say_on() {
    let core = late()
        .config("websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n")
        .start();
    grant_accessibility(&core);
    core.handle(reported(true, custom_hours()));
    let safari_app = Place::App("com.apple.Safari".parse().unwrap());

    assert_eq!(
        activate_browser(&core, "com.apple.Safari"),
        vec![Effect::SetNightShift { enabled: false }]
    );
    let (effects, _) = core.send(set(
        safari_app.clone(),
        OnOff::On,
        Length::UntilScheduleTurns,
    ));
    assert!(effects.contains(&watch("com.apple.Safari", 1)));
    let (effects, _) = core.send(Command::Reset(safari_app));
    assert!(effects.contains(&Effect::StopWatching));
    let (effects, _) = core.send(Command::AddRule {
        identity: RuleIdentity::Domain("youtube.com".parse().unwrap()),
        says: OnOff::On,
    });
    assert!(effects.contains(&watch("com.apple.Safari", 2)));
}

#[test]
fn snoozing_a_domain_rule_lifts_it_and_keeps_watching() {
    let core = on_figma().start();
    let figma_site = Place::Domain("figma.com".parse().unwrap());

    let (effects, _) = core.send(set(
        figma_site.clone(),
        OnOff::On,
        Length::UntilScheduleTurns,
    ));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    let status = core.status();
    assert_eq!(
        status.layers,
        vec![layered(
            figma_site,
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );
    assert!(status.watching);
}

#[test]
fn a_config_rule_cant_be_replaced_or_removed() {
    let core = forced();

    for command in [
        Command::RemoveRule(figma()),
        Command::AddRule {
            identity: figma(),
            says: OnOff::On,
        },
    ] {
        let (effects, response) = core.send(command.clone());
        assert_eq!(effects, Vec::new(), "{command:?}");
        let Response::Error(error) = response else {
            panic!("{command:?} was accepted: {response:?}");
        };
        assert_eq!(error.code, ErrorCode::ConfigRule, "{command:?}");
        assert!(
            error.message.contains("change it there"),
            "{}",
            error.message
        );
    }
    assert_eq!(core.rules(), vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn removing_a_config_rule_from_the_menu_is_refused() {
    let core = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();

    let (effects, outcome) = core.choose(Command::RemoveRule(figma()));

    assert_eq!(effects, Vec::new());
    assert_eq!(outcome.unwrap_err().code, ErrorCode::ConfigRule);
    assert_eq!(core.rules(), vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn always_from_the_menu_while_rules_toml_is_from_a_newer_nightcap_is_refused() {
    let newer = "version = 3\n[rules.off]\napp = [\"com.apple.mail\"]\n";
    let core = late()
        .rules(newer)
        .then(reported(true, custom_hours()))
        .then(activate_event("com.figma.Desktop"))
        .start();
    let always = choice_titled(&app_row(&core).unwrap(), "Always").command;

    let (effects, outcome) = core.choose(always);

    assert_eq!(effects, Vec::new());
    assert_eq!(outcome.unwrap_err().code, ErrorCode::AdHocRulesOff);
    assert_eq!(core.rules(), Vec::new());
    assert_eq!(core.file(File::Rules).as_deref(), Some(newer));
}

fn mail() -> RuleIdentity {
    RuleIdentity::App("com.apple.mail".parse().unwrap())
}

#[test]
fn always_makes_the_users_rule_say_that_way_replacing_one_that_said_the_other() {
    let core = on_by_the_schedule().start();
    activate(&core, "com.apple.mail");
    core.send(add_rule(mail()));
    core.send(set(Place::from(mail()), OnOff::On, minutes(30)));

    core.send(Command::AddRule {
        identity: mail(),
        says: OnOff::On,
    });

    let status = core.status();
    assert_eq!(
        status.rules,
        vec![Rule {
            identity: mail(),
            source: RuleSource::AdHoc,
            says: OnOff::On,
        }]
    );
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn adding_your_rule_again_from_settings_ends_its_snooze_and_leaves_rules_toml_as_it_is() {
    let written = "version = 2\n[rules.off]\napp = [\"com.apple.mail\"]\n";
    let core = on_by_the_schedule().rules(written).start();
    activate(&core, "com.apple.mail");
    core.handle(reported(false, custom_hours()));
    core.choose(set(Place::from(mail()), OnOff::On, minutes(30)))
        .1
        .unwrap();
    core.handle(reported(true, custom_hours()));

    let (effects, outcome) = core.choose(add_rule(mail()));

    assert!(effects.contains(&Effect::SetNightShift { enabled: false }));
    assert_eq!(outcome.unwrap().place, Place::from(mail()));
    assert_eq!(core.status().layers, Vec::new());
    assert_eq!(core.file(File::Rules).as_deref(), Some(written));
}

/// At noon, off by the custom hours, with Books in front.
fn in_books_at_noon() -> Scenario {
    noon()
        .then(reported(false, custom_hours()))
        .then(activate_event("com.apple.iBooksX"))
}

fn books() -> RuleIdentity {
    RuleIdentity::App("com.apple.iBooksX".parse().unwrap())
}

#[test]
fn an_app_rule_that_says_on_turns_night_shift_on_by_day() {
    let core = in_books_at_noon().start();

    let (effects, _) = core.send(Command::AddRule {
        identity: books(),
        says: OnOff::On,
    });
    core.handle(reported(true, custom_hours()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        activate(&core, "com.apple.mail"),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn a_rule_that_says_on_is_released_on_a_clean_exit() {
    let core = in_books_at_noon().start();
    core.send(Command::AddRule {
        identity: books(),
        says: OnOff::On,
    });
    core.handle(reported(true, custom_hours()));

    assert_eq!(
        core.handle(Event::Terminating),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn an_ad_hoc_rule_graduates_whatever_either_says() {
    let core = noon().start();
    core.send(Command::AddRule {
        identity: figma(),
        says: OnOff::On,
    });

    core.write_config("[rules.off]\napp = [\"com.figma.desktop\"]\n");
    core.request(Request::Reload);

    assert_eq!(core.rules(), vec![app_rule("com.figma.desktop")]);
    assert_eq!(kept_ad_hoc(&core), Vec::new());
}

#[test]
fn export_writes_both_lists() {
    let core = noon().start();
    core.send(add_rule(figma()));
    core.send(Command::AddRule {
        identity: RuleIdentity::Domain("youtube.com".parse().unwrap()),
        says: OnOff::On,
    });

    assert_eq!(
        core.export(ExportFormat::Nix),
        "{ off = { app = [ \"com.figma.Desktop\" ]; domain = [ ]; }; on = { app = [ ]; domain = [ \"youtube.com\" ]; }; }\n"
    );
    assert_eq!(
        core.export(ExportFormat::Toml),
        "[rules.off]\napp = [\"com.figma.Desktop\"]\n\n[rules.on]\ndomain = [\"youtube.com\"]\n"
    );
}

#[test]
fn rules_are_listed_by_what_they_say() {
    let core = noon().config("[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.apple.iBooksX\"]\ndomain = [\"youtube.com\"]\n").start();

    let on = |identity| Rule {
        identity,
        source: RuleSource::Config,
        says: OnOff::On,
    };
    assert_eq!(
        core.status().rules,
        vec![
            app_rule("com.figma.Desktop"),
            on(books()),
            on(RuleIdentity::Domain("youtube.com".parse().unwrap())),
        ]
    );
}

#[test]
fn a_config_that_cant_be_is_refused() {
    let core = noon().start();

    for (config, said) in [
        (
            "[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.figma.desktop\"]\n",
            "both rules.off and rules.on",
        ),
        (
            "[rules]\napp = [\"com.figma.Desktop\"]\n",
            "under [rules.off] and [rules.on]",
        ),
        ("[menu]\nlengths = [\"45m\", \"forever\"]\n", "menu.lengths"),
        ("[menu]\nlengths = [\"30s\"]\n", "menu.lengths"),
        ("[menu]\nlengths = []\n", "menu.lengths"),
        (
            "[rules.off]\napp = [\"com.figma.Desktop \"]\n",
            "\"com.figma.Desktop \" isn't a bundle ID",
        ),
        ("[rules.on]\napp = [\"\"]\n", "\"\" isn't a bundle ID"),
        (
            "[rules.off]\ndomain = [\"https://figma.com\"]\n",
            "\"https://figma.com\" isn't a domain",
        ),
    ] {
        core.write_config(config);
        let (_, response) = core.request(Request::Reload);
        let Response::Error(error) = response else {
            panic!("{config:?} was accepted: {response:?}");
        };
        assert_eq!(error.code, ErrorCode::InvalidConfig, "{config:?}");
        assert!(
            error.message.contains(said),
            "{config:?}: {}",
            error.message
        );
    }
}

/// The menu's Night Shift item: checked while it's on everywhere, with what decides the
/// screen.
fn night_shift_item(on: bool, reason: &str, command: Command) -> NightShiftItem {
    NightShiftItem {
        on,
        reason: Some(reason.to_owned()),
        command: Some(command),
    }
}

#[test]
fn the_menu_says_what_night_shift_is_doing_and_why() {
    let core = forced();
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    assert_eq!(core.menu().icon, MenuIcon::ForcedOff);
    assert_eq!(
        core.menu().night_shift,
        night_shift_item(true, "Off in Figma: your config’s rule", Command::Off)
    );
    core.send(set(Place::Everywhere, OnOff::Off, minutes(30)));
    assert_eq!(
        core.menu().night_shift,
        night_shift_item(false, "Off in Figma: your config’s rule", Command::On)
    );
    activate(&core, "com.apple.Terminal");
    assert_eq!(core.menu().icon, MenuIcon::Off);
    assert_eq!(
        core.menu().night_shift,
        night_shift_item(false, "Off until 23:30", Command::On)
    );
    core.send(Command::On);
    assert_eq!(
        core.menu().night_shift,
        night_shift_item(true, "Following the schedule until 07:00", Command::Off)
    );
    core.send(Command::Off);
    assert_eq!(
        core.menu().night_shift,
        night_shift_item(false, "Off until you turn it on", Command::On)
    );
}

#[test]
fn while_night_shift_is_unavailable_the_menu_only_says_so() {
    let core = noon().start();

    let menu = core.menu();

    assert_eq!(menu.icon, MenuIcon::Unavailable);
    assert_eq!(
        menu.night_shift,
        NightShiftItem {
            on: false,
            reason: Some("Unavailable".to_owned()),
            command: None,
        }
    );
    assert_eq!(menu.entries, Vec::new());
}

/// The menu's place rows, top down.
fn rows(core: &Core) -> Vec<PlaceRow> {
    core.menu()
        .entries
        .into_iter()
        .filter_map(|entry| match entry {
            MenuEntry::Row { row } => Some(row),
            _ => None,
        })
        .collect()
}

fn everywhere_row(core: &Core) -> Option<PlaceRow> {
    rows(core)
        .into_iter()
        .find(|row| row.image == RowImage::Everywhere)
}

fn app_row(core: &Core) -> Option<PlaceRow> {
    rows(core)
        .into_iter()
        .find(|row| matches!(row.image, RowImage::App { .. }))
}

fn site_row(core: &Core) -> Option<PlaceRow> {
    rows(core)
        .into_iter()
        .find(|row| row.image == RowImage::Site)
}

/// A site row's title, and the site it names under it.
fn site_title(row: &PlaceRow) -> (&str, Option<&str>) {
    (&row.title, row.site.as_deref())
}

/// A row's items as they read: their titles, "Custom…" for picking a time, and "—" between
/// groups.
fn titles(row: &PlaceRow) -> Vec<String> {
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
fn choice_titled(row: &PlaceRow, title: &str) -> MenuItem {
    row.items
        .iter()
        .find_map(|item| match item {
            RowItem::Choice { item } if item.title == title => Some(item.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {title:?} in {:?}", titles(row)))
}

fn choice(title: &str, command: Command) -> RowItem {
    RowItem::Choice {
        item: MenuItem {
            title: title.to_owned(),
            subtitle: None,
            enabled: true,
            command,
        },
    }
}

#[test]
fn the_menu_has_everywheres_row_then_the_apps_and_the_sites() {
    let core = on_figma().start();
    core.handle(Event::StrengthChanged { value: 60 });

    let entries = core.menu().entries;

    let kinds: Vec<_> = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Strength { .. } => "strength",
            MenuEntry::Separator => "—",
            MenuEntry::Row { row } => match row.image {
                RowImage::Everywhere => "everywhere",
                RowImage::App { .. } => "app",
                RowImage::Site => "site",
            },
        })
        .collect();
    assert_eq!(kinds, ["strength", "everywhere", "—", "app", "site"]);
    assert!(site_row(&core).unwrap().indented);
}

#[test]
fn everywheres_row_has_every_length_but_the_switchs() {
    let core = on_by_the_schedule().start();

    let menu = core.menu();

    assert_eq!(menu.icon, MenuIcon::On);
    assert_eq!(menu.night_shift.command, Some(Command::Off));
    let off = |length| set(Place::Everywhere, OnOff::Off, length);
    assert_eq!(
        everywhere_row(&core),
        Some(PlaceRow {
            title: "Turn Off".to_owned(),
            site: None,
            subtitle: None,
            image: RowImage::Everywhere,
            indented: false,
            items: vec![
                choice("For 30 Minutes", off(minutes(30))),
                choice("For 1 Hour", off(minutes(60))),
                choice("For 2 Hours", off(minutes(120))),
                RowItem::PickTime {
                    title: "Turn Off Everywhere".to_owned(),
                    place: Place::Everywhere,
                    state: OnOff::Off,
                    start: at("2026-10-17T00:00:00+02:00"),
                },
                RowItem::Choice {
                    item: MenuItem {
                        title: "Until 22:00".to_owned(),
                        subtitle: Some("When the schedule starts".to_owned()),
                        enabled: true,
                        command: off(Length::UntilScheduleTurns),
                    },
                },
            ],
        })
    );
}

/// The line under everywhere's "Until" item at `time`, with Night Shift `enabled` as the
/// schedule has it then.
fn until_note(time: &str, enabled: bool, schedule: Schedule, title: &str) -> Option<String> {
    let core = Scenario::at(time).start();
    report_zurich_sun(&core);
    core.handle(reported(enabled, schedule));
    choice_titled(&everywhere_row(&core).unwrap(), title).subtitle
}

#[test]
fn until_the_schedule_turns_names_sunrise_or_sunset() {
    assert_eq!(
        until_note(
            "2026-10-16T01:40:00+02:00",
            true,
            Schedule::SunsetToSunrise,
            "Until 18:31"
        )
        .as_deref(),
        Some("At sunset")
    );
    assert_eq!(
        until_note(
            "2026-10-16T12:00:00+02:00",
            false,
            Schedule::SunsetToSunrise,
            "Until 07:52"
        )
        .as_deref(),
        Some("At sunrise")
    );
}

#[test]
fn until_the_schedule_turns_names_custom_hours_start_or_end() {
    assert_eq!(
        until_note(
            "2026-10-16T23:00:00+02:00",
            true,
            custom_hours(),
            "Until 22:00"
        )
        .as_deref(),
        Some("When the schedule starts")
    );
    assert_eq!(
        until_note(
            "2026-10-16T12:00:00+02:00",
            false,
            custom_hours(),
            "Until 07:00"
        )
        .as_deref(),
        Some("When the schedule ends")
    );
}

#[test]
fn the_menus_lengths_come_from_the_config() {
    let core = on_by_the_schedule()
        .config("[menu]\nlengths = [\"15m\", \"1h30m\"]\n")
        .start();

    assert_eq!(
        titles(&everywhere_row(&core).unwrap()),
        [
            "For 15 Minutes",
            "For 1 Hour 30 Minutes",
            "Custom…",
            "Until 22:00"
        ]
    );
}

#[test]
fn everywheres_row_names_the_place_that_keeps_the_other_way() {
    let core = noon().config("[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.apple.iBooksX\"]\n").start();
    core.handle(reported(false, custom_hours()));
    let front = |bundle_id: &str, name: &str| {
        core.handle(Event::AppActivated {
            bundle_id: Some(bundle_id.to_owned()),
            name: Some(name.to_owned()),
            is_browser: false,
        });
        let row = everywhere_row(&core).unwrap();
        (row.title, row.subtitle)
    };
    let row = |title: &str, subtitle: Option<&str>| (title.to_owned(), subtitle.map(str::to_owned));

    // By day, turning Night Shift on everywhere leaves Figma off, and Books on as it is.
    assert_eq!(
        front("com.figma.Desktop", "Figma"),
        row("Turn On", Some("Everywhere but in Figma"))
    );
    assert_eq!(front("com.apple.iBooksX", "Books"), row("Turn On", None));
    // At night, turning it off everywhere leaves Books on, and Figma off as it is.
    core.set_clock("2026-10-16T23:00:00+02:00");
    core.handle(Event::Woke);
    assert_eq!(
        front("com.apple.iBooksX", "Books"),
        row("Turn Off", Some("Everywhere but in Books"))
    );
    assert_eq!(front("com.figma.Desktop", "Figma"), row("Turn Off", None));
}

#[test]
fn by_day_the_rows_turn_on_and_by_night_off() {
    let core = noon().start();
    core.handle(reported(false, custom_hours()));
    core.handle(Event::AppActivated {
        bundle_id: Some("com.apple.mail".to_owned()),
        name: Some("Mail".to_owned()),
        is_browser: false,
    });
    let titles_of =
        |core: &Core| -> Vec<String> { rows(core).into_iter().map(|row| row.title).collect() };

    assert_eq!(titles_of(&core), ["Turn On", "Turn On in Mail"]);
    core.set_clock("2026-10-16T23:00:00+02:00");
    core.handle(Event::Woke);
    assert_eq!(titles_of(&core), ["Turn Off", "Turn Off in Mail"]);
}

#[test]
fn without_the_sun_times_an_apps_row_turns_night_shift_the_other_way_from_how_it_is() {
    let core = late().start();
    core.handle(Event::AppActivated {
        bundle_id: Some("com.googlecode.iterm2".to_owned()),
        name: Some("iTerm2".to_owned()),
        is_browser: false,
    });

    core.handle(reported(true, Schedule::SunsetToSunrise));
    assert_eq!(app_row(&core).unwrap().title, "Turn Off in iTerm2");
    core.handle(reported(false, Schedule::SunsetToSunrise));
    assert_eq!(app_row(&core).unwrap().title, "Turn On in iTerm2");
}

#[test]
fn without_the_sun_times_the_menu_follows_the_schedule_as_macos_applies_it() {
    let core = late().start();

    core.handle(reported(true, Schedule::SunsetToSunrise));

    let menu = core.menu();
    assert!(menu.night_shift.on);
    assert_eq!(
        menu.night_shift.reason.as_deref(),
        Some("Following the schedule")
    );
    let row = everywhere_row(&core).unwrap();
    assert_eq!(row.title, "Turn Off");
    assert_eq!(
        titles(&row),
        ["For 30 Minutes", "For 1 Hour", "For 2 Hours", "Custom…"]
    );
    assert!(
        core.handle(Event::CommandChosen {
            command: choice_titled(&row, "For 30 Minutes").command,
        })
        .contains(&Effect::SetNightShift { enabled: false })
    );
}

#[test]
fn undoing_is_offered_where_it_goes_the_right_way_and_a_rule_of_yours_can_always_go() {
    let core = forced();
    core.send(set(figma_app(), OnOff::On, Length::UntilScheduleTurns));
    assert_eq!(
        app_row(&core).unwrap().items[0],
        choice("Put the Rule Back", Command::Reset(figma_app()))
    );

    activate(&core, "com.apple.mail");
    core.send(set(Place::from(mail()), OnOff::Off, minutes(30)));
    assert_eq!(
        app_row(&core).unwrap().items[0],
        choice("Like Everywhere Else", Command::Reset(Place::from(mail())))
    );

    core.send(Command::Reset(Place::from(mail())));
    core.send(add_rule(mail()));
    assert_eq!(
        choice_titled(&app_row(&core).unwrap(), "Remove Your Rule").command,
        Command::RemoveRule(mail())
    );
    // By day removing it changes nothing on screen, but the rule is still the user's to remove.
    core.set_clock("2026-10-17T12:00:00+02:00");
    core.handle(Event::Woke);
    assert_eq!(
        choice_titled(&app_row(&core).unwrap(), "Remove Your Rule").command,
        Command::RemoveRule(mail())
    );
}

#[test]
fn lengths_that_come_to_the_same_as_undoing_are_left_out() {
    let core = forced();
    core.send(set(figma_app(), OnOff::On, Length::UntilScheduleTurns));

    assert_eq!(
        titles(&app_row(&core).unwrap()),
        [
            "Put the Rule Back",
            "—",
            "For 30 Minutes",
            "For 1 Hour",
            "For 2 Hours",
            "Custom…"
        ]
    );
    activate(&core, "com.apple.mail");
    core.send(set(Place::from(mail()), OnOff::Off, minutes(30)));
    assert_eq!(
        titles(&app_row(&core).unwrap()),
        ["Like Everywhere Else", "—", "Always"]
    );
}

#[test]
fn a_config_rules_always_is_greyed() {
    let core = forced();

    let row = app_row(&core).unwrap();

    assert_eq!(row.title, "Turn On in com.figma.Desktop");
    assert_eq!(
        row.subtitle.as_deref(),
        Some("Always off: your config’s rule")
    );
    let always = choice_titled(&row, "Always");
    assert!(!always.enabled);
    assert_eq!(always.subtitle.as_deref(), Some("Your config decides"));
}

#[test]
fn custom_is_offered_beside_the_lengths_with_its_place_and_way() {
    let core = forced();
    core.set_clock("2026-10-16T23:10:00+02:00");
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    let row = app_row(&core).unwrap();

    assert!(row.items.contains(&RowItem::PickTime {
        title: "Turn On in Figma".to_owned(),
        place: figma_app(),
        state: OnOff::On,
        start: at("2026-10-17T00:15:00+02:00"),
    }));
}

#[test]
fn the_site_row_is_the_tab_domain_or_the_most_specific_covering_rules() {
    let core = on_by_the_schedule().config("websites = true\n").start();
    activate_browser(&core, "com.apple.Safari");
    report_host(&core, 1, "docs.figma.com");
    let row = site_row(&core).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn Off at This Site", Some("docs.figma.com"))
    );
    assert_eq!(
        choice_titled(&row, "Always").command,
        add_rule(domain("docs.figma.com"))
    );
    report_host(&core, 1, "www.figma.com");
    assert_eq!(
        site_title(&site_row(&core).unwrap()),
        ("Turn Off at This Site", Some("figma.com"))
    );

    report_host(&core, 1, "docs.figma.com");
    core.send(add_rule(domain("figma.com")));
    assert_eq!(
        site_title(&site_row(&core).unwrap()),
        ("Turn On at This Site", Some("figma.com"))
    );
    core.send(add_rule(domain("docs.figma.com")));
    assert_eq!(
        site_title(&site_row(&core).unwrap()),
        ("Turn On at This Site", Some("docs.figma.com"))
    );
}

#[test]
fn a_site_goes_back_to_its_browsers_say_by_the_browsers_name() {
    let core = on_by_the_schedule().config("websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n").start();
    core.handle(Event::AppActivated {
        bundle_id: Some("com.apple.Safari".to_owned()),
        name: Some("Safari".to_owned()),
        is_browser: true,
    });
    report_host(&core, 1, "example.com");
    let example = Place::Domain("example.com".parse().unwrap());

    core.send(set(example.clone(), OnOff::On, minutes(30)));

    let row = site_row(&core).unwrap();
    assert_eq!(
        choice_titled(&row, "Like the Rest of Safari").command,
        Command::Reset(example)
    );
}

#[test]
fn a_site_that_says_nothing_turns_the_other_way_from_its_browser() {
    let core = on_by_the_schedule().config("websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n").start();
    activate_browser(&core, "com.apple.Safari");

    report_host(&core, 1, "example.com");

    let row = site_row(&core).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn On at This Site", Some("example.com"))
    );
    assert_eq!(
        choice_titled(&row, "Always").command,
        Command::AddRule {
            identity: domain("example.com"),
            says: OnOff::On,
        }
    );
}

#[test]
fn the_site_row_snoozes_a_config_rule() {
    let core = on_figma().start();
    let figma_site = Place::Domain("figma.com".parse().unwrap());

    let row = site_row(&core).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn On at This Site", Some("figma.com"))
    );
    assert_eq!(
        row.subtitle.as_deref(),
        Some("Always off: your config’s rule")
    );
    let until_morning = choice_titled(&row, "Until 07:00").command;
    assert_eq!(
        until_morning,
        set(figma_site.clone(), OnOff::On, Length::UntilScheduleTurns)
    );
    core.send(until_morning);
    let row = site_row(&core).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn Off at This Site", Some("figma.com"))
    );
    assert_eq!(
        row.subtitle.as_deref(),
        Some("On until 07:00, then your config’s rule")
    );
    assert_eq!(
        row.items[0],
        choice("Put the Rule Back", Command::Reset(figma_site))
    );
}

#[test]
fn the_menu_opens_with_a_strength_slider_once_the_strength_is_known() {
    let core = noon().start();
    core.handle(reported(false, Schedule::Off));
    assert!(
        !core
            .menu()
            .entries
            .iter()
            .any(|entry| matches!(entry, MenuEntry::Strength { .. }))
    );

    core.handle(Event::StrengthChanged { value: 60 });

    assert_eq!(core.menu().entries[0], MenuEntry::Strength { value: 60 });
}

#[test]
fn the_menu_adds_and_removes_an_ad_hoc_rule_for_the_frontmost_app() {
    let core = on_by_the_schedule().start();
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    let row = app_row(&core).unwrap();
    assert_eq!(row.title, "Turn Off in Figma");
    assert_eq!(row.subtitle, None);
    assert_eq!(choice_titled(&row, "Always").command, add_rule(figma()));
    core.send(add_rule(figma()));
    let row = app_row(&core).unwrap();
    assert_eq!(row.title, "Turn On in Figma");
    assert_eq!(row.subtitle.as_deref(), Some("Always off: your rule"));
    assert_eq!(
        choice_titled(&row, "Remove Your Rule").command,
        Command::RemoveRule(figma())
    );
}

#[test]
fn choosing_a_menu_item_runs_its_command() {
    let core = on_by_the_schedule().start();
    activate(&core, "com.figma.Desktop");

    let effects = core.handle(Event::CommandChosen {
        command: add_rule(figma()),
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
}

#[test]
fn website_checking_is_off_until_turned_on() {
    let core = on_by_the_schedule()
        .config("[rules.off]\ndomain = [\"figma.com\"]\n")
        .start();

    assert_eq!(activate_browser(&core, "com.apple.Safari"), Vec::new());
    assert_eq!(
        core.status().website_checking,
        WebsiteChecking {
            state: OnOff::Off,
            from_config: false,
        }
    );

    let (effects, _) = core.send(Command::Websites(OnOff::On));

    assert!(
        effects.contains(&watch("com.apple.Safari", 1)),
        "{effects:?}"
    );
}

#[test]
fn the_config_decides_website_checking_when_it_sets_it() {
    let core = noon().config("websites = true\n").start();

    let (_, response) = core.send(Command::Websites(OnOff::Off));

    assert_eq!(refusal(response), ErrorCode::SetInConfig);
    assert_eq!(
        core.status().website_checking,
        WebsiteChecking {
            state: OnOff::On,
            from_config: true,
        }
    );
}

#[test]
fn turning_website_checking_on_without_accessibility_asks_for_it() {
    let core = noon().start();
    core.handle(Event::PermissionsChanged {
        accessibility: false,
        notifications: NotificationPermission::Granted,
    });

    let (effects, _) = core.send(Command::Websites(OnOff::On));

    assert_eq!(effects, vec![Effect::AskForAccessibility]);
}

#[test]
fn settings_switch_website_checking_unless_the_config_sets_it() {
    let core = on_by_the_schedule().start();
    let switch = |on, locked, command| SettingsSwitch {
        on,
        locked,
        command,
    };

    assert!(!rows(&core).iter().any(|row| row.title == "Check Websites"));
    assert_eq!(
        core.settings().website_checking,
        switch(false, false, Command::Websites(OnOff::On))
    );
    core.handle(Event::CommandChosen {
        command: Command::Websites(OnOff::On),
    });
    assert_eq!(
        core.settings().website_checking,
        switch(true, false, Command::Websites(OnOff::Off))
    );

    core.write_config("websites = false\n");
    core.request(Request::Reload);
    assert_eq!(
        core.settings().website_checking,
        switch(false, true, Command::Websites(OnOff::On))
    );
}

#[test]
fn the_menu_shows_a_problem_line_while_any_problem_exists() {
    let core = noon().config("websites = true\n").start();
    core.handle(reported(false, Schedule::Off));
    let line = |title: &str, subtitle: Option<&str>| {
        Some(ProblemLine {
            title: title.to_owned(),
            subtitle: subtitle.map(str::to_owned),
        })
    };

    assert_eq!(core.menu().problem_line, None);
    core.handle(permissions(false, NotificationPermission::Granted));
    assert_eq!(
        core.menu().problem_line,
        line("1 Thing Needs Attention…", Some("Accessibility"))
    );
    core.handle(permissions(false, NotificationPermission::Denied));
    assert_eq!(
        core.menu().problem_line,
        line(
            "2 Things Need Attention…",
            Some("Accessibility and notifications")
        )
    );
    core.handle(permissions(true, NotificationPermission::Granted));
    assert_eq!(core.menu().problem_line, None);
}

fn domain(name: &str) -> RuleIdentity {
    RuleIdentity::Domain(name.parse().unwrap())
}

#[test]
fn a_rule_from_the_tab_keeps_www_where_nothing_but_a_suffix_would_be_left() {
    let core = late().config("websites = true\n").start();
    report(&core, true);
    activate_browser(&core, "com.apple.Safari");
    report_host(&core, 1, "www.com");

    let row = site_row(&core).unwrap();
    assert_eq!(site_title(&row), ("Turn Off at This Site", Some("www.com")));
    assert_eq!(
        choice_titled(&row, "Always").command,
        add_rule(domain("www.com"))
    );
}

fn safari() -> RuleIdentity {
    RuleIdentity::App("com.apple.Safari".parse().unwrap())
}

#[test]
fn a_browser_its_own_app_rule_forces_off_is_not_watched() {
    let core = on_figma().start();

    let (effects, _) = core.send(add_rule(safari()));
    assert!(effects.contains(&Effect::StopWatching));
    assert_eq!(site_row(&core), None);
    assert_eq!(core.status().host, None);
    let (effects, _) = core.send(Command::RemoveRule(safari()));
    assert!(effects.contains(&watch("com.apple.Safari", 2)));
}

fn accessibility_notice() -> Effect {
    Effect::Notify {
        title: "Website checking needs Accessibility".to_owned(),
        body: "Open Nightcap’s settings to allow it, or to stop checking websites.".to_owned(),
        opens: None,
    }
}

fn notifies(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::Notify { .. }))
}

#[test]
fn a_problem_that_appears_on_its_own_is_announced_once() {
    let core = noon().config("websites = true\n").start();

    assert!(
        core.handle(permissions(false, NotificationPermission::Granted))
            .contains(&accessibility_notice())
    );
    assert!(!notifies(
        &core.handle(permissions(false, NotificationPermission::Granted))
    ));
    core.handle(permissions(true, NotificationPermission::Granted));
    assert!(
        core.handle(permissions(false, NotificationPermission::Granted))
            .contains(&accessibility_notice())
    );

    let core = core.restart_at("2026-10-16T12:10:00+02:00");
    assert!(!notifies(
        &core.handle(permissions(false, NotificationPermission::Granted))
    ));
}

#[test]
fn a_problem_the_user_causes_in_nightcap_is_not_announced() {
    let core = noon().start();
    core.handle(permissions(false, NotificationPermission::Granted));

    let effects = core.handle(Event::CommandChosen {
        command: Command::Websites(OnOff::On),
    });
    assert!(effects.contains(&Effect::AskForAccessibility));
    assert!(!notifies(&effects));
    assert!(!notifies(
        &core.handle(permissions(false, NotificationPermission::Granted))
    ));
}

#[test]
fn a_config_broken_at_launch_is_announced() {
    let core = noon()
        .config("[rules.off]\ndomian = [\"figma.com\"]\n")
        .start();

    let body = format!(
        "{}: line 2: unknown field `domian`, expected `app` or `domain`",
        core.settings().config_path
    );
    assert!(launch(&core).iter().any(|effect| matches!(
        effect,
        Effect::Notify { title, body: said, .. }
            if title == "Nightcap’s config is invalid" && *said == body
    )));
}

fn update_notice() -> Effect {
    let version = env!("CARGO_PKG_VERSION");
    Effect::Notify {
        title: format!("Nightcap updated to {version}"),
        body: "Click to see what’s new.".to_owned(),
        opens: Some(format!("https://nightcap.choco.me/changelog/#v{version}")),
    }
}

#[test]
fn the_first_launch_of_a_new_version_says_whats_new_once() {
    // An earlier version saved this after onboarding.
    let core = noon()
        .state(r#"{"version":1,"onboarding":1,"last_version":"1.0.0"}"#)
        .start();

    assert!(launch(&core).contains(&update_notice()));
    let core = core.restart_at("2026-10-16T12:10:00+02:00");
    assert!(!launch(&core).contains(&update_notice()));
}

#[test]
fn a_run_from_before_versions_were_kept_counts_as_an_update() {
    let core = noon().state(r#"{"version":1,"onboarding":1}"#).start();

    assert!(launch(&core).contains(&update_notice()));
}

#[test]
fn a_first_run_has_nothing_new_to_tell() {
    let core = noon().start();

    assert!(!notifies(&launch(&core)));
}

#[test]
fn onboarding_runs_at_the_first_launch_and_not_again() {
    let core = noon().start();

    assert!(
        core.handle(Event::Launched)
            .contains(&Effect::ShowOnboarding { completed: 0 })
    );
    core.handle(Event::OnboardingFinished { websites: true });
    assert_eq!(core.status().website_checking.state, OnOff::On);

    let core = core.restart_at("2026-10-16T12:10:00+02:00");
    assert!(
        !core
            .handle(Event::Launched)
            .iter()
            .any(|effect| matches!(effect, Effect::ShowOnboarding { .. }))
    );
}

fn shows_onboarding(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::ShowOnboarding { .. }))
}

#[test]
fn onboarding_asks_about_night_shift_starting_from_the_macs_values() {
    let core = noon().start();
    report_night_shift(&core, Schedule::Off, 60);

    assert_eq!(
        core.onboarding(),
        OnboardingSteps {
            night_shift: OnboardingNightShift {
                ask_strength: true,
                ask_schedule: true,
                already_set_up: false,
                strength: Some(60),
                schedule: Some(Schedule::Off),
            },
            websites_from_config: None,
            opens_at_login: None,
        }
    );
}

#[test]
fn onboarding_offers_to_skip_night_shift_once_its_on_a_schedule() {
    let core = noon().start();
    report_night_shift(&core, Schedule::SunsetToSunrise, 60);

    assert!(core.onboarding().night_shift.already_set_up);
}

#[test]
fn onboarding_leaves_out_what_the_config_sets() {
    let core = noon()
        .config("websites = true\n[night_shift]\nstrength = 40\n")
        .start();
    report_night_shift(&core, Schedule::Off, 60);

    let steps = core.onboarding();

    assert!(!steps.night_shift.ask_strength);
    assert!(steps.night_shift.ask_schedule);
    assert_eq!(steps.websites_from_config, Some(true));
}

#[test]
fn a_config_that_sets_website_checking_and_night_shift_leaves_onboarding_nothing_to_ask() {
    let core = noon()
        .config("websites = false\n[night_shift]\nstrength = 40\nschedule = \"off\"\n")
        .start();
    report_night_shift(&core, Schedule::Off, 60);

    assert!(!shows_onboarding(&core.handle(Event::Launched)));
}

#[test]
fn a_config_that_sets_only_website_checking_still_leaves_night_shift_to_ask() {
    let core = noon().config("websites = false\n").start();
    report_night_shift(&core, Schedule::Off, 60);

    assert!(shows_onboarding(&core.handle(Event::Launched)));
}

#[test]
fn a_config_that_sets_website_checking_leaves_onboarding_nothing_to_ask_without_night_shift() {
    let core = noon().config("websites = false\n").start();

    assert!(
        !core
            .handle(Event::Launched)
            .iter()
            .any(|effect| matches!(effect, Effect::ShowOnboarding { .. }))
    );
}

#[test]
fn a_self_managed_release_still_asks_about_opening_at_login_when_the_config_sets_website_checking()
{
    let core = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .config("websites = false\n")
        .start();

    assert!(
        core.handle(Event::Launched)
            .contains(&Effect::ShowOnboarding { completed: 0 })
    );
}

#[test]
fn the_status_reports_the_last_permission_change() {
    let core = noon().start();

    core.handle(Event::PermissionsChanged {
        accessibility: true,
        notifications: NotificationPermission::Denied,
    });

    assert_eq!(
        core.status().permissions,
        Permissions {
            accessibility: true,
            notifications: NotificationPermission::Denied,
        }
    );
}

#[test]
fn a_self_managed_release_starts_its_updater_at_launch_and_registers_nothing() {
    let core = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .start();

    assert_eq!(launch(&core), vec![Effect::StartUpdater]);
}

#[test]
fn a_managed_install_leaves_starting_at_login_to_its_installer() {
    let core = noon()
        .installed(Instance::Release, Install::Managed)
        .start();

    assert_eq!(launch(&core), Vec::new());
}

#[test]
fn a_dev_build_never_registers_a_login_item() {
    let core = noon()
        .installed(Instance::Dev, Install::SelfManaged)
        .start();

    assert_eq!(launch(&core), Vec::new());
}

#[test]
fn saved_ad_hoc_rules_load_back() {
    let store = adhoc::Store::new(in_memory());
    let rules = vec![
        (figma(), OnOff::Off),
        (
            RuleIdentity::Domain("youtube.com".parse().unwrap()),
            OnOff::On,
        ),
    ];

    store.save(&rules).unwrap();

    assert_eq!(store.load().unwrap(), rules);
}

#[test]
fn ad_hoc_rules_from_a_newer_build_are_refused() {
    let files = holding(
        File::Rules,
        "version = 3\n[rules.off]\napp = [\"com.figma.Desktop\"]\n",
    );

    assert!(matches!(
        adhoc::Store::new(files).load(),
        Err(adhoc::AdHocError::Newer { version: 3, .. })
    ));
}

#[test]
fn ad_hoc_rules_with_keys_this_build_doesnt_know_still_load() {
    let files = holding(
        File::Rules,
        "version = 1\nlabel = \"x\"\n[rules]\napp = [\"com.figma.Desktop\"]\nkeyword = []\n",
    );

    assert_eq!(
        adhoc::Store::new(files).load().unwrap(),
        vec![(figma(), OnOff::Off)]
    );
}

#[test]
fn saved_state_loads_back() {
    let store = Store::new(in_memory());
    let state = Persisted {
        layers: vec![
            layered(
                figma_app(),
                Some(Layer {
                    state: OnOff::On,
                    until: Some(at("2026-10-17T07:00:00+02:00")),
                    with: Some(Place::Everywhere),
                }),
                None,
            ),
            layered(
                Place::Everywhere,
                Some(Layer {
                    state: OnOff::On,
                    until: Some(at("2026-10-17T07:00:00+02:00")),
                    with: Some(figma_app()),
                }),
                Some(layer(OnOff::Off, Some("2026-10-17T00:00:00+02:00"))),
            ),
        ],
        websites: true,
        onboarding: 1,
        notifications_wanted: true,
        open_at_login: true,
        announced: vec![CheckKind::Accessibility],
        night_shift: Some(OnOff::Off),
        until: Some(at("2026-10-17T07:00:00+02:00")),
        applied_from_config: NightShiftConfig {
            strength: Some(40),
            schedule: Some(custom_hours()),
        },
        schedule_off_accepted: true,
        last_version: Some("1.0.0".to_owned()),
    };

    store.save(&state).unwrap();

    assert_eq!(store.load(), state);
}

#[test]
fn state_from_an_earlier_build_loads_without_its_manual_state_snoozes_or_timed_disable() {
    let files = holding(
        File::State,
        r#"{"version":1,"manual_state":"off","snoozes":["app:com.figma.Desktop"],"timed_disable":"2026-10-16T22:00:00Z","websites":true,"onboarding":1}"#,
    );

    let state = Store::new(files).load();

    assert_eq!(state.layers, Vec::new());
    assert!(state.websites);
    assert_eq!(state.onboarding, 1);
}

#[test]
fn missing_state_starts_fresh() {
    assert_eq!(Store::new(in_memory()).load(), Persisted::default());
}

#[test]
fn state_from_a_newer_build_starts_fresh() {
    let files = holding(File::State, r#"{"version":2,"login_item_registered":true}"#);

    assert_eq!(Store::new(files).load(), Persisted::default());
}

#[test]
fn state_with_keys_this_build_doesnt_know_still_loads() {
    let files = holding(
        File::State,
        r#"{"version":1,"login_item_registered":true,"websites":true}"#,
    );

    assert!(Store::new(files).load().websites);
}

/// The check of `kind` in the status, if the core reports one.
fn check_of(core: &Harness, kind: CheckKind) -> Option<(CheckState, String)> {
    core.status()
        .checks
        .into_iter()
        .find(|check| check.kind == kind)
        .map(|Check { state, text, .. }| (state, text))
}

fn stands(state: CheckState, text: &str) -> Option<(CheckState, String)> {
    Some((state, text.to_owned()))
}

fn permissions(accessibility: bool, notifications: NotificationPermission) -> Event {
    Event::PermissionsChanged {
        accessibility,
        notifications,
    }
}

#[test]
fn missing_accessibility_is_a_problem_only_while_website_checking_is_on() {
    let core = noon().start();
    core.handle(permissions(false, NotificationPermission::Granted));

    assert_eq!(
        check_of(&core, CheckKind::Accessibility),
        stands(CheckState::Off, "Not needed while website checking is off.")
    );
    core.send(Command::Websites(OnOff::On));
    assert_eq!(
        check_of(&core, CheckKind::Accessibility),
        stands(
            CheckState::Drifted,
            "Accessibility isn’t allowed, so website checking can’t read the front tab."
        )
    );
    core.handle(permissions(true, NotificationPermission::Granted));
    assert_eq!(
        check_of(&core, CheckKind::Accessibility),
        stands(CheckState::Ok, "Accessibility is allowed.")
    );
}

#[test]
fn notifications_switched_off_after_being_allowed_are_a_problem() {
    let core = noon().start();
    let notifications = |core: &Harness| check_of(core, CheckKind::Notifications);

    core.handle(permissions(true, NotificationPermission::Denied));
    assert_eq!(
        notifications(&core),
        stands(CheckState::Off, "Notifications are off.")
    );
    core.handle(permissions(true, NotificationPermission::Granted));
    assert_eq!(notifications(&core), stands(CheckState::Ok, "Allowed."));
    core.handle(permissions(true, NotificationPermission::Denied));
    let drifted = stands(
        CheckState::Drifted,
        "Notifications were allowed, but they’re switched off in System Settings.",
    );
    assert_eq!(notifications(&core), drifted);

    let core = core.restart_at("2026-10-16T12:10:00+02:00");
    core.handle(permissions(true, NotificationPermission::Denied));
    assert_eq!(notifications(&core), drifted);
}

#[test]
fn the_config_file_check_says_whether_the_file_is_missing_valid_or_broken() {
    let core = noon().start();
    let config = |core: &Harness| check_of(core, CheckKind::ConfigFile);

    assert_eq!(config(&core), stands(CheckState::Off, "No config file."));
    core.write_config("[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    core.request(Request::Reload);
    assert_eq!(config(&core), stands(CheckState::Ok, "Valid, with 1 rule."));
    core.write_config("[rules.off]\ndomian = [\"figma.com\"]\n");
    core.request(Request::Reload);
    assert_eq!(
        config(&core),
        stands(
            CheckState::Broken,
            "Line 2: unknown field `domian`, expected `app` or `domain`."
        )
    );
    core.write_config("[night_shift]\nstrength = 101\n");
    core.request(Request::Reload);
    assert_eq!(
        config(&core),
        stands(
            CheckState::Broken,
            "night_shift.strength is 0 to 100, not 101."
        )
    );
}

#[test]
fn the_ad_hoc_rules_check_counts_them_or_says_why_they_are_off() {
    let core = noon().start();

    assert_eq!(
        check_of(&core, CheckKind::AdHocRules),
        stands(CheckState::Ok, "No ad-hoc rules.")
    );
    core.send(add_rule(figma()));
    assert_eq!(
        check_of(&core, CheckKind::AdHocRules),
        stands(CheckState::Ok, "1 ad-hoc rule.")
    );

    let core = noon().rules("version = 3\n").start();
    let (state, text) = check_of(&core, CheckKind::AdHocRules).unwrap();
    assert_eq!(state, CheckState::Broken);
    assert!(text.starts_with("Ad-hoc rules are off: "), "{text}");

    let core = noon()
        .rules("version = 2\n[rules.off]\napp = \"com.figma.Desktop\"\n")
        .start();
    assert_eq!(
        check_of(&core, CheckKind::AdHocRules),
        stands(
            CheckState::Broken,
            &format!(
                "Ad-hoc rules are off: {}: line 3: invalid type: string \"com.figma.Desktop\", expected a sequence.",
                in_memory().path(File::Rules).display()
            )
        )
    );
}

#[test]
fn night_shift_missing_on_this_mac_is_broken() {
    let core = noon().start();

    assert_eq!(
        check_of(&core, CheckKind::NightShift),
        stands(
            CheckState::Broken,
            "Night Shift isn’t available on this Mac."
        )
    );
    core.handle(reported(false, Schedule::Off));
    assert_eq!(
        check_of(&core, CheckKind::NightShift),
        stands(CheckState::Ok, "Available, with no schedule.")
    );
    core.handle(reported(false, custom_hours()));
    assert_eq!(
        check_of(&core, CheckKind::NightShift),
        stands(
            CheckState::Ok,
            "Available, following its schedule: 22:00 to 07:00."
        )
    );
}

/// The check of `kind` as Settings shows it.
fn settings_check(core: &Core, kind: CheckKind) -> Option<SettingsCheck> {
    core.settings()
        .checks
        .into_iter()
        .find(|check| check.kind == kind)
}

fn button(title: &str, fix: Fix) -> Option<FixButton> {
    Some(FixButton {
        title: title.to_owned(),
        fix,
    })
}

#[test]
fn settings_offer_to_allow_accessibility_or_stop_checking_websites() {
    let core = noon().config("").start();
    core.handle(Event::CommandChosen {
        command: Command::Websites(OnOff::On),
    });
    core.handle(permissions(false, NotificationPermission::Granted));

    assert_eq!(
        settings_check(&core, CheckKind::Accessibility),
        Some(SettingsCheck {
            kind: CheckKind::Accessibility,
            state: CheckState::Drifted,
            text: "Accessibility isn’t allowed, so website checking can’t read the front tab."
                .to_owned(),
            restore: button("Allow…", Fix::AskForAccessibility),
            accept: button("Stop Checking Websites", Fix::StopCheckingWebsites),
        })
    );
    core.handle(Event::FixChosen {
        fix: Fix::StopCheckingWebsites,
    });
    assert!(!core.settings().website_checking.on);
    assert_eq!(
        settings_check(&core, CheckKind::Accessibility),
        Some(SettingsCheck {
            kind: CheckKind::Accessibility,
            state: CheckState::Off,
            text: "Not needed while website checking is off.".to_owned(),
            restore: None,
            accept: None,
        })
    );
}

#[test]
fn settings_offer_to_allow_notifications_or_keep_them_off() {
    let core = noon().start();
    core.handle(permissions(true, NotificationPermission::Granted));
    core.handle(permissions(true, NotificationPermission::Denied));

    assert_eq!(
        settings_check(&core, CheckKind::Notifications),
        Some(SettingsCheck {
            kind: CheckKind::Notifications,
            state: CheckState::Drifted,
            text: "Notifications were allowed, but they’re switched off in System Settings."
                .to_owned(),
            restore: button("Allow Notifications…", Fix::AllowNotifications),
            accept: button("Keep Notifications Off", Fix::KeepNotificationsOff),
        })
    );
    core.handle(Event::FixChosen {
        fix: Fix::KeepNotificationsOff,
    });
    let kept_off = Some(SettingsCheck {
        kind: CheckKind::Notifications,
        state: CheckState::Off,
        text: "Notifications are off.".to_owned(),
        restore: button("Allow Notifications…", Fix::AllowNotifications),
        accept: None,
    });
    assert_eq!(settings_check(&core, CheckKind::Notifications), kept_off);

    let core = core.restart_at("2026-10-16T12:10:00+02:00");
    core.handle(permissions(true, NotificationPermission::Denied));
    assert_eq!(settings_check(&core, CheckKind::Notifications), kept_off);
}

/// Night Shift as CoreBrightness reports it while macOS has sunset to sunrise set aside.
fn reported_set_aside(enabled: bool, schedule: Schedule) -> Event {
    Event::StatusChanged {
        enabled,
        schedule,
        sun_schedule_set_aside: true,
    }
}

fn night_shift_check(state: CheckState, text: &str) -> Option<SettingsCheck> {
    Some(SettingsCheck {
        kind: CheckKind::NightShift,
        state,
        text: text.to_owned(),
        restore: None,
        accept: None,
    })
}

fn set_aside() -> Option<SettingsCheck> {
    Some(SettingsCheck {
        restore: button("Open Location Services…", Fix::OpenLocationServices),
        accept: button("Keep the Schedule Off", Fix::KeepScheduleOff),
        ..night_shift_check(
            CheckState::Drifted,
            "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on.",
        )
        .unwrap()
    })
}

#[test]
fn sunset_to_sunrise_set_aside_is_a_problem_until_location_services_is_back_on() {
    let core = Scenario::at("2026-10-16T01:40:00+02:00").start();
    report_zurich_sun(&core);
    core.handle(reported(true, Schedule::SunsetToSunrise));

    let effects = core.handle(reported_set_aside(false, Schedule::Off));
    assert!(effects.contains(&Effect::Notify {
        title: "Sunset to sunrise needs Location Services".to_owned(),
        body: "macOS turned Night Shift’s schedule off. Open Nightcap’s settings to turn Location Services back on, or to keep the schedule off.".to_owned(),
        opens: None,
    }));
    assert_eq!(settings_check(&core, CheckKind::NightShift), set_aside());
    assert_eq!(
        core.menu().problem_line,
        Some(ProblemLine {
            title: "1 Thing Needs Attention…".to_owned(),
            subtitle: Some("Night Shift".to_owned()),
        })
    );

    core.handle(reported(true, Schedule::SunsetToSunrise));
    assert_eq!(
        settings_check(&core, CheckKind::NightShift),
        night_shift_check(
            CheckState::Ok,
            "Available, following its schedule: sunset to sunrise."
        )
    );
}

#[test]
fn nightcap_opening_with_sunset_to_sunrise_set_aside_shows_the_problem() {
    let core = Scenario::at("2026-10-16T01:40:00+02:00").start();

    core.handle(reported_set_aside(false, Schedule::Off));

    assert_eq!(settings_check(&core, CheckKind::NightShift), set_aside());
}

#[test]
fn the_schedule_set_aside_can_be_kept_off_until_location_services_is_back_on() {
    let core = Scenario::at("2026-10-16T01:40:00+02:00").start();
    core.handle(reported_set_aside(false, Schedule::Off));

    core.handle(Event::FixChosen {
        fix: Fix::KeepScheduleOff,
    });

    let no_schedule = night_shift_check(CheckState::Ok, "Available, with no schedule.");
    assert_eq!(settings_check(&core, CheckKind::NightShift), no_schedule);
    let core = core.restart_at("2026-10-16T01:45:00+02:00");
    assert!(!notifies(
        &core.handle(reported_set_aside(false, Schedule::Off))
    ));
    assert_eq!(settings_check(&core, CheckKind::NightShift), no_schedule);

    core.handle(reported(true, Schedule::SunsetToSunrise));
    assert!(notifies(
        &core.handle(reported_set_aside(false, Schedule::Off))
    ));
    assert_eq!(settings_check(&core, CheckKind::NightShift), set_aside());
}

#[test]
fn a_schedule_chosen_while_sunset_to_sunrise_is_set_aside_is_no_problem() {
    let core = Scenario::at("2026-10-16T01:40:00+02:00").start();
    core.handle(reported_set_aside(false, Schedule::Off));

    core.handle(reported_set_aside(true, custom_hours()));

    assert_eq!(
        settings_check(&core, CheckKind::NightShift),
        night_shift_check(
            CheckState::Ok,
            "Available, following its schedule: 22:00 to 07:00."
        )
    );
}

#[test]
fn sunset_to_sunrise_without_sun_times_for_ten_minutes_is_a_problem() {
    let core = Scenario::at("2026-10-16T01:40:00+02:00").start();
    core.handle(reported(true, Schedule::SunsetToSunrise));
    let following = night_shift_check(
        CheckState::Ok,
        "Available, following its schedule: sunset to sunrise.",
    );

    core.set_clock("2026-10-16T01:49:00+02:00");
    assert!(!notifies(&core.handle(Event::TimerFired)));
    assert_eq!(settings_check(&core, CheckKind::NightShift), following);

    core.set_clock("2026-10-16T01:50:00+02:00");
    assert!(core.handle(Event::TimerFired).contains(&Effect::Notify {
        title: "Nightcap can’t follow sunset to sunrise".to_owned(),
        body: "macOS hasn’t worked out sunrise and sunset. Custom hours in Night Shift’s settings work without them.".to_owned(),
        opens: None,
    }));
    assert_eq!(
        settings_check(&core, CheckKind::NightShift),
        night_shift_check(
            CheckState::Broken,
            "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. Custom hours work without them.",
        )
    );

    core.set_clock("2026-10-16T01:51:00+02:00");
    report_zurich_sun(&core);
    core.handle(Event::TimerFired);
    assert_eq!(settings_check(&core, CheckKind::NightShift), following);
}

#[test]
fn settings_offer_to_reload_a_broken_config() {
    let core = noon()
        .config("[rules.off]\ndomian = [\"figma.com\"]\n")
        .start();

    let broken = settings_check(&core, CheckKind::ConfigFile).unwrap();
    assert_eq!(broken.state, CheckState::Broken);
    assert_eq!(broken.restore, button("Reload", Fix::Reload));
    core.write_config("[rules.off]\ndomain = [\"figma.com\"]\n");
    core.handle(Event::FixChosen { fix: Fix::Reload });
    assert_eq!(
        settings_check(&core, CheckKind::ConfigFile),
        Some(SettingsCheck {
            kind: CheckKind::ConfigFile,
            state: CheckState::Ok,
            text: "Valid, with 1 rule.".to_owned(),
            restore: None,
            accept: None,
        })
    );
}

fn login_item(item: LoginItem) -> Event {
    Event::LoginItemChanged { item }
}

#[test]
fn settings_switch_opening_at_login_in_a_self_managed_install() {
    let core = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .start();
    let switch = |on, command| {
        Some(SettingsSwitch {
            on,
            locked: false,
            command,
        })
    };
    core.handle(login_item(LoginItem::NotRegistered));

    assert_eq!(
        core.settings().open_at_login,
        switch(false, Command::OpenAtLogin(OnOff::On))
    );
    assert!(
        core.handle(Event::CommandChosen {
            command: Command::OpenAtLogin(OnOff::On),
        })
        .contains(&Effect::RegisterLoginItem)
    );
    core.handle(login_item(LoginItem::Enabled));
    assert_eq!(
        core.settings().open_at_login,
        switch(true, Command::OpenAtLogin(OnOff::Off))
    );
    assert!(
        core.handle(Event::CommandChosen {
            command: Command::OpenAtLogin(OnOff::Off),
        })
        .contains(&Effect::UnregisterLoginItem)
    );
}

#[test]
fn managed_installs_and_dev_builds_have_no_open_at_login_switch() {
    let managed = noon()
        .installed(Instance::Release, Install::Managed)
        .start();
    managed.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(managed.settings().open_at_login, None);

    let dev = noon()
        .installed(Instance::Dev, Install::SelfManaged)
        .start();
    dev.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(dev.settings().open_at_login, None);
    let (_, response) = dev.send(Command::OpenAtLogin(OnOff::On));
    assert_eq!(refusal(response), ErrorCode::NoLoginItem);
}

fn login_check(
    state: CheckState,
    text: &str,
    restore: Option<FixButton>,
    accept: Option<FixButton>,
) -> Option<SettingsCheck> {
    Some(SettingsCheck {
        kind: CheckKind::OpenAtLogin,
        state,
        text: text.to_owned(),
        restore,
        accept,
    })
}

#[test]
fn opening_at_login_that_stops_after_the_user_chose_it_is_a_problem() {
    let core = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .start();
    let off = login_check(
        CheckState::Off,
        "Nightcap doesn’t open at login.",
        None,
        None,
    );
    core.handle(login_item(LoginItem::NotRegistered));

    assert_eq!(settings_check(&core, CheckKind::OpenAtLogin), off);
    core.handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    core.handle(login_item(LoginItem::Enabled));
    assert_eq!(
        settings_check(&core, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Ok,
            "Nightcap opens when you log in.",
            None,
            None
        )
    );
    core.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(
        settings_check(&core, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Drifted,
            "Nightcap no longer opens at login: it was removed in System Settings.",
            button("Open at Login", Fix::OpenAtLogin),
            button("Don’t Open at Login", Fix::DontOpenAtLogin),
        )
    );
    assert!(
        core.handle(Event::FixChosen {
            fix: Fix::OpenAtLogin
        })
        .contains(&Effect::RegisterLoginItem)
    );
    core.handle(Event::FixChosen {
        fix: Fix::DontOpenAtLogin,
    });
    assert_eq!(settings_check(&core, CheckKind::OpenAtLogin), off);

    let core = core.restart();
    core.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(settings_check(&core, CheckKind::OpenAtLogin), off);
}

#[test]
fn opening_at_login_waiting_for_approval_offers_login_items() {
    let core = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .start();
    core.handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    core.handle(login_item(LoginItem::RequiresApproval));

    assert_eq!(
        settings_check(&core, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Drifted,
            "macOS is waiting for your approval in Login Items.",
            button("Open Login Items…", Fix::OpenLoginItems),
            button("Don’t Open at Login", Fix::DontOpenAtLogin),
        )
    );
    assert!(
        core.handle(Event::FixChosen {
            fix: Fix::DontOpenAtLogin
        })
        .contains(&Effect::UnregisterLoginItem)
    );
}

#[test]
fn a_login_item_added_in_system_settings_becomes_the_users_choice() {
    let core = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .start();

    core.handle(login_item(LoginItem::Enabled));
    core.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(
        settings_check(&core, CheckKind::OpenAtLogin).map(|check| check.state),
        Some(CheckState::Drifted)
    );
}

#[test]
fn opening_at_login_removed_in_system_settings_is_announced() {
    let core = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .start();
    core.handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    core.handle(login_item(LoginItem::Enabled));

    assert!(
        core.handle(login_item(LoginItem::NotRegistered))
            .contains(&Effect::Notify {
                title: "Nightcap no longer opens at login".to_owned(),
                body: "Open Nightcap’s settings to turn it back on, or to keep it off.".to_owned(),
                opens: None,
            })
    );
}

#[test]
fn the_rules_tab_lists_every_rule_with_what_can_be_done_about_it() {
    let core = on_by_the_schedule()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n")
        .start();
    let photos = RuleIdentity::App("com.apple.Photos".parse().unwrap());
    core.send(add_rule(photos.clone()));
    core.send(Command::AddRule {
        identity: domain("youtube.com"),
        says: OnOff::On,
    });
    let figma_site = Place::Domain("figma.com".parse().unwrap());
    core.send(set(
        figma_site.clone(),
        OnOff::On,
        Length::UntilScheduleTurns,
    ));
    let action = |title: &str, command| {
        Some(RuleAction {
            title: title.to_owned(),
            command,
        })
    };
    let rule = |identity: RuleIdentity, says, label: &str, snoozed, action| SettingsRule {
        identity,
        says,
        label: label.to_owned(),
        snoozed,
        action,
    };

    assert_eq!(
        core.settings().rules,
        vec![
            rule(figma(), OnOff::Off, "From your config", false, None),
            rule(
                domain("figma.com"),
                OnOff::Off,
                "Snoozed until 07:00",
                true,
                action("Put Back", Command::Reset(figma_site))
            ),
            rule(
                photos.clone(),
                OnOff::Off,
                "Ad-hoc",
                false,
                action("Remove", Command::RemoveRule(photos))
            ),
            rule(
                domain("youtube.com"),
                OnOff::On,
                "Ad-hoc",
                false,
                action("Remove", Command::RemoveRule(domain("youtube.com")))
            ),
        ]
    );
}

#[test]
fn the_rules_tab_lists_what_holds_for_now_where_no_rule_does() {
    let core = on_by_the_schedule()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    let mail_app = Place::from(mail());
    core.send(set(
        mail_app.clone(),
        OnOff::Off,
        Length::UntilScheduleTurns,
    ));
    core.send(set(mail_app.clone(), OnOff::On, minutes(30)));
    // A snooze shows on its rule, and everywhere's own on the Night Shift item.
    core.send(set(figma_app(), OnOff::On, minutes(30)));
    core.send(Command::Off);

    assert_eq!(
        core.settings().for_now,
        vec![SettingsLayers {
            place: mail_app.clone(),
            label: "Night Shift on until 23:30, then off until 22:00".to_owned(),
            action: RuleAction {
                title: "End".to_owned(),
                command: Command::Reset(mail_app),
            },
        }]
    );
}

#[test]
fn the_rules_tab_shows_where_the_config_is_and_copies_the_ad_hoc_rules_for_it() {
    let core = noon().start();
    core.send(add_rule(figma()));
    core.send(add_rule(domain("figma.com")));

    assert_eq!(
        core.settings().config_path,
        etcetera::BaseStrategy::config_dir(&etcetera::base_strategy::Xdg::new().unwrap())
            .join("nightcap/config.toml")
            .display()
            .to_string()
    );
    assert_eq!(
        core.export(ExportFormat::Nix),
        "{ off = { app = [ \"com.figma.Desktop\" ]; domain = [ \"figma.com\" ]; }; on = { app = [ ]; domain = [ ]; }; }\n"
    );
    assert_eq!(
        core.export(ExportFormat::Toml),
        "[rules.off]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n"
    );
}

#[test]
fn a_typed_domain_is_normalized_or_turned_down() {
    assert_eq!(
        normalize_domain("Docs.Figma.COM".to_owned()),
        Some("docs.figma.com".to_owned())
    );
    assert_eq!(normalize_domain("https://figma.com/file".to_owned()), None);
    assert_eq!(normalize_domain("*.figma.com".to_owned()), None);
}

// The tour (ADR-0016).

const SAMPLE: &str = "io.github.choco.nightcap.tour-sample";

fn sample_rule() -> Rule {
    Rule {
        identity: RuleIdentity::App(SAMPLE.parse().unwrap()),
        source: RuleSource::Tour,
        says: OnOff::Off,
    }
}

const SITE: &str = "nightcap.choco.me";

fn tour_sample() -> TourApp {
    TourApp {
        bundle_id: SAMPLE.parse().unwrap(),
        name: "Tour Sample".to_owned(),
    }
}

fn tour_site() -> TourSite {
    TourSite {
        domain: SITE.parse().unwrap(),
        browser: "com.apple.Safari".parse().unwrap(),
    }
}

/// The app the tour's steps give as an example of a rule that says on.
fn mail_app() -> TourApp {
    TourApp {
        bundle_id: "com.apple.mail".parse().unwrap(),
        name: "Mail".to_owned(),
    }
}

fn start_tour(core: &Core) -> Vec<Effect> {
    core.handle(tour_started())
}

/// The tour starts with its sample, its site and Mail as the example.
fn tour_started() -> Event {
    Event::TourStarted {
        sample: tour_sample(),
        site: Some(tour_site()),
        example: mail_app(),
    }
}

fn report(core: &Core, enabled: bool) -> Vec<Effect> {
    core.handle(reported(enabled, custom_hours()))
}

/// At 20:00, with Night Shift off by the custom hours until 22:00.
fn off_by_the_schedule() -> Scenario {
    Scenario::at("2026-10-16T20:00:00+02:00").then(reported(false, custom_hours()))
}

/// At 23:00, with Night Shift on by the custom hours until 07:00.
fn on_by_the_schedule() -> Scenario {
    late().then(reported(true, custom_hours()))
}

/// Night Shift on for the tour, over the custom hours' off until 22:00.
fn on_for_the_tour() -> Decision {
    Decision {
        state: Some(OnOff::On),
        place: Place::Everywhere,
        reasons: vec![
            Reason::Tour,
            Reason::Schedule {
                state: OnOff::Off,
                until: Some(at("2026-10-16T22:00:00+02:00")),
            },
        ],
    }
}

#[test]
fn a_tour_started_with_night_shift_off_by_the_schedule_turns_it_on_for_the_tour() {
    let core = off_by_the_schedule().start();

    assert_eq!(
        start_tour(&core),
        vec![Effect::SetNightShift { enabled: true }]
    );
    assert_eq!(report(&core, true), Vec::new());

    let status = core.status();
    assert_eq!(status.decision, on_for_the_tour());
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn the_tours_end_turns_night_shift_back_off_following_the_schedule() {
    let core = off_by_the_schedule().start();
    start_tour(&core);
    report(&core, true);

    assert_eq!(
        core.handle(Event::TourEnded),
        vec![Effect::SetNightShift { enabled: false }]
    );
    assert_eq!(report(&core, false), Vec::new());

    let status = core.status();
    assert_eq!(
        status.decision.reasons,
        vec![Reason::Schedule {
            state: OnOff::Off,
            until: Some(at("2026-10-16T22:00:00+02:00")),
        }]
    );
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn the_tour_sets_aside_everywheres_hold_and_puts_it_back() {
    let core = on_by_the_schedule().start();
    core.send(Command::Off);
    report(&core, false);

    assert_eq!(
        start_tour(&core),
        vec![Effect::SetNightShift { enabled: true }]
    );
    report(&core, true);
    assert_eq!(
        core.handle(Event::TourEnded),
        vec![Effect::SetNightShift { enabled: false }]
    );

    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            None
        )]
    );
}

#[test]
fn the_tour_sets_aside_everywheres_timer_and_puts_it_back_with_its_end() {
    let core = on_by_the_schedule().start();
    core.send(set(Place::Everywhere, OnOff::Off, minutes(60)));
    report(&core, false);

    assert_eq!(
        start_tour(&core),
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    report(&core, true);
    assert_eq!(
        core.handle(Event::TourEnded),
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T00:00:00+02:00").into()
            },
        ]
    );

    assert_eq!(
        core.status().decision.reasons[0],
        Reason::Timer(layer(OnOff::Off, Some("2026-10-17T00:00:00+02:00")))
    );
}

#[test]
fn while_the_tour_stands_in_everywheres_row_offers_no_way_back_to_the_schedule() {
    let core = off_by_the_schedule().start();
    start_tour(&core);
    report(&core, true);

    core.send(set(
        Place::Everywhere,
        OnOff::On,
        Length::UntilScheduleTurns,
    ));
    core.send(set(Place::Everywhere, OnOff::Off, minutes(30)));

    // With nothing else to offer, the row is left out.
    assert!(
        everywhere_row(&core)
            .is_none_or(|row| !titles(&row).contains(&"Back to the Schedule".to_owned()))
    );
}

#[test]
fn while_the_tour_stands_in_a_tap_off_and_on_returns_to_the_tours_on() {
    let core = off_by_the_schedule().start();
    start_tour(&core);
    report(&core, true);

    let (effects, _) = core.send(Command::Toggle);
    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    report(&core, false);
    let (effects, response) = core.send(Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, Response::Ok(Payload::Decision(on_for_the_tour())));
    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn a_tour_started_with_night_shift_on_changes_nothing() {
    let core = on_by_the_schedule().start();

    assert_eq!(start_tour(&core), Vec::new());
    assert!(matches!(
        core.status().decision.reasons[..],
        [Reason::Schedule { .. }]
    ));
    assert_eq!(core.handle(Event::TourEnded), Vec::new());
}

#[test]
fn adding_the_samples_rule_mid_tour_forces_night_shift_off_while_it_is_in_front() {
    let core = on_by_the_schedule().start();
    start_tour(&core);
    assert_eq!(activate(&core, SAMPLE), Vec::new());

    let (effects, response) = core.send(add_rule(sample_rule().identity));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: Place::App(SAMPLE.parse().unwrap()),
            reasons: vec![Reason::Rule(sample_rule())],
        }))
    );
    report(&core, false);
    assert_eq!(
        activate(&core, "com.apple.mail"),
        vec![Effect::SetNightShift { enabled: true }]
    );
}

#[test]
fn outside_a_tour_the_sample_matches_no_rule() {
    let core = on_by_the_schedule().start();
    start_tour(&core);
    core.send(add_rule(sample_rule().identity));
    activate(&core, "com.apple.mail");
    core.handle(Event::TourEnded);

    assert_eq!(activate(&core, SAMPLE), Vec::new());
}

#[test]
fn removing_the_samples_rule_mid_tour_drops_it() {
    let core = on_by_the_schedule().start();
    start_tour(&core);
    activate(&core, SAMPLE);
    core.send(add_rule(sample_rule().identity));
    report(&core, false);

    let (effects, response) = core.send(Command::RemoveRule(sample_rule().identity));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert!(matches!(response, Response::Ok(_)), "{response:?}");
    assert_eq!(core.status().rules, Vec::new());
}

#[test]
fn the_menu_offers_to_add_the_samples_rule_and_then_to_remove_it() {
    let core = on_by_the_schedule().start();
    start_tour(&core);
    core.handle(Event::AppActivated {
        bundle_id: Some(SAMPLE.to_owned()),
        name: Some("Tour Sample".to_owned()),
        is_browser: false,
    });
    let row = app_row(&core).unwrap();
    assert_eq!(row.title, "Turn Off in Tour Sample");
    assert_eq!(
        choice_titled(&row, "Always").command,
        add_rule(sample_rule().identity)
    );

    core.send(add_rule(sample_rule().identity));

    let row = app_row(&core).unwrap();
    assert_eq!(row.title, "Turn On in Tour Sample");
    assert_eq!(row.subtitle.as_deref(), Some("Always off: the tour’s rule"));
    assert_eq!(
        choice_titled(&row, "Remove Your Rule").command,
        Command::RemoveRule(sample_rule().identity)
    );
}

#[test]
fn rules_added_mid_tour_stay_out_of_settings_and_files_and_go_with_their_snoozes() {
    let core = on_by_the_schedule().start();
    start_tour(&core);
    let site = RuleIdentity::Domain("choco.me".parse().unwrap());
    for identity in [sample_rule().identity, site.clone()] {
        let (_, response) = core.send(add_rule(identity));
        assert!(matches!(response, Response::Ok(_)), "{response:?}");
    }
    let (_, response) = core.send(set(
        Place::App(SAMPLE.parse().unwrap()),
        OnOff::On,
        Length::UntilScheduleTurns,
    ));
    assert!(matches!(response, Response::Ok(_)), "{response:?}");

    assert_eq!(
        core.status().rules,
        vec![
            sample_rule(),
            Rule {
                identity: site,
                source: RuleSource::Tour,
                says: OnOff::Off,
            },
        ]
    );
    assert!(core.settings().rules.is_empty());
    assert!(!core.export(ExportFormat::Toml).contains(SAMPLE));
    let kept = core.file(File::Rules).unwrap_or_default();
    assert!(
        !kept.contains(SAMPLE) && !kept.contains("choco.me"),
        "{kept}"
    );
    let saved = core.file(File::State).unwrap();
    assert!(!saved.contains(SAMPLE), "{saved}");

    core.handle(Event::TourEnded);
    let status = core.status();
    assert_eq!(status.rules, Vec::new());
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn adding_a_rule_that_exists_mid_tour_leaves_it_as_it_is() {
    let core = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    report(&core, true);
    start_tour(&core);

    core.send(add_rule(figma()));

    assert_eq!(core.status().rules, vec![app_rule("com.figma.Desktop")]);
    core.handle(Event::TourEnded);
    assert_eq!(core.status().rules, vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn adding_the_rule_a_snoozed_app_has_mid_tour_ends_the_snooze_only_while_it_runs() {
    let core = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    report(&core, true);
    core.send(set(figma_app(), OnOff::On, minutes(30)));
    start_tour(&core);

    core.send(add_rule(figma()));
    assert_eq!(core.status().layers, Vec::new());
    core.handle(Event::TourEnded);

    assert_eq!(
        core.status().layers,
        vec![layered(
            figma_app(),
            None,
            Some(layer(OnOff::On, Some("2026-10-16T23:30:00+02:00")))
        )]
    );
}

#[test]
fn a_config_rule_reloaded_mid_tour_replaces_the_tours_rule_for_its_app() {
    let core = on_by_the_schedule().start();
    start_tour(&core);
    core.send(add_rule(sample_rule().identity));
    core.write_config(&format!("[rules.on]\napp = [\"{SAMPLE}\"]\n"));

    core.request(Request::Reload);

    let config_rule = Rule {
        source: RuleSource::Config,
        says: OnOff::On,
        ..sample_rule()
    };
    assert_eq!(core.status().rules, vec![config_rule]);
    let (_, response) = core.send(Command::RemoveRule(sample_rule().identity));
    assert_eq!(refusal(response), ErrorCode::ConfigRule);
}

#[test]
fn removing_your_own_rule_mid_tour_lasts_only_while_it_runs() {
    let core = on_by_the_schedule().start();
    let mail = RuleIdentity::App("com.apple.mail".parse().unwrap());
    core.send(add_rule(mail.clone()));
    start_tour(&core);

    core.send(Command::RemoveRule(mail.clone()));
    assert_eq!(core.status().rules, Vec::new());
    assert_eq!(kept_ad_hoc(&core), vec![(mail.clone(), OnOff::Off)]);
    core.handle(Event::TourEnded);

    assert_eq!(core.status().rules, vec![ad_hoc(mail)]);
}

#[test]
fn turning_your_own_rule_the_other_way_mid_tour_lasts_only_while_it_runs() {
    let core = on_by_the_schedule().start();
    let mail = RuleIdentity::App("com.apple.mail".parse().unwrap());
    core.send(add_rule(mail.clone()));
    start_tour(&core);

    core.send(Command::AddRule {
        identity: mail.clone(),
        says: OnOff::On,
    });
    assert_eq!(
        core.status().rules,
        vec![Rule {
            says: OnOff::On,
            ..ad_hoc(mail.clone())
        }]
    );
    assert!(core.export(ExportFormat::Toml).contains("[rules.off]"));
    core.handle(Event::TourEnded);

    assert_eq!(core.status().rules, vec![ad_hoc(mail)]);
}

#[test]
fn snoozes_and_the_switch_mid_tour_last_only_while_it_runs() {
    let core = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .start();
    report(&core, true);
    start_tour(&core);

    let figma = Place::App("com.figma.Desktop".parse().unwrap());
    core.send(set(figma, OnOff::On, minutes(30)));
    core.send(Command::Off);
    assert_eq!(core.status().layers.len(), 2);
    core.handle(Event::TourEnded);

    assert_eq!(core.status().layers, Vec::new());
}

#[test]
fn a_restart_after_a_crash_mid_tour_puts_night_shift_back_where_the_tour_found_it() {
    let crashed = on_by_the_schedule().start();
    crashed.send(Command::Off);
    report(&crashed, false);
    start_tour(&crashed);
    report(&crashed, true);

    let core = crashed.restart_at("2026-10-16T23:10:00+02:00");
    let effects = report(&core, true);

    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            None
        )]
    );
}

#[test]
fn quitting_mid_tour_ends_it() {
    let core = off_by_the_schedule().start();
    start_tour(&core);
    report(&core, true);

    assert_eq!(
        core.handle(Event::Terminating),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn starting_the_tour_again_keeps_what_the_first_start_set_aside() {
    let core = on_by_the_schedule().start();
    core.send(Command::Off);
    report(&core, false);
    start_tour(&core);
    report(&core, true);

    assert_eq!(start_tour(&core), Vec::new());
    core.handle(Event::TourEnded);

    assert_eq!(
        core.status().layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            None
        )]
    );
}

// What the tour's steps follow and the titles they quote (ADR-0016).

/// The tour's facts, whose switch is always the Night Shift item's.
fn tour_facts(core: &Core) -> TourFacts {
    let facts = core.tour().expect("a tour runs").facts;
    assert_eq!(facts.everywhere_on, core.menu().night_shift.on);
    facts
}

fn away() -> TourPlace {
    TourPlace {
        in_front: false,
        rule: None,
        snoozed: false,
    }
}

fn bring(core: &Core, app: &TourApp) -> Vec<Effect> {
    core.handle(Event::AppActivated {
        bundle_id: Some(app.bundle_id.to_string()),
        name: Some(app.name.clone()),
        is_browser: false,
    })
}

fn choose(core: &Core, command: Command) -> Vec<Effect> {
    core.handle(Event::CommandChosen { command })
}

/// The row's item that sends a command `wanted` picks out, which it must have.
fn item_sending(row: &PlaceRow, wanted: impl Fn(&Command) -> bool) -> MenuItem {
    row.items
        .iter()
        .find_map(|item| match item {
            RowItem::Choice { item } if wanted(&item.command) => Some(item.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no such item in {:?}", titles(row)))
}

fn snoozes(command: &Command) -> bool {
    matches!(
        command,
        Command::Set {
            length: Length::For(_),
            ..
        }
    )
}

fn sample_place() -> Place {
    Place::App(SAMPLE.parse().unwrap())
}

#[test]
fn the_tour_reports_nothing_before_it_starts_or_once_it_ends() {
    let core = on_by_the_schedule().start();
    assert_eq!(core.tour(), None);

    start_tour(&core);
    assert!(core.tour().is_some());
    core.handle(Event::TourEnded);
    assert_eq!(core.tour(), None);

    start_tour(&core);
    core.handle(Event::Terminating);
    assert_eq!(core.tour(), None);
}

#[test]
fn the_samples_facts_follow_it_to_the_front_into_its_rule_and_away() {
    let core = on_by_the_schedule().start();
    start_tour(&core);
    assert_eq!(tour_facts(&core).sample, away());

    bring(&core, &tour_sample());
    assert_eq!(
        tour_facts(&core).sample,
        TourPlace {
            in_front: true,
            ..away()
        }
    );
    let always = choice_titled(&app_row(&core).unwrap(), "Always");
    choose(&core, always.command);
    assert_eq!(
        tour_facts(&core).sample,
        TourPlace {
            in_front: true,
            rule: Some(OnOff::Off),
            snoozed: false,
        }
    );
    bring(&core, &mail_app());

    assert_eq!(
        tour_facts(&core).sample,
        TourPlace {
            rule: Some(OnOff::Off),
            ..away()
        }
    );
}

#[test]
fn a_snooze_from_the_samples_row_and_putting_the_rule_back_show_in_its_facts() {
    let core = on_by_the_schedule().start();
    start_tour(&core);
    bring(&core, &tour_sample());
    choose(&core, add_rule(sample_rule().identity));

    let snooze = item_sending(&app_row(&core).unwrap(), snoozes);
    choose(&core, snooze.command);
    let snoozed = TourPlace {
        in_front: true,
        rule: Some(OnOff::Off),
        snoozed: true,
    };
    assert_eq!(tour_facts(&core).sample, snoozed);
    let back = item_sending(&app_row(&core).unwrap(), |command| {
        *command == Command::Reset(sample_place())
    });
    choose(&core, back.command);

    assert_eq!(
        tour_facts(&core).sample,
        TourPlace {
            snoozed: false,
            ..snoozed
        }
    );
}

#[test]
fn a_snooze_that_ends_while_the_sample_is_in_the_background_shows_in_its_facts() {
    let core = on_by_the_schedule().start();
    start_tour(&core);
    bring(&core, &tour_sample());
    choose(&core, add_rule(sample_rule().identity));
    choose(&core, set(sample_place(), OnOff::On, minutes(30)));
    bring(&core, &mail_app());
    assert!(tour_facts(&core).sample.snoozed);

    core.set_clock("2026-10-16T23:30:00+02:00");
    core.handle(Event::TimerFired);

    assert_eq!(
        tour_facts(&core).sample,
        TourPlace {
            rule: Some(OnOff::Off),
            ..away()
        }
    );
}

#[test]
fn a_rule_and_a_snooze_for_the_sample_from_the_cli_show_in_its_facts() {
    let core = on_by_the_schedule().start();
    start_tour(&core);

    core.send(add_rule(sample_rule().identity));
    assert_eq!(
        tour_facts(&core).sample,
        TourPlace {
            rule: Some(OnOff::Off),
            ..away()
        }
    );
    core.send(set(sample_place(), OnOff::On, Length::UntilScheduleTurns));

    assert_eq!(
        tour_facts(&core).sample,
        TourPlace {
            rule: Some(OnOff::Off),
            snoozed: true,
            ..away()
        }
    );
}

/// At 23:00 with Night Shift on by the custom hours and website checking on, the tour started
/// and Safari in front.
fn tour_in_safari() -> Scenario {
    on_by_the_schedule()
        .config("websites = true\n")
        .then(tour_started())
        .then(activate_browser_event("com.apple.Safari"))
}

#[test]
fn the_site_is_in_front_on_its_host_and_its_www_and_not_on_another_tab() {
    let core = tour_in_safari().start();
    assert_eq!(tour_facts(&core).site, Some(away()));
    let in_front = TourPlace {
        in_front: true,
        ..away()
    };

    report_host(&core, 1, SITE);
    assert_eq!(tour_facts(&core).site, Some(in_front.clone()));
    report_host(&core, 1, "www.nightcap.choco.me");
    assert_eq!(tour_facts(&core).site, Some(in_front));
    report_host(&core, 1, "example.com");
    assert_eq!(tour_facts(&core).site, Some(away()));
    assert_eq!(tour_facts(&core).sample, away());
}

#[test]
fn the_sites_own_rule_shows_in_its_facts_and_a_tap_changes_only_the_switch() {
    let core = tour_in_safari().start();
    report_host(&core, 1, "www.nightcap.choco.me");
    let always = choice_titled(&site_row(&core).unwrap(), "Always");
    assert_eq!(always.command, add_rule(domain(SITE)));
    choose(&core, always.command);
    let ruled = tour_facts(&core);
    assert_eq!(
        ruled.site,
        Some(TourPlace {
            in_front: true,
            rule: Some(OnOff::Off),
            snoozed: false,
        })
    );
    assert!(ruled.everywhere_on);

    choose(&core, Command::Toggle);

    assert_eq!(
        tour_facts(&core),
        TourFacts {
            everywhere_on: false,
            ..ruled
        }
    );
}

#[test]
fn a_tour_without_a_site_step_reports_no_site() {
    let core = on_by_the_schedule().start();

    core.handle(Event::TourStarted {
        sample: tour_sample(),
        site: None,
        example: mail_app(),
    });

    let tour = core.tour().unwrap();
    assert_eq!(tour.facts.site, None);
    assert_eq!(tour.titles.add_site_rule, None);
}

#[test]
fn the_tour_stands_in_as_night_shift_was_when_it_started_whatever_the_switch_does() {
    let core = off_by_the_schedule().start();
    start_tour(&core);
    report(&core, true);
    assert!(tour_facts(&core).stands_in);
    choose(&core, Command::Toggle);
    report(&core, false);
    let tapped = tour_facts(&core);
    assert!(tapped.stands_in);
    assert!(!tapped.everywhere_on);

    let core = on_by_the_schedule().start();
    start_tour(&core);
    assert!(!tour_facts(&core).stands_in);
    choose(&core, Command::Toggle);
    report(&core, false);
    let tapped = tour_facts(&core);
    assert!(!tapped.stands_in);
    assert!(!tapped.everywhere_on);
}

#[test]
fn the_tours_titles_are_the_menus() {
    let path = |row: &str, item: &str| MenuPath {
        row: row.to_owned(),
        item: item.to_owned(),
    };

    assert_eq!(
        tour_titles(tour_sample(), Some(tour_site()), mail_app()),
        TourTitles {
            add_sample_rule: path("Turn Off in Tour Sample", "Always"),
            snooze_sample: "Turn On in Tour Sample".to_owned(),
            put_sample_rule_back: path("Turn Off in Tour Sample", "Put the Rule Back"),
            add_site_rule: Some(path("Turn Off at This Site", "Always")),
            add_example_rule: path("Turn On in Mail", "Always"),
        }
    );
}

#[test]
fn each_title_the_tour_quotes_is_where_the_menu_shows_it_in_the_situation_its_step_describes() {
    let core = off_by_the_schedule().config("websites = true\n").start();
    start_tour(&core);
    report(&core, true);
    let titles = core.tour().unwrap().titles;
    let quoted = |row: &PlaceRow, wanted: &dyn Fn(&Command) -> bool| {
        let item = item_sending(row, wanted);
        (
            MenuPath {
                row: row.title.clone(),
                item: item.title,
            },
            item.command,
        )
    };
    let rule_for = |identity: RuleIdentity| move |command: &Command| matches!(command, Command::AddRule { identity: ruled, .. } if *ruled == identity);

    bring(&core, &tour_sample());
    let (shown, always) = quoted(&app_row(&core).unwrap(), &rule_for(sample_rule().identity));
    assert_eq!(shown, titles.add_sample_rule);
    choose(&core, always);
    let row = app_row(&core).unwrap();
    assert_eq!(row.title, titles.snooze_sample);
    choose(&core, item_sending(&row, snoozes).command);
    let (shown, _) = quoted(&app_row(&core).unwrap(), &|command| {
        *command == Command::Reset(sample_place())
    });
    assert_eq!(shown, titles.put_sample_rule_back);

    activate_browser(&core, "com.apple.Safari");
    report_host(&core, 1, SITE);
    let (shown, _) = quoted(&site_row(&core).unwrap(), &rule_for(domain(SITE)));
    assert_eq!(Some(shown), titles.add_site_rule);

    choose(&core, Command::Off);
    report(&core, false);
    bring(&core, &mail_app());
    let (shown, _) = quoted(&app_row(&core).unwrap(), &rule_for(mail()));
    assert_eq!(shown, titles.add_example_rule);
}
