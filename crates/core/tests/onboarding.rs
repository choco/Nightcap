#![cfg(debug_assertions)]
//! Onboarding: when it opens, and what finishing it sets. Which steps it shows, from the
//! queries, is the shell's.

mod common;

use nightcap_core::{Effect, Fact, Staged, UiCommand};
use nightcap_protocol::{Install, Instance, OnOff, Request, Schedule};

use common::{at, noon, report_night_shift};

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
    assert_eq!(
        staged.core().preferences().website_checking.state,
        OnOff::On
    );

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

    let checking = staged.core().preferences().website_checking;
    assert_eq!(checking.state, OnOff::Off);
    assert!(checking.from_config);
    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    staged.write_config(None);
    staged.request(Request::Reload);
    assert_eq!(
        staged.core().preferences().website_checking.state,
        OnOff::Off
    );
}

#[test]
fn onboarding_is_finished_once_the_user_goes_through_it_or_the_config_answers_it() {
    let staged = noon().stage().unwrap();
    let finished = |staged: &Staged| staged.core().preferences().onboarding_finished;
    assert!(!finished(&staged));
    staged.send_ui(UiCommand::FinishOnboarding { websites: false });
    assert!(finished(&staged));
    assert_eq!(staged.core().preferences().onboarding_completed, 1);

    let answered = noon()
        .config("websites = false\n[night_shift]\nstrength = 40\nschedule = \"off\"\n")
        .stage()
        .unwrap();
    report_night_shift(&answered, Schedule::Off, 60);
    assert!(!finished(&answered));
    answered.report(Fact::Launched);
    assert!(finished(&answered));
}

fn shows_onboarding(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::ShowOnboarding { .. }))
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
