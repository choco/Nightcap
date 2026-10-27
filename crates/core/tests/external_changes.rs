#![cfg(debug_assertions)]
//! External changes and their adoption, what only looks like one, and Night Shift while another
//! user's session is active (ADR-0004).

#![expect(
    clippy::unwrap_used,
    reason = "clippy.toml lets tests unwrap, but not the helpers beside them"
)]

mod common;

use nightcap_core::{Effect, Event, Fact, Outcome, Scenario, Staged};
use nightcap_protocol::{Command, Layer, Length, OnOff, Place, Schedule, TimeOfDay};

use common::{
    activate, at, custom_hours, figma, figma_app, forced, late, layer, layered, minutes, noon,
    off_by_the_schedule, on_by_the_schedule, report, reported, send, set, status,
};

#[test]
fn an_external_change_holds_everywhere_until_the_schedule_turns_it_back() {
    let staged = off_by_the_schedule().stage().unwrap();

    let effects = staged.report(reported(true, custom_hours())).effects;

    assert_eq!(effects, Vec::new());
    assert_eq!(
        status(&staged).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn an_external_change_back_takes_off_what_the_switch_held() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(&staged, Command::On);
    staged.report(reported(true, custom_hours()));

    let effects = staged.report(reported(false, custom_hours())).effects;

    assert_eq!(effects, Vec::new());
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn an_edited_schedule_ends_the_holds_waiting_on_it() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(
        &staged,
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
    );
    staged.report(reported(false, custom_hours()));

    // Moving the start to 21:00 makes macOS apply the schedule again, so Night Shift comes on.
    let effects = staged
        .report(reported(
            true,
            Schedule::Custom {
                from: TimeOfDay {
                    hour: 21,
                    minute: 0,
                },
                to: TimeOfDay { hour: 7, minute: 0 },
            },
        ))
        .effects;

    assert_eq!(effects, Vec::new());
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn an_edited_schedule_keeps_timers_and_holds_with_no_end() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    send(&staged, set(Place::Everywhere, OnOff::On, minutes(30)));

    staged.report(reported(true, Schedule::SunsetToSunrise));

    assert_eq!(
        status(&staged).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            Some(layer(OnOff::On, Some("2026-10-16T23:30:00+02:00")))
        )]
    );
}

#[test]
fn a_change_just_before_a_schedule_transition_is_that_transition() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(
        &staged,
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
    );

    staged.set_now(at("2026-10-16T21:59:00+02:00"));
    let effects = staged.report(reported(true, custom_hours())).effects;

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
    let status = status(&staged);
    assert_eq!(status.layers, Vec::new());
    assert_eq!(status.scheduled_state, Some(OnOff::On));
}

#[test]
fn a_change_against_the_coming_transition_is_still_an_external_change() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(&staged, Command::On);
    staged.report(reported(true, custom_hours()));

    staged.set_now(at("2026-10-16T21:59:00+02:00"));
    staged.report(reported(false, custom_hours()));

    let status = status(&staged);
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(
        status.next_transition,
        Some(at("2026-10-16T22:00:00+02:00"))
    );
}

