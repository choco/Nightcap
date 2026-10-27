#![cfg(debug_assertions)]
//! The settings window: its switches, its checks and their fixes, and the rules tab.

#![expect(
    clippy::unwrap_used,
    reason = "clippy.toml lets tests unwrap, but not the helpers beside them"
)]

mod common;

use nightcap_core::{
    Effect, Event, Fix, ProblemLine, RuleAction, Scenario, SettingsCheck, SettingsLayers,
    SettingsRule, SettingsSwitch, Staged, normalize_domain,
};
use nightcap_protocol::{
    Check, CheckKind, CheckState, Command, ExportFormat, Length, NotificationPermission, OnOff,
    Place, Request, RuleIdentity, Schedule,
};

use common::{
    add_rule, at, button, custom_hours, domain, figma, figma_app, mail, minutes, noon, notifies,
    on_by_the_schedule, permissions, report_zurich_sun, reported, rows, send, set, settings_check,
    status,
};

#[test]
fn settings_switch_website_checking_unless_the_config_sets_it() {
    let staged = on_by_the_schedule().stage().unwrap();
    let switch = |on, locked, command| SettingsSwitch {
        on,
        locked,
        command,
    };

    assert!(
        !rows(&staged)
            .iter()
            .any(|row| row.title == "Check Websites")
    );
    assert_eq!(
        staged.core().settings().website_checking,
        switch(false, false, Command::Websites(OnOff::On))
    );
    staged.core().handle(Event::CommandChosen {
        command: Command::Websites(OnOff::On),
    });
    assert_eq!(
        staged.core().settings().website_checking,
        switch(true, false, Command::Websites(OnOff::Off))
    );

    staged.write_config(Some("websites = false\n"));
    staged.request(Request::Reload);
    assert_eq!(
        staged.core().settings().website_checking,
        switch(false, true, Command::Websites(OnOff::On))
    );
}

/// The check of `kind` in the status, if the core reports one.
fn check_of(staged: &Staged, kind: CheckKind) -> Option<(CheckState, String)> {
    status(staged)
        .checks
        .into_iter()
        .find(|check| check.kind == kind)
        .map(|Check { state, text, .. }| (state, text))
}

fn stands(state: CheckState, text: &str) -> Option<(CheckState, String)> {
    Some((state, text.to_owned()))
}

#[test]
fn missing_accessibility_is_a_problem_only_while_website_checking_is_on() {
    let staged = noon().stage().unwrap();
    staged
        .core()
        .handle(permissions(false, NotificationPermission::Granted));

    assert_eq!(
        check_of(&staged, CheckKind::Accessibility),
        stands(CheckState::Off, "Not needed while website checking is off.")
    );
    send(&staged, Command::Websites(OnOff::On));
    assert_eq!(
        check_of(&staged, CheckKind::Accessibility),
        stands(
            CheckState::Drifted,
            "Accessibility isn’t allowed, so website checking can’t read the front tab."
        )
    );
    staged
        .core()
        .handle(permissions(true, NotificationPermission::Granted));
    assert_eq!(
        check_of(&staged, CheckKind::Accessibility),
        stands(CheckState::Ok, "Accessibility is allowed.")
    );
}

#[test]
fn notifications_switched_off_after_being_allowed_are_a_problem() {
    let staged = noon().stage().unwrap();
    let notifications = |staged: &Staged| check_of(staged, CheckKind::Notifications);

    staged
        .core()
        .handle(permissions(true, NotificationPermission::Denied));
    assert_eq!(
        notifications(&staged),
        stands(CheckState::Off, "Notifications are off.")
    );
    staged
        .core()
        .handle(permissions(true, NotificationPermission::Granted));
    assert_eq!(notifications(&staged), stands(CheckState::Ok, "Allowed."));
    staged
        .core()
        .handle(permissions(true, NotificationPermission::Denied));
    let drifted = stands(
        CheckState::Drifted,
        "Notifications were allowed, but they’re switched off in System Settings.",
    );
    assert_eq!(notifications(&staged), drifted);

    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    staged
        .core()
        .handle(permissions(true, NotificationPermission::Denied));
    assert_eq!(notifications(&staged), drifted);
}

#[test]
fn the_config_file_check_says_whether_the_file_is_missing_valid_or_broken() {
    let staged = noon().stage().unwrap();
    let config = |staged: &Staged| check_of(staged, CheckKind::ConfigFile);

    assert_eq!(config(&staged), stands(CheckState::Off, "No config file."));
    staged.write_config(Some("[rules.off]\napp = [\"com.figma.Desktop\"]\n"));
    staged.request(Request::Reload);
    assert_eq!(
        config(&staged),
        stands(CheckState::Ok, "Valid, with 1 rule.")
    );
    staged.write_config(Some("[rules.off]\ndomian = [\"figma.com\"]\n"));
    staged.request(Request::Reload);
    assert_eq!(
        config(&staged),
        stands(
            CheckState::Broken,
            "Line 2: unknown field `domian`, expected `app` or `domain`."
        )
    );
    staged.write_config(Some("[night_shift]\nstrength = 101\n"));
    staged.request(Request::Reload);
    assert_eq!(
        config(&staged),
        stands(
            CheckState::Broken,
            "night_shift.strength is 0 to 100, not 101."
        )
    );
}

