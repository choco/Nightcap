#![cfg(debug_assertions)]
//! Onboarding is the app's: the core opens none, and answers the onboarding version
//! `state.json` kept up to 1.2, for the app to take over.

mod common;

use nightcap_core::{Effect, File};
use nightcap_protocol::{Command, Install, Instance, OnOff, Schedule};

use common::{at, launch, noon, report_night_shift, send};

/// `state.json` as 1.2 left it for a user who went through onboarding.
const ONBOARDED: &str = r#"{"version":2,"websites":true,"onboarding":1}"#;

#[test]
fn the_onboarding_version_state_json_kept_is_answered_while_the_core_rewrites_it() {
    let staged = noon().state(ONBOARDED).stage().unwrap();
    assert_eq!(staged.core().snapshot().about().legacy_onboarding, 1);

    send(&staged, Command::Websites(OnOff::Off));

    let saved = staged.file(File::State).unwrap();
    assert!(saved.contains("\"websites\": false"), "{saved}");
    assert!(!saved.contains("onboarding"), "{saved}");
    assert_eq!(staged.core().snapshot().about().legacy_onboarding, 1);
    let restarted = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    assert_eq!(restarted.core().snapshot().about().legacy_onboarding, 0);
}

#[test]
fn a_first_run_answers_no_onboarding_version() {
    let staged = noon().stage().unwrap();

    assert_eq!(staged.core().snapshot().about().legacy_onboarding, 0);
}

/// Whether onboarding opens is the app's to decide, from the launch's snapshot.
#[test]
fn a_first_launch_opens_no_onboarding_whatever_is_left_to_ask() {
    let managed = noon().stage().unwrap();
    report_night_shift(&managed, Schedule::Off, 60);
    let self_managed = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    report_night_shift(&self_managed, Schedule::Off, 60);

    assert_eq!(launch(&managed).effects, Vec::new());
    assert_eq!(launch(&self_managed).effects, vec![Effect::StartUpdater]);
}
