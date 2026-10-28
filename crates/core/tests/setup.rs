#![cfg(debug_assertions)]
//! What the user set up: website checking, the checks and what their fixes do, and where the
//! config is read from. What the app shows for them, and in what words, is its own.

mod common;

use jiff::SignedDuration;
use nightcap_core::{AppCommand, Event, Fact, Fix, Scenario, Staged, normalize_domain};
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, Check, CheckDetail, CheckKind, CheckState, Command,
    ConfigFileDetail, ExportFormat, FileError, NightShiftDetail, NotSelfManaged,
    NotificationPermission, NotificationsDetail, OnOff, Problem, Refusal, Request, Schedule,
    WebsiteChecking,
};

use common::{
    add_rule, at, check, custom_hours, domain, figma, noon, on_by_the_schedule, permissions,
    report_zurich_sun, reported, send, tells,
};

#[test]
fn website_checking_switches_unless_the_config_sets_it() {
    let staged = on_by_the_schedule().stage().unwrap();
    let checking = |staged: &Staged| staged.core().preferences().website_checking;
    let state = |state, from_config| WebsiteChecking { state, from_config };

    assert_eq!(checking(&staged), state(OnOff::Off, false));
    _ = staged.send(Command::Websites(OnOff::On));
    assert_eq!(checking(&staged), state(OnOff::On, false));

    staged.write_config(Some("websites = false\n"));
    staged.request(Request::Reload);
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
    let path = staged.core().config().path;

    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(ConfigFileDetail::Missing, CheckState::Off)
    );
    staged.write_config(Some("[rules.off]\napp = [\"com.figma.Desktop\"]\n"));
    staged.request(Request::Reload);
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(ConfigFileDetail::Valid { rules: 1 }, CheckState::Ok)
    );
    staged.write_config(Some("[rules.off]\ndomian = [\"figma.com\"]\n"));
    staged.request(Request::Reload);
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        config(
            ConfigFileDetail::Error(FileError::Malformed {
                path: path.clone(),
                line: Some(2),
                message: "unknown field `domian`, expected `app` or `domain`".to_owned(),
            }),
            CheckState::Broken
        )
    );
    staged.write_config(Some("[night_shift]\nstrength = 101\n"));
    staged.request(Request::Reload);
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
            AdHocRulesDetail::Off(FileError::Malformed {
                path,
                line: Some(3),
                message: "invalid type: string \"com.figma.Desktop\", expected a sequence"
                    .to_owned(),
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
    staged.send_app(AppCommand::Fix(Fix::StopCheckingWebsites));
    assert_eq!(
        staged.core().preferences().website_checking.state,
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

    let stopped = set_in_config.send_app(AppCommand::Fix(Fix::StopCheckingWebsites));
    let not_opened = managed.send_app(AppCommand::Fix(Fix::DontOpenAtLogin));

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
    staged.send_app(AppCommand::Fix(Fix::KeepNotificationsOff));
    let kept_off = notifications(NotificationsDetail::Off, CheckState::Off);
    assert_eq!(found(&staged, CheckKind::Notifications), kept_off);
    assert!(!staged.core().preferences().notifications_wanted);

    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    staged.report(permissions(true, NotificationPermission::Denied));
    assert_eq!(found(&staged, CheckKind::Notifications), kept_off);
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

    let events = staged
        .report(reported_set_aside(false, Schedule::Off))
        .events;
    assert!(events.contains(&Event::ProblemAppeared {
        check: Check {
            detail: CheckDetail::NightShift(NightShiftDetail::SetAside),
            state: CheckState::Drifted,
        },
    }));
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

    staged.send_app(AppCommand::Fix(Fix::KeepScheduleOff));

    assert_eq!(
        found(&staged, CheckKind::NightShift),
        available(Schedule::Off)
    );
    let staged = staged.restart_at(at("2026-10-16T01:45:00+02:00"));
    assert!(!tells(
        &staged.report(reported_set_aside(false, Schedule::Off))
    ));
    assert_eq!(
        found(&staged, CheckKind::NightShift),
        available(Schedule::Off)
    );

    staged.report(reported(true, Schedule::SunsetToSunrise));
    assert!(tells(
        &staged.report(reported_set_aside(false, Schedule::Off))
    ));
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
    assert!(!tells(&staged.report(Fact::TimerFired)));
    assert_eq!(found(&staged, CheckKind::NightShift), following);

    staged.set_now(at("2026-10-16T01:50:00+02:00"));
    let no_sun_times = Check {
        detail: CheckDetail::NightShift(NightShiftDetail::NoSunTimes {
            waited: SignedDuration::from_mins(10),
        }),
        state: CheckState::Broken,
    };
    assert!(
        staged
            .report(Fact::TimerFired)
            .events
            .contains(&Event::ProblemAppeared {
                check: no_sun_times.clone(),
            })
    );
    assert_eq!(
        found(&staged, CheckKind::NightShift),
        Some((no_sun_times.detail, no_sun_times.state))
    );

    staged.set_now(at("2026-10-16T01:51:00+02:00"));
    report_zurich_sun(&staged);
    staged.report(Fact::TimerFired);
    assert_eq!(found(&staged, CheckKind::NightShift), following);
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
    staged.send_app(AppCommand::Fix(Fix::Reload));
    assert_eq!(
        found(&staged, CheckKind::ConfigFile),
        stands(
            CheckDetail::ConfigFile(ConfigFileDetail::Valid { rules: 1 }),
            CheckState::Ok
        )
    );
}

#[test]
fn the_config_is_read_from_its_xdg_path_and_the_ad_hoc_rules_export_for_it() {
    let staged = noon().stage().unwrap();
    send(&staged, add_rule(figma()));
    send(&staged, add_rule(domain("figma.com")));

    assert_eq!(
        staged.core().config().path,
        etcetera::BaseStrategy::config_dir(&etcetera::base_strategy::Xdg::new().unwrap())
            .join("nightcap/config.toml")
            .display()
            .to_string()
    );
    assert_eq!(
        staged.core().export(ExportFormat::Nix),
        "{ off = { app = [ \"com.figma.Desktop\" ]; domain = [ \"figma.com\" ]; }; on = { app = [ ]; domain = [ ]; }; }\n"
    );
    assert_eq!(
        staged.core().export(ExportFormat::Toml),
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
