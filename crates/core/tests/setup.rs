#![cfg(debug_assertions)]
//! What the user set up: website checking, the checks and what their fixes do, and where the
//! config is read from. What the app shows for them, and in what words, is its own.

mod common;

use jiff::SignedDuration;
use nightcap_core::{Cause, Effect, Event, Fact, Scenario, Staged, normalize_domain};
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, CheckDetail, CheckKind, CheckState, Command,
    ConfigFileDetail, ExportFormat, FileError, NightShiftDetail, NotSelfManaged,
    NotificationPermission, NotificationsDetail, OnOff, Position, Problem, Refusal, Request,
    Schedule, ValueType, WebsiteChecking,
};

use common::{
    add_rule, at, check, custom_hours, domain, figma, noon, on_by_the_schedule, permissions,
    refusal, report_zurich_sun, reported, send,
};

#[test]
fn website_checking_switches_unless_the_config_sets_it() {
    let staged = on_by_the_schedule().stage().unwrap();
    let checking = |staged: &Staged| staged.core().snapshot().preferences().website_checking;
    let state = |state, from_config| WebsiteChecking { state, from_config };

    assert_eq!(checking(&staged), state(OnOff::Off, false));
    _ = staged.send(Command::Websites(OnOff::On));
    assert_eq!(checking(&staged), state(OnOff::On, false));

    staged.write_config(Some("websites = false\n"));
    staged.request(Request::Command(Command::Reload));
    assert_eq!(checking(&staged), state(OnOff::Off, true));
}

/// What the check of `kind` found and where that leaves it, as the checks query answers it.
fn found(staged: &Staged, kind: CheckKind) -> Option<(CheckDetail, CheckState)> {
    check(staged, kind).map(|check| (check.detail, check.state))
}

fn stands(detail: CheckDetail, state: CheckState) -> Option<(CheckDetail, CheckState)> {
    Some((detail, state))
}

fn accessibility(
    found: AccessibilityDetail,
    state: CheckState,
) -> Option<(CheckDetail, CheckState)> {
    stands(CheckDetail::Accessibility(found), state)
}

fn notifications(
    found: NotificationsDetail,
    state: CheckState,
) -> Option<(CheckDetail, CheckState)> {
    stands(CheckDetail::Notifications(found), state)
}

fn night_shift(found: NightShiftDetail, state: CheckState) -> Option<(CheckDetail, CheckState)> {
    stands(CheckDetail::NightShift(found), state)
}

/// Night Shift working with `schedule`, which is no problem.
fn available(schedule: Schedule) -> Option<(CheckDetail, CheckState)> {
    night_shift(
        NightShiftDetail::Available {
            schedule: Some(schedule),
        },
        CheckState::Ok,
    )
}

/// Sunset to sunrise set aside until Location Services is back on, which the user set up.
fn set_aside() -> Option<(CheckDetail, CheckState)> {
    night_shift(NightShiftDetail::SetAside, CheckState::Drifted)
}

#[test]
fn missing_accessibility_is_a_problem_only_while_website_checking_is_on() {
    let staged = noon().stage().unwrap();
    staged.report(permissions(false, NotificationPermission::Granted));

    assert_eq!(
        found(&staged, CheckKind::Accessibility),
        accessibility(AccessibilityDetail::NotNeeded, CheckState::Off)
    );
    send(&staged, Command::Websites(OnOff::On));
    assert_eq!(
        found(&staged, CheckKind::Accessibility),
        accessibility(AccessibilityDetail::NotAllowed, CheckState::Drifted)
    );
    staged.report(permissions(true, NotificationPermission::Granted));
    assert_eq!(
        found(&staged, CheckKind::Accessibility),
        accessibility(AccessibilityDetail::Allowed, CheckState::Ok)
    );
}

