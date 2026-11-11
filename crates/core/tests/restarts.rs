#![cfg(debug_assertions)]
//! Quitting, crashing and starting again: what Nightcap releases and what it keeps (ADR-0008).

mod common;

use nightcap_core::{Effect, Scenario, Staged};
use nightcap_protocol::{Command, Lasting, Length, OnOff, Place};

use common::{
    accessibility_granted, activate, activate_browser, activate_event, add_rule, adopted, at,
    books, custom_hours, echo, figma_app, forced, in_books_at_noon, late, launch, mail, minutes,
    on_by_the_schedule, reported, resettable, rules, send, set, switched, until, until_turn, watch,
    written, yours,
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
fn rules_nightcap_never_keeps_are_dropped_from_state_json_on_start() {
    let state = r#"{
        "version": 2,
        "rules": [
            {"place": {"app": "com.figma.Desktop"}, "says": "on", "length": {"until": {"until": "2026-10-16T23:30:00+02:00"}}, "set_by": "config"},
            {"place": {"app": "com.figma.Desktop"}, "says": "on", "length": {"until": {"until": "2026-10-16T23:30:00+02:00"}}, "set_by": "you"},
            {"place": {"app": "com.apple.Notes"}, "says": "off", "length": {"until": {"until": "2026-10-16T23:30:00+02:00"}}, "set_by": "trial"},
            {"place": {"app": "com.apple.mail"}, "says": "off", "length": {"until": {"until": "2026-10-16T23:30:00+02:00"}}, "set_by": "outside_change"},
            {"place": {"app": "com.apple.mail"}, "says": "off", "length": {"until_schedule_turns": {"until": "2026-10-17T07:00:00+02:00"}}, "set_by": "outside_change"},
            {"place": {"domain": "figma.com"}, "says": "off", "length": {"until_schedule_turns": {"until": null}}, "set_by": "you"},
            {"place": {"app": "com.apple.iBooksX"}, "says": "off", "length": "indefinite", "set_by": "you"},
            {"place": "everywhere", "says": "on", "length": "indefinite", "set_by": "config"},
            {"place": "everywhere", "says": "on", "length": "indefinite", "set_by": "outside_change"},
            {"place": "everywhere", "says": "off", "length": "indefinite", "set_by": "you"}
        ]
    }"#;

    let staged = Scenario {
        state: Some(state.to_owned()),
        ..late()
    }
    .stage()
    .unwrap();

    assert_eq!(
        resettable(&staged),
        vec![
            yours(figma_app(), OnOff::On, until("2026-10-16T23:30:00+02:00")),
            adopted(
                Place::from(mail()),
                OnOff::Off,
                Some("2026-10-17T07:00:00+02:00")
            ),
            yours(Place::Everywhere, OnOff::Off, Length::Indefinite),
        ]
    );
    assert_eq!(rules(&staged), Vec::new());
}

#[test]
fn on_the_way_out_only_everywheres_rules_short_of_one_until_a_time_still_apply() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    echo(&staged, false);
    send(&staged, set(Place::Everywhere, OnOff::On, minutes(30)));
    echo(&staged, true);

    assert_eq!(staged.stop().effects, vec![switched(false)]);
}

#[test]
fn terminating_releases_every_force() {
    let staged = forced();

    assert_eq!(staged.stop().effects, vec![switched(true)]);
}

#[test]
fn terminating_stops_watching_and_releases_without_waiting_for_the_host() {
    let staged = late()
        .config("websites = true\n[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .then(accessibility_granted())
        .then(reported(true, custom_hours()))
        .then(activate_event("com.figma.Desktop"))
        .then(written(false, custom_hours(), "2026-10-16T23:00:00+02:00"))
        .stage()
        .unwrap();
    assert_eq!(
        activate_browser(&staged, "com.apple.Safari"),
        vec![watch("com.apple.Safari", 1)]
    );

    assert_eq!(
        staged.stop().effects,
        vec![Effect::StopWatching, switched(true)]
    );
}

#[test]
fn after_a_crash_night_shift_left_off_by_a_force_is_not_an_outside_change() {
    let crashed = forced();

    let staged = crashed.restart_at(at("2026-10-16T23:10:00+02:00"));
    activate(&staged, "com.apple.Terminal");
    // Still off, as the force before the crash wrote it.
    let effects = staged
        .report(written(false, custom_hours(), "2026-10-16T23:00:00+02:00"))
        .effects;

    assert_eq!(
        effects,
        vec![
            switched(true),
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
    staged.report(written(false, custom_hours(), "2026-10-16T23:05:00+02:00"));

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
            switched(false),
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
    send(&staged, add_rule(books(), OnOff::On));
    echo(&staged, true);

    assert_eq!(staged.stop().effects, vec![switched(false)]);
}
