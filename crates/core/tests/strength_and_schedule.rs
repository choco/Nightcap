#![cfg(debug_assertions)]
//! Night Shift's strength and schedule, which Nightcap writes only on a command or from the config
//! (ADR-0014).

#![expect(
    clippy::unwrap_used,
    reason = "clippy.toml lets tests unwrap, but not the helpers beside them"
)]

mod common;

use nightcap_core::{Effect, Event, Staged};
use nightcap_protocol::{
    Command, ErrorCode, OnOff, Payload, Place, Request, Response, Schedule, TimeOfDay,
};

use common::{
    at, custom_hours, late, launch, layer, layered, noon, refusal, report_night_shift, reported,
    send, status,
};

/// Night Shift reported off with no schedule, at a strength of 60.
fn at_strength_60() -> Staged {
    noon()
        .then(reported(false, Schedule::Off))
        .then(Event::StrengthChanged { value: 60 })
        .stage()
        .unwrap()
}

#[test]
fn strength_sets_it_and_answers_with_the_decision() {
    let staged = at_strength_60();
    let decision = status(&staged).decision;

    let (effects, response) = send(&staged, Command::Strength(40));

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
    let staged = at_strength_60();

    let (effects, _) = send(&staged, Command::PreviewStrength(80));

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
    let staged = noon().stage().unwrap();

    let (effects, response) = send(&staged, Command::Strength(40));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::NightShiftUnavailable);
}

#[test]
fn strength_before_the_shell_could_read_it_is_refused() {
    let staged = noon().then(reported(false, Schedule::Off)).stage().unwrap();

    let (effects, response) = send(&staged, Command::PreviewStrength(40));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::StrengthUnavailable);
}

#[test]
fn a_strength_above_100_is_refused() {
    let staged = at_strength_60();

    let (effects, response) = send(&staged, Command::Strength(101));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::BadRequest);
}

#[test]
fn set_schedule_writes_it_and_answers_with_the_decision() {
    let staged = at_strength_60();
    let decision = status(&staged).decision;

    let (effects, response) = send(&staged, Command::SetSchedule(custom_hours()));

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
    let staged = late().then(reported(true, custom_hours())).stage().unwrap();

    send(&staged, Command::SetSchedule(Schedule::Off));
    // macOS switches Night Shift off along with the schedule.
    let effects = staged.core().handle(reported(false, Schedule::Off));

    assert!(effects.contains(&Effect::SetNightShift { enabled: true }));
    assert_eq!(
        status(&staged).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, None)),
            None
        )]
    );
}

#[test]
fn set_schedule_while_night_shift_is_unavailable_is_refused() {
    let staged = noon().stage().unwrap();

    let (effects, response) = send(&staged, Command::SetSchedule(Schedule::Off));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::NightShiftUnavailable);
}

#[test]
fn custom_hours_that_arent_times_of_day_are_refused() {
    let staged = at_strength_60();
    let hours = Schedule::Custom {
        from: TimeOfDay {
            hour: 24,
            minute: 0,
        },
        to: TimeOfDay { hour: 7, minute: 0 },
    };

    let (effects, response) = send(&staged, Command::SetSchedule(hours));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::BadRequest);
}

#[test]
fn the_configs_night_shift_values_are_applied_at_launch() {
    let staged = noon()
        .config("[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n")
        .stage()
        .unwrap();
    report_night_shift(&staged, Schedule::Off, 60);

    assert_eq!(
        launch(&staged),
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
    let staged = noon()
        .config("[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n")
        .stage()
        .unwrap();
    report_night_shift(&staged, Schedule::Off, 60);
    launch(&staged);

    // The user moved the strength in System Settings, then Nightcap started again.
    let staged = staged.restart_at(at("2026-10-16T12:05:00+02:00"));
    report_night_shift(&staged, Schedule::SunsetToSunrise, 70);

    assert_eq!(launch(&staged), Vec::new());
}

#[test]
fn the_status_shows_the_configs_night_shift_values_beside_the_macs() {
    let staged = noon()
        .config("[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n")
        .stage()
        .unwrap();
    report_night_shift(&staged, Schedule::Off, 60);

    let status = status(&staged);

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
    let staged = noon()
        .config("[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n")
        .stage()
        .unwrap();
    report_night_shift(&staged, Schedule::Off, 60);
    launch(&staged);

    staged.write_config(Some(
        "[night_shift]\nstrength = 50\nschedule = \"sunset-to-sunrise\"\n",
    ));
    let (effects, _) = staged.request(Request::Reload);

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
    let staged = noon()
        .config("[night_shift]\nstrength = 40\n")
        .stage()
        .unwrap();
    report_night_shift(&staged, Schedule::Off, 60);
    launch(&staged);

    staged.write_config(Some(""));
    let (taken_out, _) = staged.request(Request::Reload);
    staged.write_config(Some("[night_shift]\nstrength = 40\n"));
    let (put_back, _) = staged.request(Request::Reload);

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
    let staged = noon().stage().unwrap();
    report_night_shift(&staged, Schedule::Off, 60);

    for night_shift in [
        "strength = 101",
        "schedule = \"dusk\"",
        "schedule = \"22:00-7am\"",
        "schedule = \"24:00-07:00\"",
    ] {
        staged.write_config(Some(format!("[night_shift]\n{night_shift}\n").as_str()));
        let (effects, response) = staged.request(Request::Reload);
        assert_eq!(effects, Vec::new(), "{night_shift}");
        assert_eq!(refusal(response), ErrorCode::InvalidConfig, "{night_shift}");
    }
}