#[test]
fn notifications_switched_off_after_being_allowed_are_a_problem() {
    let staged = noon().stage().unwrap();

    staged.report(permissions(true, NotificationPermission::Denied));
    assert_eq!(
        found(&staged, CheckKind::Notifications),
        notifications(NotificationsDetail::Off, CheckState::Off)
    );
    staged.report(permissions(true, NotificationPermission::Granted));
    assert_eq!(
        found(&staged, CheckKind::Notifications),
        notifications(NotificationsDetail::Allowed, CheckState::Ok)
    );
    staged.report(permissions(true, NotificationPermission::Denied));
    let drifted = notifications(NotificationsDetail::SwitchedOff, CheckState::Drifted);
    assert_eq!(found(&staged, CheckKind::Notifications), drifted);

    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    staged.report(permissions(true, NotificationPermission::Denied));
    assert_eq!(found(&staged, CheckKind::Notifications), drifted);
}

#[test]
fn the_config_file_check_says_whether_the_file_is_missing_valid_or_broken() {
    let staged = noon().stage().unwrap();
    let config = |found: ConfigFileDetail, state| stands(CheckDetail::ConfigFile(found), state);
    let path = staged.core().snapshot().config().path;

    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(ConfigFileDetail::Missing, CheckState::Off)
    );
    staged.write_config(Some("[rules.off]\napp = [\"com.figma.Desktop\"]\n"));
    staged.request(Request::Command(Command::Reload));
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(ConfigFileDetail::Valid { rules: 1 }, CheckState::Ok)
    );
    staged.write_config(Some("[rules.off]\ndomian = [\"figma.com\"]\n"));
    staged.request(Request::Command(Command::Reload));
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(
            ConfigFileDetail::Error(FileError::Invalid {
                path: path.clone(),
                line: 2,
                problem: Problem::UnknownKey {
                    table: vec!["rules".to_owned(), "off".to_owned()],
                    key: "domian".to_owned(),
                    expected: vec!["app".to_owned(), "domain".to_owned()],
                },
            }),
            CheckState::Broken
        )
    );
    staged.write_config(Some("[rules.off\napp = [\"com.figma.Desktop\"]\n"));
    staged.request(Request::Command(Command::Reload));
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(
            ConfigFileDetail::Error(FileError::Malformed {
                path: path.clone(),
                at: Some(Position {
                    line: 1,
                    column: 11
                }),
            }),
            CheckState::Broken
        )
    );
    staged.write_config(Some("[night_shift]\nstrength = 300\n"));
    staged.request(Request::Command(Command::Reload));
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(
            ConfigFileDetail::Error(FileError::Invalid {
                path: path.clone(),
                line: 2,
                problem: Problem::StrengthOutOfRange { strength: 300 },
            }),
            CheckState::Broken
        )
    );
    staged.write_config(Some("websites = \"yes\"\n"));
    staged.request(Request::Command(Command::Reload));
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(
            ConfigFileDetail::Error(FileError::Invalid {
                path: path.clone(),
                line: 1,
                problem: Problem::WrongType {
                    table: Vec::new(),
                    key: "websites".to_owned(),
                    found: ValueType::String,
                    expected: ValueType::Boolean,
                },
            }),
            CheckState::Broken
        )
    );
    staged.write_config(Some("[night_shift]\nstrength = 101\n"));
    staged.request(Request::Command(Command::Reload));
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(
            ConfigFileDetail::Error(FileError::Invalid {
                path,
                line: 2,
                problem: Problem::StrengthOutOfRange { strength: 101 },
            }),
            CheckState::Broken
        )
    );
}