#[test]
fn the_ad_hoc_rules_check_counts_them_or_says_why_they_are_off() {
    let staged = noon().stage().unwrap();

    assert_eq!(
        check_of(&staged, CheckKind::AdHocRules),
        stands(CheckState::Ok, "No ad-hoc rules.")
    );
    send(&staged, add_rule(figma()));
    assert_eq!(
        check_of(&staged, CheckKind::AdHocRules),
        stands(CheckState::Ok, "1 ad-hoc rule.")
    );

    let staged = noon().rules("version = 3\n").stage().unwrap();
    let (state, text) = check_of(&staged, CheckKind::AdHocRules).unwrap();
    assert_eq!(state, CheckState::Broken);
    assert!(text.starts_with("Ad-hoc rules are off: "), "{text}");

    let staged = noon()
        .rules("version = 2\n[rules.off]\napp = \"com.figma.Desktop\"\n")
        .stage()
        .unwrap();
    assert_eq!(
        check_of(&staged, CheckKind::AdHocRules),
        stands(
            CheckState::Broken,
            &format!(
                "Ad-hoc rules are off: {}: line 3: invalid type: string \"com.figma.Desktop\", expected a sequence.",
                etcetera::BaseStrategy::data_dir(&etcetera::base_strategy::Xdg::new().unwrap())
                    .join("nightcap/rules.toml")
                    .display()
            )
        )
    );
}

#[test]
fn night_shift_missing_on_this_mac_is_broken() {
    let staged = noon().stage().unwrap();

    assert_eq!(
        check_of(&staged, CheckKind::NightShift),
        stands(
            CheckState::Broken,
            "Night Shift isn’t available on this Mac."
        )
    );
    staged.core().handle(reported(false, Schedule::Off));
    assert_eq!(
        check_of(&staged, CheckKind::NightShift),
        stands(CheckState::Ok, "Available, with no schedule.")
    );
    staged.core().handle(reported(false, custom_hours()));
    assert_eq!(
        check_of(&staged, CheckKind::NightShift),
        stands(
            CheckState::Ok,
            "Available, following its schedule: 22:00 to 07:00."
        )
    );
}

#[test]
fn settings_offer_to_allow_accessibility_or_stop_checking_websites() {
    let staged = noon().config("").stage().unwrap();
    staged.core().handle(Event::CommandChosen {
        command: Command::Websites(OnOff::On),
    });
    staged
        .core()
        .handle(permissions(false, NotificationPermission::Granted));

    assert_eq!(
        settings_check(&staged, CheckKind::Accessibility),
        Some(SettingsCheck {
            kind: CheckKind::Accessibility,
            state: CheckState::Drifted,
            text: "Accessibility isn’t allowed, so website checking can’t read the front tab."
                .to_owned(),
            restore: button("Allow…", Fix::AskForAccessibility),
            accept: button("Stop Checking Websites", Fix::StopCheckingWebsites),
        })
    );
    staged.core().handle(Event::FixChosen {
        fix: Fix::StopCheckingWebsites,
    });
    assert!(!staged.core().settings().website_checking.on);
    assert_eq!(
        settings_check(&staged, CheckKind::Accessibility),
        Some(SettingsCheck {
            kind: CheckKind::Accessibility,
            state: CheckState::Off,
            text: "Not needed while website checking is off.".to_owned(),
            restore: None,
            accept: None,
        })
    );
}

#[test]
fn settings_offer_to_allow_notifications_or_keep_them_off() {
    let staged = noon().stage().unwrap();
    staged
        .core()
        .handle(permissions(true, NotificationPermission::Granted));
    staged
        .core()
        .handle(permissions(true, NotificationPermission::Denied));

    assert_eq!(
        settings_check(&staged, CheckKind::Notifications),
        Some(SettingsCheck {
            kind: CheckKind::Notifications,
            state: CheckState::Drifted,
            text: "Notifications were allowed, but they’re switched off in System Settings."
                .to_owned(),
            restore: button("Allow Notifications…", Fix::AllowNotifications),
            accept: button("Keep Notifications Off", Fix::KeepNotificationsOff),
        })
    );
    staged.core().handle(Event::FixChosen {
        fix: Fix::KeepNotificationsOff,
    });
    let kept_off = Some(SettingsCheck {
        kind: CheckKind::Notifications,
        state: CheckState::Off,
        text: "Notifications are off.".to_owned(),
        restore: button("Allow Notifications…", Fix::AllowNotifications),
        accept: None,
    });
    assert_eq!(settings_check(&staged, CheckKind::Notifications), kept_off);

    let staged = staged.restart_at(at("2026-10-16T12:10:00+02:00"));
    staged
        .core()
        .handle(permissions(true, NotificationPermission::Denied));
    assert_eq!(settings_check(&staged, CheckKind::Notifications), kept_off);
}

