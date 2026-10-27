#![cfg(debug_assertions)]
//! Onboarding: when it opens, and what it asks.

mod common;

use nightcap_core::{Effect, Event, OnboardingNightShift, OnboardingSteps};
use nightcap_protocol::{Install, Instance, OnOff, Schedule};

use common::{at, noon, report_night_shift, status};

#[test]
fn onboarding_runs_at_the_first_launch_and_not_again() {
    let staged = noon().stage().unwrap();

    assert!(
        staged
            .core()
            .handle(Event::Launched)
            .contains(&Effect::ShowOnboarding { completed: 0 })
    );
    staged
        .core()
        .handle(Event::OnboardingFinished { websites: true });
    assert_eq!(status(&staged).website_checking.state, OnOff::On);

    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    assert!(
        !staged
            .core()
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
    let staged = noon().stage().unwrap();
    report_night_shift(&staged, Schedule::Off, 60);

    assert_eq!(
        staged.core().onboarding(),
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
    let staged = noon().stage().unwrap();
    report_night_shift(&staged, Schedule::SunsetToSunrise, 60);

    assert!(staged.core().onboarding().night_shift.already_set_up);
}

#[test]
fn onboarding_leaves_out_what_the_config_sets() {
    let staged = noon()
        .config("websites = true\n[night_shift]\nstrength = 40\n")
        .stage()
        .unwrap();
    report_night_shift(&staged, Schedule::Off, 60);

    let steps = staged.core().onboarding();

    assert!(!steps.night_shift.ask_strength);
    assert!(steps.night_shift.ask_schedule);
    assert_eq!(steps.websites_from_config, Some(true));
}

#[test]
fn a_config_that_sets_website_checking_and_night_shift_leaves_onboarding_nothing_to_ask() {
    let staged = noon()
        .config("websites = false\n[night_shift]\nstrength = 40\nschedule = \"off\"\n")
        .stage()
        .unwrap();
    report_night_shift(&staged, Schedule::Off, 60);

    assert!(!shows_onboarding(&staged.core().handle(Event::Launched)));
}

#[test]
fn a_config_that_sets_only_website_checking_still_leaves_night_shift_to_ask() {
    let staged = noon().config("websites = false\n").stage().unwrap();
    report_night_shift(&staged, Schedule::Off, 60);

    assert!(shows_onboarding(&staged.core().handle(Event::Launched)));
}

#[test]
fn a_config_that_sets_website_checking_leaves_onboarding_nothing_to_ask_without_night_shift() {
    let staged = noon().config("websites = false\n").stage().unwrap();

    assert!(
        !staged
            .core()
            .handle(Event::Launched)
            .iter()
            .any(|effect| matches!(effect, Effect::ShowOnboarding { .. }))
    );
}

#[test]
fn a_self_managed_release_still_asks_about_opening_at_login_when_the_config_sets_website_checking()
{
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .config("websites = false\n")
        .stage()
        .unwrap();

    assert!(
        staged
            .core()
            .handle(Event::Launched)
            .contains(&Effect::ShowOnboarding { completed: 0 })
    );
}
