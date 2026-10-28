#![cfg(debug_assertions)]
//! The switch, and how long a rule lasts: indefinitely, until the schedule turns or until a time.

mod common;

use nightcap_core::{Effect, Fact};
use nightcap_protocol::{
    Command, Decision, ErrorCode, Lasting, Length, OnOff, Payload, Place, Reason, Response, Rule,
    Schedule,
};

use common::{
    activate, adopted, app_rule, at, custom_hours, figma_app, forced, late, minutes, noon,
    off_by_the_schedule, on_by_the_schedule, refusal, report, reported, resettable, send, set,
    until, until_turn, yours,
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

/// Everywhere set by `rule`, with no end, over no schedule.
fn everywhere_by(rule: Rule) -> Response {
    Response::Ok(Payload::Decision(Decision {
        state: Some(rule.says),
        place: Place::Everywhere,
        reasons: vec![
            Reason::Rule(rule),
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
    assert_eq!(
        response,
        everywhere_by(yours(Place::Everywhere, OnOff::On, Length::Indefinite))
    );
}

#[test]
fn off_takes_off_everywheres_rule_where_that_is_enough() {
    let staged = noon().stage().unwrap();
    // Found on with no schedule, which the core adopts as an outside change.
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
    assert_eq!(
        response,
        everywhere_by(adopted(Place::Everywhere, OnOff::On, None))
    );
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
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn on_while_the_schedule_says_on_sets_nothing() {
    let staged = on_by_the_schedule().stage().unwrap();

    let (effects, response) = send(&staged, Command::On);

    assert_eq!(effects, Vec::new());
    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn an_indefinite_rule_outlasts_sunrise_and_sunset() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(&staged, Command::On);
    staged.report(reported(true, custom_hours()));

    staged.set_now(at("2026-10-17T07:00:00+02:00"));
    staged.report(Fact::TimerFired);
    // macOS performs its own transition, and Nightcap puts Night Shift back on.
    let effects = staged.report(reported(false, custom_hours())).effects;

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        resettable(&staged),
        vec![yours(Place::Everywhere, OnOff::On, Length::Indefinite)]
    );
}

#[test]
fn a_rule_until_the_schedule_turns_ends_at_its_turn_even_asleep() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(
        &staged,
        set(Place::Everywhere, OnOff::On, Lasting::UntilScheduleTurns),
    );

    staged.set_now(at("2026-10-17T08:00:00+02:00"));
    let effects = staged.report(Fact::Woke).effects;

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T22:00:00+02:00").into()
        }]
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_rule_until_a_time_gives_back_what_was_under_it() {
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
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_new_rule_until_a_time_replaces_the_places_and_keeps_its_longer_one() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    send(&staged, set(Place::Everywhere, OnOff::On, minutes(30)));

    send(&staged, set(Place::Everywhere, OnOff::On, minutes(120)));

    assert_eq!(
        resettable(&staged),
        vec![
            yours(
                Place::Everywhere,
                OnOff::On,
                until("2026-10-17T01:00:00+02:00")
            ),
            yours(Place::Everywhere, OnOff::Off, Length::Indefinite),
        ]
    );
}

#[test]
fn until_the_schedule_turns_ends_at_sunrise_for_on_and_at_sunset_for_off() {
    let staged = off_by_the_schedule().stage().unwrap();

    send(
        &staged,
        set(Place::Everywhere, OnOff::On, Lasting::UntilScheduleTurns),
    );
    assert_eq!(
        resettable(&staged),
        vec![yours(
            Place::Everywhere,
            OnOff::On,
            until_turn("2026-10-17T07:00:00+02:00")
        )]
    );

    staged.set_now(at("2026-10-16T23:00:00+02:00"));
    send(
        &staged,
        set(Place::Everywhere, OnOff::Off, Lasting::UntilScheduleTurns),
    );
    assert_eq!(
        resettable(&staged),
        vec![yours(
            Place::Everywhere,
            OnOff::Off,
            until_turn("2026-10-17T22:00:00+02:00")
        )]
    );
}

#[test]
fn until_the_schedule_turns_sits_over_everywheres_indefinite_rule_and_ends_its_shorter_one() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    send(&staged, set(Place::Everywhere, OnOff::On, minutes(30)));

    send(
        &staged,
        set(Place::Everywhere, OnOff::Off, Lasting::UntilScheduleTurns),
    );

    assert_eq!(
        resettable(&staged),
        vec![
            yours(
                Place::Everywhere,
                OnOff::Off,
                until_turn("2026-10-17T22:00:00+02:00")
            ),
            yours(Place::Everywhere, OnOff::Off, Length::Indefinite),
        ]
    );
}

#[test]
fn everywheres_indefinite_rule_comes_back_when_a_temporary_one_over_it_ends() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    staged.report(reported(false, custom_hours()));
    send(
        &staged,
        set(Place::Everywhere, OnOff::On, Lasting::UntilScheduleTurns),
    );
    staged.report(reported(true, custom_hours()));

    assert_eq!(
        resettable(&staged),
        vec![
            yours(
                Place::Everywhere,
                OnOff::On,
                until_turn("2026-10-17T07:00:00+02:00")
            ),
            yours(Place::Everywhere, OnOff::Off, Length::Indefinite),
        ]
    );
    staged.set_now(at("2026-10-17T07:00:00+02:00"));
    staged.report(Fact::TimerFired);
    staged.report(reported(false, custom_hours()));
    assert_eq!(
        resettable(&staged),
        vec![yours(Place::Everywhere, OnOff::Off, Length::Indefinite)]
    );
    staged.set_now(at("2026-10-17T22:00:00+02:00"));
    staged.report(Fact::TimerFired);
    // macOS performs its own transition, and Nightcap puts Night Shift back off.
    let effects = staged.report(reported(true, custom_hours())).effects;
    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
}

#[test]
fn until_a_picked_time_is_a_rule_that_ends_then() {
    let staged = on_by_the_schedule().stage().unwrap();

    let (effects, _) = send(
        &staged,
        set(
            Place::Everywhere,
            OnOff::Off,
            Lasting::Until(at("2026-10-17T00:15:00+02:00")),
        ),
    );

    assert!(effects.contains(&Effect::ScheduleWakeup {
        at: at("2026-10-17T00:15:00+02:00").into()
    }));
    assert_eq!(
        resettable(&staged),
        vec![yours(
            Place::Everywhere,
            OnOff::Off,
            until("2026-10-17T00:15:00+02:00")
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
            Lasting::Until(at("2026-10-16T22:00:00+02:00")),
        ),
        set(Place::Everywhere, OnOff::Off, Lasting::UntilScheduleTurns),
        set(
            Place::App("com.figma.Desktop".parse().unwrap()),
            OnOff::Off,
            Lasting::Indefinite,
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
        resettable(&staged),
        vec![yours(Place::Everywhere, OnOff::Off, Length::Indefinite)]
    );
    assert_eq!(activate(&staged, "com.apple.Terminal"), Vec::new());
}

#[test]
fn the_switch_first_takes_off_everywheres_rules_that_say_otherwise() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, set(Place::Everywhere, OnOff::Off, minutes(60)));

    let (_, response) = send(&staged, Command::On);

    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(resettable(&staged), Vec::new());
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