#[test]
fn the_ad_hoc_rules_check_counts_them_or_says_why_they_are_off() {
    let staged = noon().stage().unwrap();
    let ad_hoc = |found: AdHocRulesDetail, state| stands(CheckDetail::AdHocRules(found), state);

    assert_eq!(
        found(&staged, CheckKind::AdHocRules),
        ad_hoc(AdHocRulesDetail::On { rules: 0 }, CheckState::Ok)
    );
    send(&staged, add_rule(figma()));
    assert_eq!(
        found(&staged, CheckKind::AdHocRules),
        ad_hoc(AdHocRulesDetail::On { rules: 1 }, CheckState::Ok)
    );
    let path = etcetera::BaseStrategy::data_dir(&etcetera::base_strategy::Xdg::new().unwrap())
        .join("nightcap/rules.toml")
        .display()
        .to_string();

    let staged = noon().rules("version = 3\n").stage().unwrap();
    assert_eq!(
        found(&staged, CheckKind::AdHocRules),
        ad_hoc(
            AdHocRulesDetail::Off(FileError::Newer {
                path: path.clone(),
                version: 3
            }),
            CheckState::Broken
        )
    );

    let staged = noon()
        .rules("version = 2\n[rules.off]\napp = \"com.figma.Desktop\"\n")
        .stage()
        .unwrap();
    assert_eq!(
        found(&staged, CheckKind::AdHocRules),
        ad_hoc(
            AdHocRulesDetail::Off(FileError::Invalid {
                path: path.clone(),
                line: 3,
                problem: Problem::WrongType {
                    table: vec!["rules".to_owned(), "off".to_owned()],
                    key: "app".to_owned(),
                    found: ValueType::String,
                    expected: ValueType::Strings,
                },
            }),
            CheckState::Broken
        )
    );

    let staged = noon()
        .rules("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    assert_eq!(
        found(&staged, CheckKind::AdHocRules),
        ad_hoc(
            AdHocRulesDetail::Off(FileError::Invalid {
                path,
                line: 1,
                problem: Problem::MissingKey {
                    table: Vec::new(),
                    key: "version".to_owned(),
                },
            }),
            CheckState::Broken
        )
    );
}

#[test]
fn night_shift_missing_on_this_mac_is_broken() {
    let staged = noon().stage().unwrap();

    assert_eq!(
        found(&staged, CheckKind::NightShift),
        night_shift(NightShiftDetail::Unavailable, CheckState::Broken)
    );
    staged.report(reported(false, Schedule::Off));
    assert_eq!(
        found(&staged, CheckKind::NightShift),
        available(Schedule::Off)
    );
    staged.report(reported(false, custom_hours()));
    assert_eq!(
        found(&staged, CheckKind::NightShift),
        available(custom_hours())
    );
}

#[test]
fn stopping_website_checking_settles_missing_accessibility() {
    let staged = noon().config("").stage().unwrap();
    _ = staged.send(Command::Websites(OnOff::On));
    staged.report(permissions(false, NotificationPermission::Granted));

    assert_eq!(
        found(&staged, CheckKind::Accessibility),
        accessibility(AccessibilityDetail::NotAllowed, CheckState::Drifted)
    );
    _ = staged.send(Command::Websites(OnOff::Off));
    assert_eq!(
        staged
            .core()
            .snapshot()
            .preferences()
            .website_checking
            .state,
        OnOff::Off
    );
    assert_eq!(
        found(&staged, CheckKind::Accessibility),
        accessibility(AccessibilityDetail::NotNeeded, CheckState::Off)
    );
}

#[test]
fn a_fix_whose_command_is_refused_comes_back_as_a_refusal() {
    let set_in_config = noon().config("websites = true\n").stage().unwrap();
    set_in_config.report(permissions(false, NotificationPermission::Granted));
    let managed = noon().stage().unwrap();

    let (stopped, _) = set_in_config.send(Command::Websites(OnOff::Off));
    let (not_opened, _) = managed.send(Command::OpenAtLogin(OnOff::Off));

    assert!(matches!(
        stopped.events.as_slice(),
        [Event::Refused {
            command: Command::Websites(OnOff::Off),
            why: Refusal::WebsiteCheckingInConfig,
        }]
    ));
    assert!(matches!(
        not_opened.events.as_slice(),
        [Event::Refused {
            command: Command::OpenAtLogin(OnOff::Off),
            why: Refusal::NoLoginItem(NotSelfManaged::ManagedInstall),
        }]
    ));
}

#[test]
fn keeping_notifications_off_settles_them_switched_off() {
    let staged = noon().stage().unwrap();
    staged.report(permissions(true, NotificationPermission::Granted));
    staged.report(permissions(true, NotificationPermission::Denied));

    assert_eq!(
        found(&staged, CheckKind::Notifications),
        notifications(NotificationsDetail::SwitchedOff, CheckState::Drifted)
    );
    _ = staged.send(Command::KeepNotificationsOff);
    let kept_off = notifications(NotificationsDetail::Off, CheckState::Off);
    assert_eq!(found(&staged, CheckKind::Notifications), kept_off);
    assert!(!staged.core().snapshot().preferences().notifications_wanted);

    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    staged.report(permissions(true, NotificationPermission::Denied));
    assert_eq!(found(&staged, CheckKind::Notifications), kept_off);
}