/// Night Shift as CoreBrightness reports it while macOS has sunset to sunrise set aside.
fn reported_set_aside(enabled: bool, schedule: Schedule) -> Event {
    Event::StatusChanged {
        enabled,
        schedule,
        sun_schedule_set_aside: true,
    }
}

fn night_shift_check(state: CheckState, text: &str) -> Option<SettingsCheck> {
    Some(SettingsCheck {
        kind: CheckKind::NightShift,
        state,
        text: text.to_owned(),
        restore: None,
        accept: None,
    })
}

fn set_aside() -> Option<SettingsCheck> {
    Some(SettingsCheck {
        restore: button("Open Location Services…", Fix::OpenLocationServices),
        accept: button("Keep the Schedule Off", Fix::KeepScheduleOff),
        ..night_shift_check(
            CheckState::Drifted,
            "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on.",
        )
        .unwrap()
    })
}

#[test]
fn sunset_to_sunrise_set_aside_is_a_problem_until_location_services_is_back_on() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    report_zurich_sun(&staged);
    staged
        .core()
        .handle(reported(true, Schedule::SunsetToSunrise));

    let effects = staged
        .core()
        .handle(reported_set_aside(false, Schedule::Off));
    assert!(effects.contains(&Effect::Notify {
        title: "Sunset to sunrise needs Location Services".to_owned(),
        body: "macOS turned Night Shift’s schedule off. Open Nightcap’s settings to turn Location Services back on, or to keep the schedule off.".to_owned(),
        opens: None,
    }));
    assert_eq!(settings_check(&staged, CheckKind::NightShift), set_aside());
    assert_eq!(
        staged.core().menu().problem_line,
        Some(ProblemLine {
            title: "1 Thing Needs Attention…".to_owned(),
            subtitle: Some("Night Shift".to_owned()),
        })
    );

    staged
        .core()
        .handle(reported(true, Schedule::SunsetToSunrise));
    assert_eq!(
        settings_check(&staged, CheckKind::NightShift),
        night_shift_check(
            CheckState::Ok,
            "Available, following its schedule: sunset to sunrise."
        )
    );
}

#[test]
fn nightcap_opening_with_sunset_to_sunrise_set_aside_shows_the_problem() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();

    staged
        .core()
        .handle(reported_set_aside(false, Schedule::Off));

    assert_eq!(settings_check(&staged, CheckKind::NightShift), set_aside());
}

#[test]
fn the_schedule_set_aside_can_be_kept_off_until_location_services_is_back_on() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    staged
        .core()
        .handle(reported_set_aside(false, Schedule::Off));

    staged.core().handle(Event::FixChosen {
        fix: Fix::KeepScheduleOff,
    });

    let no_schedule = night_shift_check(CheckState::Ok, "Available, with no schedule.");
    assert_eq!(settings_check(&staged, CheckKind::NightShift), no_schedule);
    let staged = staged.restart_at(at("2026-10-16T01:45:00+02:00"));
    assert!(!notifies(
        &staged
            .core()
            .handle(reported_set_aside(false, Schedule::Off))
    ));
    assert_eq!(settings_check(&staged, CheckKind::NightShift), no_schedule);

    staged
        .core()
        .handle(reported(true, Schedule::SunsetToSunrise));
    assert!(notifies(
        &staged
            .core()
            .handle(reported_set_aside(false, Schedule::Off))
    ));
    assert_eq!(settings_check(&staged, CheckKind::NightShift), set_aside());
}

#[test]
fn a_schedule_chosen_while_sunset_to_sunrise_is_set_aside_is_no_problem() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    staged
        .core()
        .handle(reported_set_aside(false, Schedule::Off));

    staged
        .core()
        .handle(reported_set_aside(true, custom_hours()));

    assert_eq!(
        settings_check(&staged, CheckKind::NightShift),
        night_shift_check(
            CheckState::Ok,
            "Available, following its schedule: 22:00 to 07:00."
        )
    );
}

