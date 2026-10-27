#![cfg(debug_assertions)]
//! The switch, and how long a change lasts: holds, timers and their lengths.

mod common;

use nightcap_core::{Effect, Fact};
use nightcap_protocol::{
    Command, Decision, ErrorCode, Length, OnOff, Payload, Place, Reason, Response, Schedule,
};

use common::{
    activate, app_rule, at, custom_hours, figma_app, forced, late, layer, layered, minutes, noon,
    off_by_the_schedule, on_by_the_schedule, refusal, report, reported, send, set, status,
};

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
    let staged = noon().stage().unwrap();

    let (effects, response) = send(&staged, Command::On);

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

#[test]
fn on_turns_night_shift_on_everywhere_until_turned_back() {
    let staged = noon().stage().unwrap();
    staged.report(reported(false, Schedule::Off));

    let (effects, response) = send(&staged, Command::On);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, held(OnOff::On));
}

#[test]
fn off_takes_off_everywheres_hold_where_that_is_enough() {
    let staged = noon().stage().unwrap();
    // Found on with no schedule, which the core takes as the user's own on.
    staged.report(reported(true, Schedule::Off));

    let (effects, response) = send(&staged, Command::Off);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(response, off_with_no_schedule());
}

#[test]
fn on_while_everywhere_says_on_writes_nothing() {
    let staged = noon().stage().unwrap();
    staged.report(reported(true, Schedule::Off));

    let (effects, response) = send(&staged, Command::On);

    assert_eq!(effects, Vec::new());
    assert_eq!(response, held(OnOff::On));
}

#[test]
fn toggle_turns_everywhere_the_other_way() {
    let staged = noon().stage().unwrap();
    staged.report(reported(true, Schedule::Off));

    let (effects, response) = send(&staged, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(response, off_with_no_schedule());
}

#[test]
fn two_flips_leave_nothing_behind() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Toggle);
    staged.report(reported(false, custom_hours()));

    let (effects, response) = send(&staged, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn on_while_the_schedule_says_on_holds_nothing() {
    let staged = on_by_the_schedule().stage().unwrap();

    let (effects, response) = send(&staged, Command::On);

    assert_eq!(effects, Vec::new());
    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn a_hold_with_no_end_outlasts_sunrise_and_sunset() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(&staged, Command::On);
    staged.report(reported(true, custom_hours()));

    staged.set_now(at("2026-10-17T07:00:00+02:00"));
    staged.report(Fact::TimerFired);
    // macOS performs its own transition, and Nightcap puts Night Shift back on.
    let effects = staged.report(reported(false, custom_hours())).effects;

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
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
fn a_hold_until_the_schedule_turns_ends_at_its_turn_even_asleep() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(
        &staged,
        set(Place::Everywhere, OnOff::On, Length::UntilScheduleTurns),
    );

    staged.set_now(at("2026-10-17T08:00:00+02:00"));
    let effects = staged.report(Fact::Woke).effects;

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T22:00:00+02:00").into()
        }]
    );
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn a_timer_gives_back_what_was_under_it() {
    let staged = on_by_the_schedule().stage().unwrap();

    let (effects, _) = send(&staged, set(Place::Everywhere, OnOff::Off, minutes(30)));
    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-16T23:30:00+02:00").into()
            },
        ]
    );
    staged.report(reported(false, custom_hours()));

    staged.set_now(at("2026-10-16T23:30:00+02:00"));
    assert_eq!(
        staged.report(Fact::TimerFired).effects,
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn a_new_timer_replaces_the_places_timer_and_keeps_its_hold() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    send(&staged, set(Place::Everywhere, OnOff::On, minutes(30)));

    send(&staged, set(Place::Everywhere, OnOff::On, minutes(120)));

    assert_eq!(
        status(&staged).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            Some(layer(OnOff::On, Some("2026-10-17T01:00:00+02:00")))
        )]
    );
}

#[test]
fn until_the_schedule_turns_ends_at_sunrise_for_on_and_at_sunset_for_off() {
    let staged = off_by_the_schedule().stage().unwrap();

    send(
        &staged,
        set(Place::Everywhere, OnOff::On, Length::UntilScheduleTurns),
    );
    assert_eq!(
        status(&staged).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );

    staged.set_now(at("2026-10-16T23:00:00+02:00"));
    send(
        &staged,
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
    );
    assert_eq!(
        status(&staged).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, Some("2026-10-17T22:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn a_new_hold_replaces_the_places_hold_and_ends_its_timer() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    send(&staged, set(Place::Everywhere, OnOff::On, minutes(30)));

    send(
        &staged,
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
    );

    assert_eq!(
        status(&staged).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, Some("2026-10-17T22:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn until_a_picked_time_is_a_timer_that_ends_then() {
    let staged = on_by_the_schedule().stage().unwrap();

    let (effects, _) = send(
        &staged,
        set(
            Place::Everywhere,
            OnOff::Off,
            Length::Until(at("2026-10-17T00:15:00+02:00")),
        ),
    );

    assert!(effects.contains(&Effect::ScheduleWakeup {
        at: at("2026-10-17T00:15:00+02:00").into()
    }));
    assert_eq!(
        status(&staged).layers,
        vec![layered(
            Place::Everywhere,
            None,
            Some(layer(OnOff::Off, Some("2026-10-17T00:15:00+02:00")))
        )]
    );
}

#[test]
fn lengths_that_cant_be_are_refused() {
    let staged = late().stage().unwrap();
    staged.report(reported(true, Schedule::Off));

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
        let (effects, response) = send(&staged, command.clone());
        assert_eq!(effects, Vec::new(), "{command:?}");
        assert_eq!(refusal(response), ErrorCode::BadRequest, "{command:?}");
    }
}

#[test]
fn in_front_of_a_rule_the_switch_changes_everywhere_and_the_rule_keeps_its_say() {
    let staged = forced();

    let (effects, response) = send(&staged, Command::Off);

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
        status(&staged).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            None
        )]
    );
    assert_eq!(activate(&staged, "com.apple.Terminal"), Vec::new());
}

#[test]
fn the_switch_first_takes_off_everywheres_layers_that_say_otherwise() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, set(Place::Everywhere, OnOff::Off, minutes(60)));

    let (_, response) = send(&staged, Command::On);

    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn turning_night_shift_off_everywhere_leaves_a_snoozed_rule_warm() {
    let staged = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    report(&staged, true);
    staged.report(Fact::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });
    let figma = Place::App("com.figma.Desktop".parse().unwrap());
    send(&staged, set(figma.clone(), OnOff::On, minutes(30)));
    report(&staged, true);

    let (effects, response) = send(&staged, Command::Off);

    assert_eq!(effects, Vec::new());
    let Response::Ok(Payload::Decision(decision)) = response else {
        panic!("{response:?}");
    };
    assert_eq!((decision.state, decision.place), (Some(OnOff::On), figma));
}
