#![cfg(debug_assertions)]
//! Outside changes and their adoption, what only looks like one, and Night Shift while another
//! user's session is active (ADR-0004).

#![expect(
    clippy::unwrap_used,
    reason = "clippy.toml lets tests unwrap, but not the helpers beside them"
)]

mod common;

use nightcap_core::{Effect, Event, Fact, Outcome, Scenario, Staged};
use nightcap_protocol::{Command, Lasting, Length, OnOff, Place, Schedule, TimeOfDay};

use common::{
    activate, adopted, at, custom_hours, figma, figma_app, forced, late, minutes, noon,
    off_by_the_schedule, on_by_the_schedule, report, reported, resettable, send, set, until,
    until_turn, yours,
};

#[test]
fn an_outside_change_sets_everywhere_until_the_schedule_turns_it_back() {
    let staged = off_by_the_schedule().stage().unwrap();

    let effects = staged.report(reported(true, custom_hours())).effects;

    assert_eq!(effects, Vec::new());
    assert_eq!(
        resettable(&staged),
        vec![adopted(
            Place::Everywhere,
            OnOff::On,
            Some("2026-10-17T07:00:00+02:00")
        )]
    );
}

#[test]
fn an_outside_change_back_takes_off_what_the_switch_held() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(&staged, Command::On);
    staged.report(reported(true, custom_hours()));

    let effects = staged.report(reported(false, custom_hours())).effects;

    assert_eq!(effects, Vec::new());
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn an_edited_schedule_ends_the_rules_waiting_on_its_turn() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(
        &staged,
        set(Place::Everywhere, OnOff::Off, Lasting::UntilScheduleTurns),
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
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn an_edited_schedule_keeps_rules_until_a_time_and_with_no_end() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    send(&staged, set(Place::Everywhere, OnOff::On, minutes(30)));

    staged.report(reported(true, Schedule::SunsetToSunrise));

    assert_eq!(
        resettable(&staged),
        vec![
            yours(
                Place::Everywhere,
                OnOff::On,
                until("2026-10-16T23:30:00+02:00")
            ),
            yours(Place::Everywhere, OnOff::Off, Length::Indefinite),
        ]
    );
}

#[test]
fn a_change_just_before_a_schedule_transition_is_that_transition() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(
        &staged,
        set(Place::Everywhere, OnOff::Off, Lasting::UntilScheduleTurns),
    );

    staged.set_now(at("2026-10-16T21:59:00+02:00"));
    let effects = staged.report(reported(true, custom_hours())).effects;

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
    assert_eq!(resettable(&staged), Vec::new());
    assert_eq!(staged.core().night_shift().scheduled, Some(OnOff::On));
}

#[test]
fn a_change_against_the_coming_transition_is_still_an_outside_change() {
    let staged = off_by_the_schedule().stage().unwrap();
    send(&staged, Command::On);
    staged.report(reported(true, custom_hours()));

    staged.set_now(at("2026-10-16T21:59:00+02:00"));
    staged.report(reported(false, custom_hours()));

    assert_eq!(staged.core().night_shift().scheduled, Some(OnOff::Off));
    assert_eq!(
        staged.core().night_shift().next_transition(),
        Some(at("2026-10-16T22:00:00+02:00"))
    );
}

#[test]
fn macos_performing_a_transition_just_after_the_core_applied_it_is_not_an_outside_change() {
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
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn after_waking_macos_catching_up_on_a_missed_transition_is_not_an_outside_change() {
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
    assert_eq!(resettable(&staged), Vec::new());
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
        resettable(&staged),
        vec![adopted(
            figma_app(),
            OnOff::On,
            Some("2026-10-17T07:00:00+02:00")
        )]
    );
}

#[test]
fn with_no_schedule_an_outside_change_lasts_until_turned_back() {
    let staged = noon().stage().unwrap();
    staged.report(reported(false, Schedule::Off));

    staged.report(reported(true, Schedule::Off));

    assert_eq!(
        resettable(&staged),
        vec![adopted(Place::Everywhere, OnOff::On, None)]
    );
}

#[test]
fn flipping_back_from_outside_takes_off_both_linked_rules() {
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
    let morning = Some("2026-10-17T07:00:00+02:00");
    assert_eq!(
        resettable(&staged),
        vec![
            adopted(figma_app(), OnOff::On, morning),
            adopted(Place::Everywhere, OnOff::On, morning),
        ]
    );

    staged.report(reported(false, custom_hours()));

    assert_eq!(resettable(&staged), Vec::new());
    assert_eq!(activate(&staged, "com.apple.mail"), Vec::new());
}

#[test]
fn a_rule_set_over_a_linked_one_leaves_its_partner_unlinked() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    report(&staged, false);
    activate(&staged, "com.figma.Desktop");
    report(&staged, true);

    send(
        &staged,
        set(figma_app(), OnOff::Off, Lasting::UntilScheduleTurns),
    );

    let everywheres = adopted(
        Place::Everywhere,
        OnOff::On,
        Some("2026-10-17T07:00:00+02:00"),
    );
    assert_eq!(
        resettable(&staged),
        vec![
            yours(
                figma_app(),
                OnOff::Off,
                until_turn("2026-10-16T22:00:00+02:00")
            ),
            everywheres.clone(),
        ]
    );
    send(&staged, Command::Reset(figma_app()));
    assert_eq!(resettable(&staged), vec![everywheres]);
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
    assert_eq!(resettable(&staged), Vec::new());
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
    // The shell's fresh read: still off, as the rule until a time left it.
    let effects = staged.report(reported(false, custom_hours())).effects;

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(resettable(&staged), Vec::new());
}
