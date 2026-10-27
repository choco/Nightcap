#![cfg(debug_assertions)]
//! What Nightcap announces on its own: problems as they appear, and what's new after an update.

mod common;

use nightcap_core::{Effect, Event};
use nightcap_protocol::{Command, NotificationPermission, OnOff};

use common::{at, launch, noon, notifies, permissions};

fn accessibility_notice() -> Effect {
    Effect::Notify {
        title: "Website checking needs Accessibility".to_owned(),
        body: "Open Nightcap’s settings to allow it, or to stop checking websites.".to_owned(),
        opens: None,
    }
}

#[test]
fn a_problem_that_appears_on_its_own_is_announced_once() {
    let staged = noon().config("websites = true\n").stage().unwrap();

    assert!(
        staged
            .core()
            .handle(permissions(false, NotificationPermission::Granted))
            .contains(&accessibility_notice())
    );
    assert!(!notifies(
        &staged
            .core()
            .handle(permissions(false, NotificationPermission::Granted))
    ));
    staged
        .core()
        .handle(permissions(true, NotificationPermission::Granted));
    assert!(
        staged
            .core()
            .handle(permissions(false, NotificationPermission::Granted))
            .contains(&accessibility_notice())
    );

    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    assert!(!notifies(
        &staged
            .core()
            .handle(permissions(false, NotificationPermission::Granted))
    ));
}

#[test]
fn a_problem_the_user_causes_in_nightcap_is_not_announced() {
    let staged = noon().stage().unwrap();
    staged
        .core()
        .handle(permissions(false, NotificationPermission::Granted));

    let effects = staged.core().handle(Event::CommandChosen {
        command: Command::Websites(OnOff::On),
    });
    assert!(effects.contains(&Effect::AskForAccessibility));
    assert!(!notifies(&effects));
    assert!(!notifies(
        &staged
            .core()
            .handle(permissions(false, NotificationPermission::Granted))
    ));
}

#[test]
fn a_config_broken_at_launch_is_announced() {
    let staged = noon()
        .config("[rules.off]\ndomian = [\"figma.com\"]\n")
        .stage()
        .unwrap();

    let body = format!(
        "{}: line 2: unknown field `domian`, expected `app` or `domain`",
        staged.core().settings().config_path
    );
    assert!(launch(&staged).iter().any(|effect| matches!(
        effect,
        Effect::Notify { title, body: said, .. }
            if title == "Nightcap’s config is invalid" && *said == body
    )));
}

fn update_notice() -> Effect {
    let version = env!("CARGO_PKG_VERSION");
    Effect::Notify {
        title: format!("Nightcap updated to {version}"),
        body: "Click to see what’s new.".to_owned(),
        opens: Some(format!("https://nightcap.choco.me/changelog/#v{version}")),
    }
}

#[test]
fn the_first_launch_of_a_new_version_says_whats_new_once() {
    // An earlier version saved this after onboarding.
    let staged = noon()
        .state(r#"{"version":1,"onboarding":1,"last_version":"1.0.0"}"#)
        .stage()
        .unwrap();

    assert!(launch(&staged).contains(&update_notice()));
    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    assert!(!launch(&staged).contains(&update_notice()));
}

#[test]
fn a_run_from_before_versions_were_kept_counts_as_an_update() {
    let staged = noon()
        .state(r#"{"version":1,"onboarding":1}"#)
        .stage()
        .unwrap();

    assert!(launch(&staged).contains(&update_notice()));
}

#[test]
fn a_first_run_has_nothing_new_to_tell() {
    let staged = noon().stage().unwrap();

    assert!(!notifies(&launch(&staged)));
}