#[test]
fn macos_performing_a_transition_just_after_the_core_applied_it_is_not_an_external_change() {
    let staged = Scenario::at(at("2026-10-16T21:00:00+02:00"))
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    staged.report(reported(false, custom_hours()));
    activate(&staged, "com.figma.Desktop");
    staged.set_now(at("2026-10-16T22:00:00+02:00"));
    staged.report(Fact::TimerFired);

    staged.set_now(at("2026-10-16T22:01:00+02:00"));
    let effects = staged.report(reported(true, custom_hours())).effects;

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn after_waking_macos_catching_up_on_a_missed_transition_is_not_an_external_change() {
    let staged = Scenario::at(at("2026-10-16T21:00:00+02:00"))
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    staged.report(reported(false, custom_hours()));
    activate(&staged, "com.figma.Desktop");
    staged.set_now(at("2026-10-16T23:00:00+02:00"));
    staged.report(Fact::Woke);

    staged.set_now(at("2026-10-16T23:03:00+02:00"));
    let effects = staged.report(reported(true, custom_hours())).effects;

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn an_external_on_in_front_of_a_rule_snoozes_it_until_the_schedule_turns_and_says_so() {
    let staged = forced();

    let outcome = staged.report(reported(true, custom_hours()));

    assert_eq!(
        outcome,
        Outcome {
            effects: Vec::new(),
            events: vec![Event::Adopted {
                place: figma_app(),
                snoozed: Some(figma()),
                state: OnOff::On,
                until: Some(at("2026-10-17T07:00:00+02:00")),
            }],
        }
    );
    assert_eq!(
        status(&staged).layers,
        vec![layered(
            figma_app(),
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn with_no_schedule_an_external_change_holds_until_turned_back() {
    let staged = noon().stage().unwrap();
    staged.report(reported(false, Schedule::Off));

    staged.report(reported(true, Schedule::Off));

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
fn flipping_back_from_outside_takes_off_both_linked_holds() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    staged.report(reported(false, custom_hours()));
    activate(&staged, "com.figma.Desktop");

    let outcome = staged.report(reported(true, custom_hours()));

    assert_eq!(
        outcome,
        Outcome {
            effects: Vec::new(),
            events: vec![Event::Adopted {
                place: figma_app(),
                snoozed: Some(figma()),
                state: OnOff::On,
                until: Some(at("2026-10-17T07:00:00+02:00")),
            }],
        }
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
        status(&staged).layers,
        vec![
            layered(figma_app(), linked(Place::Everywhere), None),
            layered(Place::Everywhere, linked(figma_app()), None),
        ]
    );

    staged.report(reported(false, custom_hours()));

    assert_eq!(status(&staged).layers, Vec::new());
    assert_eq!(activate(&staged, "com.apple.mail"), Vec::new());
}

#[test]
fn a_hold_set_over_a_linked_one_leaves_its_partner_unlinked() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    report(&staged, false);
    activate(&staged, "com.figma.Desktop");
    report(&staged, true);

    send(
        &staged,
        set(figma_app(), OnOff::Off, Length::UntilScheduleTurns),
    );

    assert_eq!(
        status(&staged).layers,
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

/// Starts at 23:00 with Night Shift on by the custom hours, then turns it off for an hour.
fn disabled() -> Staged {
    let staged = late().then(reported(true, custom_hours())).stage().unwrap();
    send(&staged, set(Place::Everywhere, OnOff::Off, minutes(60)));
    staged.report(reported(false, custom_hours()));
    staged
}

#[test]
fn while_another_user_is_active_nightcap_writes_nothing() {
    let staged = disabled();
    staged.report(Fact::SessionChanged { active: false });

    staged.set_now(at("2026-10-17T00:00:00+02:00"));

    assert_eq!(
        staged.report(Fact::TimerFired).effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
}

#[test]
fn status_callbacks_while_another_user_is_active_are_ignored() {
    let staged = forced();
    staged.report(Fact::SessionChanged { active: false });

    let effects = staged.report(reported(true, custom_hours())).effects;

    assert_eq!(effects, Vec::new());
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn returning_to_the_session_puts_night_shift_where_nightcap_wants_it() {
    let staged = disabled();
    staged.report(Fact::SessionChanged { active: false });
    staged.set_now(at("2026-10-17T00:00:00+02:00"));
    staged.report(Fact::TimerFired);

    assert_eq!(
        staged.report(Fact::SessionChanged { active: true }).effects,
        Vec::new()
    );
    // The shell's fresh read: still off, as the timer left it.
    let effects = staged.report(reported(false, custom_hours())).effects;

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(status(&staged).layers, Vec::new());
}