#[test]
fn notifications_that_arent_switched_off_cant_be_kept_off() {
    let staged = noon().stage().unwrap();
    staged.report(permissions(true, NotificationPermission::Granted));

    let (_, response) = send(&staged, Command::KeepNotificationsOff);

    assert_eq!(
        refusal(response),
        Refusal::NothingToAccept {
            check: CheckKind::Notifications
        }
    );
    assert!(staged.core().snapshot().preferences().notifications_wanted);
    staged.report(permissions(true, NotificationPermission::Denied));
    assert_eq!(
        found(&staged, CheckKind::Notifications),
        notifications(NotificationsDetail::SwitchedOff, CheckState::Drifted)
    );
}

/// Night Shift as CoreBrightness reports it while macOS has sunset to sunrise set aside.
fn reported_set_aside(enabled: bool, schedule: Schedule) -> Fact {
    Fact::StatusChanged {
        enabled,
        schedule,
        sun_schedule_set_aside: true,
    }
}

#[test]
fn sunset_to_sunrise_set_aside_is_a_problem_until_location_services_is_back_on() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    report_zurich_sun(&staged);
    staged.report(reported(true, Schedule::SunsetToSunrise));

    staged.report(reported_set_aside(false, Schedule::Off));
    assert_eq!(found(&staged, CheckKind::NightShift), set_aside());

    staged.report(reported(true, Schedule::SunsetToSunrise));
    assert_eq!(
        found(&staged, CheckKind::NightShift),
        available(Schedule::SunsetToSunrise)
    );
}

#[test]
fn nightcap_opening_with_sunset_to_sunrise_set_aside_shows_the_problem() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();

    staged.report(reported_set_aside(false, Schedule::Off));

    assert_eq!(found(&staged, CheckKind::NightShift), set_aside());
}

#[test]
fn the_schedule_set_aside_can_be_kept_off_until_location_services_is_back_on() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    staged.report(reported_set_aside(false, Schedule::Off));

    _ = staged.send(Command::KeepScheduleOff);

    assert_eq!(
        found(&staged, CheckKind::NightShift),
        available(Schedule::Off)
    );
    let staged = staged.restart_at(at("2026-10-16T01:45:00+02:00"));
    staged.report(reported_set_aside(false, Schedule::Off));
    assert_eq!(
        found(&staged, CheckKind::NightShift),
        available(Schedule::Off)
    );

    staged.report(reported(true, Schedule::SunsetToSunrise));
    staged.report(reported_set_aside(false, Schedule::Off));
    assert_eq!(found(&staged, CheckKind::NightShift), set_aside());
}

#[test]
fn a_schedule_that_isnt_set_aside_cant_be_kept_off() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    report_zurich_sun(&staged);
    staged.report(reported(true, Schedule::SunsetToSunrise));

    let (_, response) = send(&staged, Command::KeepScheduleOff);

    assert_eq!(
        refusal(response),
        Refusal::NothingToAccept {
            check: CheckKind::NightShift
        }
    );
    staged.report(reported_set_aside(false, Schedule::Off));
    assert_eq!(found(&staged, CheckKind::NightShift), set_aside());
}

#[test]
fn a_schedule_chosen_while_sunset_to_sunrise_is_set_aside_is_no_problem() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    staged.report(reported_set_aside(false, Schedule::Off));

    staged.report(reported_set_aside(true, custom_hours()));

    assert_eq!(
        found(&staged, CheckKind::NightShift),
        available(custom_hours())
    );
}

#[test]
fn sunset_to_sunrise_without_sun_times_for_ten_minutes_is_a_problem() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    staged.report(reported(true, Schedule::SunsetToSunrise));
    let following = available(Schedule::SunsetToSunrise);

    staged.set_now(at("2026-10-16T01:49:00+02:00"));
    staged.report(Fact::TimerFired);
    assert_eq!(found(&staged, CheckKind::NightShift), following);

    staged.set_now(at("2026-10-16T01:50:00+02:00"));
    staged.report(Fact::TimerFired);
    assert_eq!(
        found(&staged, CheckKind::NightShift),
        Some((
            CheckDetail::NightShift(NightShiftDetail::NoSunTimes {
                waited: SignedDuration::from_mins(10),
            }),
            CheckState::Broken
        ))
    );

    staged.set_now(at("2026-10-16T01:51:00+02:00"));
    report_zurich_sun(&staged);
    staged.report(Fact::TimerFired);
    assert_eq!(found(&staged, CheckKind::NightShift), following);
}

