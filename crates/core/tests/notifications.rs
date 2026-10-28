#![cfg(debug_assertions)]
//! What Nightcap announces on its own: problems as they appear.

mod common;

use jiff::SignedDuration;
use nightcap_core::{Event, Fact, Outcome, Scenario, Staged};
use nightcap_protocol::{
    AccessibilityDetail, Check, CheckDetail, CheckState, Command, ConfigFileDetail, FileError,
    NightShiftDetail, NotificationPermission, OnOff, Request, Schedule,
};

use common::{at, launch, noon, permissions, reported, tells};

fn accessibility_notice() -> Event {
    Event::ProblemAppeared {
        check: Check {
            detail: CheckDetail::Accessibility(AccessibilityDetail::NotAllowed),
            state: CheckState::Drifted,
        },
    }
}

#[test]
fn a_problem_that_appears_on_its_own_is_announced_once() {
    let staged = noon().config("websites = true\n").stage().unwrap();

    assert!(
        staged
            .report(permissions(false, NotificationPermission::Granted))
            .events
            .contains(&accessibility_notice())
    );
    assert!(!tells(
        &staged.report(permissions(false, NotificationPermission::Granted))
    ));
    staged.report(permissions(true, NotificationPermission::Granted));
    assert!(
        staged
            .report(permissions(false, NotificationPermission::Granted))
            .events
            .contains(&accessibility_notice())
    );

    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    assert!(!tells(
        &staged.report(permissions(false, NotificationPermission::Granted))
    ));
}

#[test]
fn a_problem_the_user_causes_in_nightcap_is_not_announced() {
    let staged = noon().stage().unwrap();
    staged.report(permissions(false, NotificationPermission::Granted));

    let (outcome, _) = staged.send(Command::Websites(OnOff::On));
    assert!(!tells(&outcome));
    assert!(!tells(
        &staged.report(permissions(false, NotificationPermission::Granted))
    ));
}

#[test]
fn a_problem_that_appears_on_its_own_during_a_command_is_still_announced() {
    let no_sun_times = Event::ProblemAppeared {
        check: Check {
            detail: CheckDetail::NightShift(NightShiftDetail::NoSunTimes {
                waited: SignedDuration::from_mins(10),
            }),
            state: CheckState::Broken,
        },
    };
    let from_the_app: fn(&Staged) -> Outcome = |staged| staged.send(Command::Off).0;
    let from_the_cli: fn(&Staged) -> Outcome =
        |staged| staged.request(Request::Command(Command::Off)).0.unwrap();

    for command in [from_the_app, from_the_cli] {
        let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
            .stage()
            .unwrap();
        staged.report(reported(true, Schedule::SunsetToSunrise));

        staged.set_now(at("2026-10-16T01:50:00+02:00"));
        assert!(!tells(&command(&staged)));
        assert!(
            staged
                .report(Fact::TimerFired)
                .events
                .contains(&no_sun_times)
        );
    }
}

#[test]
fn a_config_broken_at_launch_is_announced() {
    let staged = noon()
        .config("[rules.off]\ndomian = [\"figma.com\"]\n")
        .stage()
        .unwrap();

    let error = FileError::Malformed {
        path: staged.core().snapshot().config().path,
        line: Some(2),
        message: "unknown field `domian`, expected `app` or `domain`".to_owned(),
    };
    assert!(launch(&staged).events.contains(&Event::ProblemAppeared {
        check: Check {
            detail: CheckDetail::ConfigFile(ConfigFileDetail::Error(error)),
            state: CheckState::Broken,
        },
    }));
}