#[test]
fn sunset_to_sunrise_without_sun_times_for_ten_minutes_is_a_problem() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    staged
        .core()
        .handle(reported(true, Schedule::SunsetToSunrise));
    let following = night_shift_check(
        CheckState::Ok,
        "Available, following its schedule: sunset to sunrise.",
    );

    staged.set_now(at("2026-10-16T01:49:00+02:00"));
    assert!(!notifies(&staged.core().handle(Event::TimerFired)));
    assert_eq!(settings_check(&staged, CheckKind::NightShift), following);

    staged.set_now(at("2026-10-16T01:50:00+02:00"));
    assert!(staged.core().handle(Event::TimerFired).contains(&Effect::Notify {
        title: "Nightcap can’t follow sunset to sunrise".to_owned(),
        body: "macOS hasn’t worked out sunrise and sunset. Custom hours in Night Shift’s settings work without them.".to_owned(),
        opens: None,
    }));
    assert_eq!(
        settings_check(&staged, CheckKind::NightShift),
        night_shift_check(
            CheckState::Broken,
            "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. Custom hours work without them.",
        )
    );

    staged.set_now(at("2026-10-16T01:51:00+02:00"));
    report_zurich_sun(&staged);
    staged.core().handle(Event::TimerFired);
    assert_eq!(settings_check(&staged, CheckKind::NightShift), following);
}

#[test]
fn settings_offer_to_reload_a_broken_config() {
    let staged = noon()
        .config("[rules.off]\ndomian = [\"figma.com\"]\n")
        .stage()
        .unwrap();

    let broken = settings_check(&staged, CheckKind::ConfigFile).unwrap();
    assert_eq!(broken.state, CheckState::Broken);
    assert_eq!(broken.restore, button("Reload", Fix::Reload));
    staged.write_config(Some("[rules.off]\ndomain = [\"figma.com\"]\n"));
    staged.core().handle(Event::FixChosen { fix: Fix::Reload });
    assert_eq!(
        settings_check(&staged, CheckKind::ConfigFile),
        Some(SettingsCheck {
            kind: CheckKind::ConfigFile,
            state: CheckState::Ok,
            text: "Valid, with 1 rule.".to_owned(),
            restore: None,
            accept: None,
        })
    );
}

#[test]
fn the_rules_tab_lists_every_rule_with_what_can_be_done_about_it() {
    let staged = on_by_the_schedule()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n")
        .stage()
        .unwrap();
    let photos = RuleIdentity::App("com.apple.Photos".parse().unwrap());
    send(&staged, add_rule(photos.clone()));
    send(
        &staged,
        Command::AddRule {
            identity: domain("youtube.com"),
            says: OnOff::On,
        },
    );
    let figma_site = Place::Domain("figma.com".parse().unwrap());
    send(
        &staged,
        set(figma_site.clone(), OnOff::On, Length::UntilScheduleTurns),
    );
    let action = |title: &str, command| {
        Some(RuleAction {
            title: title.to_owned(),
            command,
        })
    };
    let rule = |identity: RuleIdentity, says, label: &str, snoozed, action| SettingsRule {
        identity,
        says,
        label: label.to_owned(),
        snoozed,
        action,
    };

    assert_eq!(
        staged.core().settings().rules,
        vec![
            rule(figma(), OnOff::Off, "From your config", false, None),
            rule(
                domain("figma.com"),
                OnOff::Off,
                "Snoozed until 07:00",
                true,
                action("Put Back", Command::Reset(figma_site))
            ),
            rule(
                photos.clone(),
                OnOff::Off,
                "Ad-hoc",
                false,
                action("Remove", Command::RemoveRule(photos))
            ),
            rule(
                domain("youtube.com"),
                OnOff::On,
                "Ad-hoc",
                false,
                action("Remove", Command::RemoveRule(domain("youtube.com")))
            ),
        ]
    );
}

#[test]
fn the_rules_tab_lists_what_holds_for_now_where_no_rule_does() {
    let staged = on_by_the_schedule()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    let mail_app = Place::from(mail());
    send(
        &staged,
        set(mail_app.clone(), OnOff::Off, Length::UntilScheduleTurns),
    );
    send(&staged, set(mail_app.clone(), OnOff::On, minutes(30)));
    // A snooze shows on its rule, and everywhere's own on the Night Shift item.
    send(&staged, set(figma_app(), OnOff::On, minutes(30)));
    send(&staged, Command::Off);

    assert_eq!(
        staged.core().settings().for_now,
        vec![SettingsLayers {
            place: mail_app.clone(),
            label: "Night Shift on until 23:30, then off until 22:00".to_owned(),
            action: RuleAction {
                title: "End".to_owned(),
                command: Command::Reset(mail_app),
            },
        }]
    );
}

#[test]
fn the_rules_tab_shows_where_the_config_is_and_copies_the_ad_hoc_rules_for_it() {
    let staged = noon().stage().unwrap();
    send(&staged, add_rule(figma()));
    send(&staged, add_rule(domain("figma.com")));

    assert_eq!(
        staged.core().settings().config_path,
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
