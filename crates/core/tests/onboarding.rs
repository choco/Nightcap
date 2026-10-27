#![cfg(debug_assertions)]
//! Onboarding: when it opens, and what it asks.

mod common;

use nightcap_core::{Effect, Fact, OnboardingNightShift, OnboardingSteps, UiCommand};
use nightcap_protocol::{Install, Instance, OnOff, Request, Schedule};

use common::{at, noon, report_night_shift, status};

#[test]
fn onboarding_runs_at_the_first_launch_and_not_again() {
    let staged = noon().stage().unwrap();

    assert!(
        staged
            .report(Fact::Launched)
            .effects
            .contains(&Effect::ShowOnboarding { completed: 0 })
    );
    staged.send_ui(UiCommand::FinishOnboarding { websites: true });
    assert_eq!(status(&staged).website_checking.state, OnOff::On);

    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    assert!(
        !staged
            .report(Fact::Launched)
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::ShowOnboarding { .. }))
    );
}

#[test]
fn finishing_onboarding_leaves_website_checking_to_a_config_that_sets_it() {
    let staged = noon().config("websites = false\n").stage().unwrap();

    staged.send_ui(UiCommand::FinishOnboarding { websites: true });

    let checking = status(&staged).website_checking;
    assert_eq!(checking.state, OnOff::Off);
    assert!(checking.from_config);
    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    staged.write_config(None);
    staged.request(Request::Reload);
    assert_eq!(status(&staged).website_checking.state, OnOff::Off);
}

#[test]
fn onboarding_is_finished_once_the_user_goes_through_it_or_the_config_answers_it() {
    let staged = noon().stage().unwrap();
    assert!(!staged.core().onboarding().finished);
    staged.send_ui(UiCommand::FinishOnboarding { websites: false });
    assert!(staged.core().onboarding().finished);

    let answered = noon()
        .config("websites = false\n[night_shift]\nstrength = 40\nschedule = \"off\"\n")
        .stage()
        .unwrap();
    report_night_shift(&answered, Schedule::Off, 60);
    assert!(!answered.core().onboarding().finished);
    answered.report(Fact::Launched);
    assert!(answered.core().onboarding().finished);
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
            finished: false,
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

    assert!(!shows_onboarding(&staged.report(Fact::Launched).effects));
}

#[test]
fn a_config_that_sets_only_website_checking_still_leaves_night_shift_to_ask() {
    let staged = noon().config("websites = false\n").stage().unwrap();
    report_night_shift(&staged, Schedule::Off, 60);

    assert!(shows_onboarding(&staged.report(Fact::Launched).effects));
}

#[test]
fn a_config_that_sets_website_checking_leaves_onboarding_nothing_to_ask_without_night_shift() {
    let staged = noon().config("websites = false\n").stage().unwrap();

    assert!(
        !staged
            .report(Fact::Launched)
            .effects
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
            .report(Fact::Launched)
            .effects
            .contains(&Effect::ShowOnboarding { completed: 0 })
    );
}
