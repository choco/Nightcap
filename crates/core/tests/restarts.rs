#![cfg(debug_assertions)]
//! Quitting, crashing and starting again: what Nightcap releases and what it keeps (ADR-0008).

mod common;

use nightcap_core::{Effect, Staged};
use nightcap_protocol::{Command, Lasting, OnOff, Place};

use common::{
    accessibility_granted, activate, activate_browser, activate_event, adopted, at, books,
    custom_hours, figma_app, forced, in_books_at_noon, late, launch, minutes, on_by_the_schedule,
    reported, resettable, send, set, until, until_turn, watch, yours,
};

/// Snoozes the Figma rule until 07:00, then turns Night Shift off everywhere for an hour.
fn snooze_figma_and_break(staged: &Staged) {
    send(
        staged,
        set(figma_app(), OnOff::On, Lasting::UntilScheduleTurns),
    );
    send(staged, set(Place::Everywhere, OnOff::Off, minutes(60)));
}

#[test]
fn temporary_rules_survive_a_restart() {
    let staged = forced();
    snooze_figma_and_break(&staged);

    let staged = staged.restart_at(at("2026-10-16T23:30:00+02:00"));

    assert_eq!(
        launch(&staged).effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T00:00:00+02:00").into()
        }]
    );
    assert_eq!(
        resettable(&staged),
        vec![
            yours(
                figma_app(),
                OnOff::On,
                until_turn("2026-10-17T07:00:00+02:00")
            ),
            yours(
                Place::Everywhere,
                OnOff::Off,
                until("2026-10-17T00:00:00+02:00")
            ),
        ]
    );
}

#[test]
fn rules_that_ended_while_nightcap_was_stopped_are_dropped_on_start() {
    let staged = forced();
    snooze_figma_and_break(&staged);

    // Past both the rule's end and the 07:00 schedule transition.
    let staged = staged.restart_at(at("2026-10-17T08:00:00+02:00"));

    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn on_the_way_out_only_everywheres_rules_short_of_one_until_a_time_still_apply() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    staged.report(reported(false, custom_hours()));
    send(&staged, set(Place::Everywhere, OnOff::On, minutes(30)));
    staged.report(reported(true, custom_hours()));

    assert_eq!(
        staged.stop().effects,
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn terminating_releases_every_force() {
    let staged = forced();

    assert_eq!(
        staged.stop().effects,
        vec![Effect::SetNightShift { enabled: true }]
    );
}

#[test]
fn terminating_stops_watching_and_releases_without_waiting_for_the_host() {
    let staged = late()
        .config("websites = true\n[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .then(accessibility_granted())
        .then(reported(true, custom_hours()))
        .then(activate_event("com.figma.Desktop"))
        .then(reported(false, custom_hours()))
        .stage()
        .unwrap();
    assert_eq!(
        activate_browser(&staged, "com.apple.Safari"),
        vec![watch("com.apple.Safari", 1)]
    );

    assert_eq!(
        staged.stop().effects,
        vec![
            Effect::StopWatching,
            Effect::SetNightShift { enabled: true }
        ]
    );
}

#[test]
fn after_a_crash_night_shift_left_off_by_a_force_is_not_an_outside_change() {
    let crashed = forced();

    let staged = crashed.restart_at(at("2026-10-16T23:10:00+02:00"));
    activate(&staged, "com.apple.Terminal");
    let effects = staged.report(reported(false, custom_hours())).effects;

    assert_eq!(
        effects,
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
fn after_a_clean_exit_night_shift_found_off_is_an_outside_change() {
    let stopped = forced();
    stopped.stop();

    let staged = stopped.restart_at(at("2026-10-16T23:10:00+02:00"));
    activate(&staged, "com.apple.Terminal");
    staged.report(reported(false, custom_hours()));

    assert_eq!(
        resettable(&staged),
        vec![adopted(
            Place::Everywhere,
            OnOff::Off,
            Some("2026-10-17T22:00:00+02:00")
        )]
    );
}

#[test]
fn after_a_clean_exit_a_force_still_in_place_comes_back() {
    let stopped = forced();
    stopped.stop();

    let staged = stopped.restart_at(at("2026-10-16T23:10:00+02:00"));
    activate(&staged, "com.figma.Desktop");
    // Night Shift as the release on exit left it.
    let effects = staged.report(reported(true, custom_hours())).effects;

    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_rule_that_says_on_is_released_on_a_clean_exit() {
    let staged = in_books_at_noon().stage().unwrap();
    send(
        &staged,
        Command::AddRule {
            identity: books(),
            says: OnOff::On,
        },
    );
    staged.report(reported(true, custom_hours()));

    assert_eq!(
        staged.stop().effects,
        vec![Effect::SetNightShift { enabled: false }]
    );
}