#[test]
fn nightcap_wakes_as_the_patience_for_sun_times_ends() {
    let staged = Scenario::at(at("2026-10-16T01:40:30+02:00"))
        .stage()
        .unwrap();
    let wakeup = |instant| Effect::ScheduleWakeup {
        at: at(instant).into(),
    };
    staged.report(reported(true, Schedule::SunsetToSunrise));

    staged.set_now(at("2026-10-16T01:50:00+02:00"));
    assert_eq!(
        staged.report(Fact::TimerFired).effects,
        vec![wakeup("2026-10-16T01:50:30+02:00")]
    );
    assert_eq!(
        found(&staged, CheckKind::NightShift),
        available(Schedule::SunsetToSunrise)
    );

    staged.set_now(at("2026-10-16T01:50:30+02:00"));
    let outcome = staged.report(Fact::TimerFired);
    assert_eq!(outcome.cause, Cause::Fact);
    assert_eq!(outcome.effects, vec![wakeup("2026-10-16T01:51:00+02:00")]);
    assert_eq!(
        found(&staged, CheckKind::NightShift),
        Some((
            CheckDetail::NightShift(NightShiftDetail::NoSunTimes {
                waited: SignedDuration::from_mins(10),
            }),
            CheckState::Broken
        ))
    );
}

#[test]
fn reloading_mends_a_broken_config_once_it_is_fixed() {
    let staged = noon()
        .config("[rules.off]\ndomian = [\"figma.com\"]\n")
        .stage()
        .unwrap();

    let broken = check(&staged, CheckKind::ConfigFile).unwrap();
    assert_eq!(broken.state, CheckState::Broken);
    staged.write_config(Some("[rules.off]\ndomain = [\"figma.com\"]\n"));
    _ = staged.send(Command::Reload);
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        stands(
            CheckDetail::ConfigFile(ConfigFileDetail::Valid { rules: 1 }),
            CheckState::Ok
        )
    );
}

#[test]
fn a_refused_reload_from_the_app_is_told_by_the_config_check_alone() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    staged.write_config(Some("[rules.off]\ndomian = [\"figma.com\"]\n"));

    let (outcome, answer) = staged.send(Command::Reload);

    assert_eq!(outcome.events, Vec::new());
    assert!(
        matches!(answer, Err(Refusal::InvalidConfig(_))),
        "{answer:?}"
    );
    assert_eq!(
        check(&staged, CheckKind::ConfigFile).map(|check| check.state),
        Some(CheckState::Broken)
    );
}

#[test]
fn the_config_is_read_from_its_xdg_path_and_the_ad_hoc_rules_export_for_it() {
    let staged = noon().stage().unwrap();
    send(&staged, add_rule(figma()));
    send(&staged, add_rule(domain("figma.com")));

    assert_eq!(
        staged.core().snapshot().config().path,
        etcetera::BaseStrategy::config_dir(&etcetera::base_strategy::Xdg::new().unwrap())
            .join("nightcap/config.toml")
            .display()
            .to_string()
    );
    assert_eq!(
        staged.core().snapshot().export(ExportFormat::Nix),
        "{ off = { app = [ \"com.figma.Desktop\" ]; domain = [ \"figma.com\" ]; }; on = { app = [ ]; domain = [ ]; }; }\n"
    );
    assert_eq!(
        staged.core().snapshot().export(ExportFormat::Toml),
        "[rules.off]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n"
    );
}

#[test]
fn a_typed_domain_is_normalized_or_turned_down() {
    assert_eq!(
        normalize_domain("Docs.Figma.COM".to_owned()),
        Some("docs.figma.com".to_owned())
    );
    assert_eq!(normalize_domain("https://figma.com/file".to_owned()), None);
    assert_eq!(normalize_domain("*.figma.com".to_owned()), None);
}
