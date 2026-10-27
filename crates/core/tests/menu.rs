#![cfg(debug_assertions)]
//! The menu: the Night Shift item, the place rows and the problem line (ADR-0018).

#![expect(
    clippy::unwrap_used,
    reason = "clippy.toml lets tests unwrap, but not the helpers beside them"
)]

mod common;

use nightcap_core::{
    Effect, Fact, MenuEntry, MenuIcon, MenuItem, NightShiftItem, PlaceRow, ProblemLine, RowImage,
    RowItem, Scenario, Staged,
};
use nightcap_protocol::{Command, Length, NotificationPermission, OnOff, Place, Schedule};

use common::{
    activate, activate_browser, add_rule, app_row, at, choice_titled, custom_hours, domain,
    everywhere_row, figma, figma_app, forced, late, mail, minutes, noon, on_by_the_schedule,
    on_figma, permissions, report, report_host, report_zurich_sun, reported, rows, send, set,
    site_row, titles,
};

#[test]
fn with_no_schedule_the_night_shift_item_says_so() {
    let staged = noon().stage().unwrap();
    staged.report(reported(false, Schedule::Off));

    assert_eq!(
        staged.core().menu().night_shift.reason.as_deref(),
        Some("No schedule")
    );
}

/// The menu's Night Shift item: checked while it's on everywhere, with what decides the
/// screen.
fn night_shift_item(on: bool, reason: &str, command: Command) -> NightShiftItem {
    NightShiftItem {
        on,
        reason: Some(reason.to_owned()),
        command: Some(command),
    }
}

#[test]
fn the_menu_says_what_night_shift_is_doing_and_why() {
    let staged = forced();
    staged.report(Fact::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    assert_eq!(staged.core().menu().icon, MenuIcon::ForcedOff);
    assert_eq!(
        staged.core().menu().night_shift,
        night_shift_item(true, "Off in Figma: your config’s rule", Command::Off)
    );
    send(&staged, set(Place::Everywhere, OnOff::Off, minutes(30)));
    assert_eq!(
        staged.core().menu().night_shift,
        night_shift_item(false, "Off in Figma: your config’s rule", Command::On)
    );
    activate(&staged, "com.apple.Terminal");
    assert_eq!(staged.core().menu().icon, MenuIcon::Off);
    assert_eq!(
        staged.core().menu().night_shift,
        night_shift_item(false, "Off until 23:30", Command::On)
    );
    send(&staged, Command::On);
    assert_eq!(
        staged.core().menu().night_shift,
        night_shift_item(true, "Following the schedule until 07:00", Command::Off)
    );
    send(&staged, Command::Off);
    assert_eq!(
        staged.core().menu().night_shift,
        night_shift_item(false, "Off until you turn it on", Command::On)
    );
}

#[test]
fn while_night_shift_is_unavailable_the_menu_only_says_so() {
    let staged = noon().stage().unwrap();

    let menu = staged.core().menu();

    assert_eq!(menu.icon, MenuIcon::Unavailable);
    assert_eq!(
        menu.night_shift,
        NightShiftItem {
            on: false,
            reason: Some("Unavailable".to_owned()),
            command: None,
        }
    );
    assert_eq!(menu.entries, Vec::new());
}

/// A site row's title, and the site it names under it.
fn site_title(row: &PlaceRow) -> (&str, Option<&str>) {
    (&row.title, row.site.as_deref())
}

fn choice(title: &str, command: Command) -> RowItem {
    RowItem::Choice {
        item: MenuItem {
            title: title.to_owned(),
            subtitle: None,
            enabled: true,
            command,
        },
    }
}

#[test]
fn the_menu_has_everywheres_row_then_the_apps_and_the_sites() {
    let staged = on_figma().stage().unwrap();
    staged.report(Fact::StrengthChanged { value: 60 });

    let entries = staged.core().menu().entries;

    let kinds: Vec<_> = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Strength { .. } => "strength",
            MenuEntry::Separator => "—",
            MenuEntry::Row { row } => match row.image {
                RowImage::Everywhere => "everywhere",
                RowImage::App { .. } => "app",
                RowImage::Site => "site",
            },
        })
        .collect();
    assert_eq!(kinds, ["strength", "everywhere", "—", "app", "site"]);
    assert!(site_row(&staged).unwrap().indented);
}

#[test]
fn everywheres_row_has_every_length_but_the_switchs() {
    let staged = on_by_the_schedule().stage().unwrap();

    let menu = staged.core().menu();

    assert_eq!(menu.icon, MenuIcon::On);
    assert_eq!(menu.night_shift.command, Some(Command::Off));
    let off = |length| set(Place::Everywhere, OnOff::Off, length);
    assert_eq!(
        everywhere_row(&staged),
        Some(PlaceRow {
            title: "Turn Off".to_owned(),
            site: None,
            subtitle: None,
            image: RowImage::Everywhere,
            indented: false,
            items: vec![
                choice("For 30 Minutes", off(minutes(30))),
                choice("For 1 Hour", off(minutes(60))),
                choice("For 2 Hours", off(minutes(120))),
                RowItem::PickTime {
                    title: "Turn Off Everywhere".to_owned(),
                    place: Place::Everywhere,
                    state: OnOff::Off,
                    start: at("2026-10-17T00:00:00+02:00"),
                },
                RowItem::Choice {
                    item: MenuItem {
                        title: "Until 22:00".to_owned(),
                        subtitle: Some("When the schedule starts".to_owned()),
                        enabled: true,
                        command: off(Length::UntilScheduleTurns),
                    },
                },
            ],
        })
    );
}

/// The line under everywhere's "Until" item at `time`, with Night Shift `enabled` as the
/// schedule has it then.
fn until_note(time: &str, enabled: bool, schedule: Schedule, title: &str) -> Option<String> {
    let staged = Scenario::at(at(time)).stage().unwrap();
    report_zurich_sun(&staged);
    staged.report(reported(enabled, schedule));
    choice_titled(&everywhere_row(&staged).unwrap(), title).subtitle
}

#[test]
fn until_the_schedule_turns_names_sunrise_or_sunset() {
    assert_eq!(
        until_note(
            "2026-10-16T01:40:00+02:00",
            true,
            Schedule::SunsetToSunrise,
            "Until 18:31"
        )
        .as_deref(),
        Some("At sunset")
    );
    assert_eq!(
        until_note(
            "2026-10-16T12:00:00+02:00",
            false,
            Schedule::SunsetToSunrise,
            "Until 07:52"
        )
        .as_deref(),
        Some("At sunrise")
    );
}

#[test]
fn until_the_schedule_turns_names_custom_hours_start_or_end() {
    assert_eq!(
        until_note(
            "2026-10-16T23:00:00+02:00",
            true,
            custom_hours(),
            "Until 22:00"
        )
        .as_deref(),
        Some("When the schedule starts")
    );
    assert_eq!(
        until_note(
            "2026-10-16T12:00:00+02:00",
            false,
            custom_hours(),
            "Until 07:00"
        )
        .as_deref(),
        Some("When the schedule ends")
    );
}

#[test]
fn the_menus_lengths_come_from_the_config() {
    let staged = on_by_the_schedule()
        .config("[menu]\nlengths = [\"15m\", \"1h30m\"]\n")
        .stage()
        .unwrap();

    assert_eq!(
        titles(&everywhere_row(&staged).unwrap()),
        [
            "For 15 Minutes",
            "For 1 Hour 30 Minutes",
            "Custom…",
            "Until 22:00"
        ]
    );
}

#[test]
fn everywheres_row_names_the_place_that_keeps_the_other_way() {
    let staged = noon().config("[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.apple.iBooksX\"]\n").stage().unwrap();
    staged.report(reported(false, custom_hours()));
    let front = |bundle_id: &str, name: &str| {
        staged.report(Fact::AppActivated {
            bundle_id: Some(bundle_id.to_owned()),
            name: Some(name.to_owned()),
            is_browser: false,
        });
        let row = everywhere_row(&staged).unwrap();
        (row.title, row.subtitle)
    };
    let row = |title: &str, subtitle: Option<&str>| (title.to_owned(), subtitle.map(str::to_owned));

    // By day, turning Night Shift on everywhere leaves Figma off, and Books on as it is.
    assert_eq!(
        front("com.figma.Desktop", "Figma"),
        row("Turn On", Some("Everywhere but in Figma"))
    );
    assert_eq!(front("com.apple.iBooksX", "Books"), row("Turn On", None));
    // At night, turning it off everywhere leaves Books on, and Figma off as it is.
    staged.set_now(at("2026-10-16T23:00:00+02:00"));
    staged.report(Fact::Woke);
    assert_eq!(
        front("com.apple.iBooksX", "Books"),
        row("Turn Off", Some("Everywhere but in Books"))
    );
    assert_eq!(front("com.figma.Desktop", "Figma"), row("Turn Off", None));
}

#[test]
fn by_day_the_rows_turn_on_and_by_night_off() {
    let staged = noon().stage().unwrap();
    staged.report(reported(false, custom_hours()));
    staged.report(Fact::AppActivated {
        bundle_id: Some("com.apple.mail".to_owned()),
        name: Some("Mail".to_owned()),
        is_browser: false,
    });
    let titles_of = |staged: &Staged| -> Vec<String> {
        rows(staged).into_iter().map(|row| row.title).collect()
    };

    assert_eq!(titles_of(&staged), ["Turn On", "Turn On in Mail"]);
    staged.set_now(at("2026-10-16T23:00:00+02:00"));
    staged.report(Fact::Woke);
    assert_eq!(titles_of(&staged), ["Turn Off", "Turn Off in Mail"]);
}

#[test]
fn without_the_sun_times_an_apps_row_turns_night_shift_the_other_way_from_how_it_is() {
    let staged = late().stage().unwrap();
    staged.report(Fact::AppActivated {
        bundle_id: Some("com.googlecode.iterm2".to_owned()),
        name: Some("iTerm2".to_owned()),
        is_browser: false,
    });

    staged.report(reported(true, Schedule::SunsetToSunrise));
    assert_eq!(app_row(&staged).unwrap().title, "Turn Off in iTerm2");
    staged.report(reported(false, Schedule::SunsetToSunrise));
    assert_eq!(app_row(&staged).unwrap().title, "Turn On in iTerm2");
}

#[test]
fn without_the_sun_times_the_menu_follows_the_schedule_as_macos_applies_it() {
    let staged = late().stage().unwrap();

    staged.report(reported(true, Schedule::SunsetToSunrise));

    let menu = staged.core().menu();
    assert!(menu.night_shift.on);
    assert_eq!(
        menu.night_shift.reason.as_deref(),
        Some("Following the schedule")
    );
    let row = everywhere_row(&staged).unwrap();
    assert_eq!(row.title, "Turn Off");
    assert_eq!(
        titles(&row),
        ["For 30 Minutes", "For 1 Hour", "For 2 Hours", "Custom…"]
    );
    assert!(
        staged
            .send(choice_titled(&row, "For 30 Minutes").command)
            .0
            .effects
            .contains(&Effect::SetNightShift { enabled: false })
    );
}

#[test]
fn undoing_is_offered_where_it_goes_the_right_way_and_a_rule_of_yours_can_always_go() {
    let staged = forced();
    send(
        &staged,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );
    assert_eq!(
        app_row(&staged).unwrap().items[0],
        choice("Put the Rule Back", Command::Reset(figma_app()))
    );

    activate(&staged, "com.apple.mail");
    send(&staged, set(Place::from(mail()), OnOff::Off, minutes(30)));
    assert_eq!(
        app_row(&staged).unwrap().items[0],
        choice("Like Everywhere Else", Command::Reset(Place::from(mail())))
    );

    send(&staged, Command::Reset(Place::from(mail())));
    send(&staged, add_rule(mail()));
    assert_eq!(
        choice_titled(&app_row(&staged).unwrap(), "Remove Your Rule").command,
        Command::RemoveRule(mail())
    );
    // By day removing it changes nothing on screen, but the rule is still the user's to remove.
    staged.set_now(at("2026-10-17T12:00:00+02:00"));
    staged.report(Fact::Woke);
    assert_eq!(
        choice_titled(&app_row(&staged).unwrap(), "Remove Your Rule").command,
        Command::RemoveRule(mail())
    );
}

#[test]
fn lengths_that_come_to_the_same_as_undoing_are_left_out() {
    let staged = forced();
    send(
        &staged,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );

    assert_eq!(
        titles(&app_row(&staged).unwrap()),
        [
            "Put the Rule Back",
            "—",
            "For 30 Minutes",
            "For 1 Hour",
            "For 2 Hours",
            "Custom…"
        ]
    );
    activate(&staged, "com.apple.mail");
    send(&staged, set(Place::from(mail()), OnOff::Off, minutes(30)));
    assert_eq!(
        titles(&app_row(&staged).unwrap()),
        ["Like Everywhere Else", "—", "Always"]
    );
}

#[test]
fn a_config_rules_always_is_greyed() {
    let staged = forced();

    let row = app_row(&staged).unwrap();

    assert_eq!(row.title, "Turn On in com.figma.Desktop");
    assert_eq!(
        row.subtitle.as_deref(),
        Some("Always off: your config’s rule")
    );
    let always = choice_titled(&row, "Always");
    assert!(!always.enabled);
    assert_eq!(always.subtitle.as_deref(), Some("Your config decides"));
}

#[test]
fn custom_is_offered_beside_the_lengths_with_its_place_and_way() {
    let staged = forced();
    staged.set_now(at("2026-10-16T23:10:00+02:00"));
    staged.report(Fact::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    let row = app_row(&staged).unwrap();

    assert!(row.items.contains(&RowItem::PickTime {
        title: "Turn On in Figma".to_owned(),
        place: figma_app(),
        state: OnOff::On,
        start: at("2026-10-17T00:15:00+02:00"),
    }));
}

#[test]
fn the_site_row_is_the_tab_domain_or_the_most_specific_covering_rules() {
    let staged = on_by_the_schedule()
        .config("websites = true\n")
        .stage()
        .unwrap();
    activate_browser(&staged, "com.apple.Safari");
    report_host(&staged, 1, "docs.figma.com");
    let row = site_row(&staged).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn Off at This Site", Some("docs.figma.com"))
    );
    assert_eq!(
        choice_titled(&row, "Always").command,
        add_rule(domain("docs.figma.com"))
    );
    report_host(&staged, 1, "www.figma.com");
    assert_eq!(
        site_title(&site_row(&staged).unwrap()),
        ("Turn Off at This Site", Some("figma.com"))
    );

    report_host(&staged, 1, "docs.figma.com");
    send(&staged, add_rule(domain("figma.com")));
    assert_eq!(
        site_title(&site_row(&staged).unwrap()),
        ("Turn On at This Site", Some("figma.com"))
    );
    send(&staged, add_rule(domain("docs.figma.com")));
    assert_eq!(
        site_title(&site_row(&staged).unwrap()),
        ("Turn On at This Site", Some("docs.figma.com"))
    );
}

#[test]
fn a_site_goes_back_to_its_browsers_say_by_the_browsers_name() {
    let staged = on_by_the_schedule().config("websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n").stage().unwrap();
    staged.report(Fact::AppActivated {
        bundle_id: Some("com.apple.Safari".to_owned()),
        name: Some("Safari".to_owned()),
        is_browser: true,
    });
    report_host(&staged, 1, "example.com");
    let example = Place::Domain("example.com".parse().unwrap());

    send(&staged, set(example.clone(), OnOff::On, minutes(30)));

    let row = site_row(&staged).unwrap();
    assert_eq!(
        choice_titled(&row, "Like the Rest of Safari").command,
        Command::Reset(example)
    );
}

#[test]
fn a_site_that_says_nothing_turns_the_other_way_from_its_browser() {
    let staged = on_by_the_schedule().config("websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n").stage().unwrap();
    activate_browser(&staged, "com.apple.Safari");

    report_host(&staged, 1, "example.com");

    let row = site_row(&staged).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn On at This Site", Some("example.com"))
    );
    assert_eq!(
        choice_titled(&row, "Always").command,
        Command::AddRule {
            identity: domain("example.com"),
            says: OnOff::On,
        }
    );
}

#[test]
fn the_site_row_snoozes_a_config_rule() {
    let staged = on_figma().stage().unwrap();
    let figma_site = Place::Domain("figma.com".parse().unwrap());

    let row = site_row(&staged).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn On at This Site", Some("figma.com"))
    );
    assert_eq!(
        row.subtitle.as_deref(),
        Some("Always off: your config’s rule")
    );
    let until_morning = choice_titled(&row, "Until 07:00").command;
    assert_eq!(
        until_morning,
        set(figma_site.clone(), OnOff::On, Length::UntilScheduleTurns)
    );
    send(&staged, until_morning);
    let row = site_row(&staged).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn Off at This Site", Some("figma.com"))
    );
    assert_eq!(
        row.subtitle.as_deref(),
        Some("On until 07:00, then your config’s rule")
    );
    assert_eq!(
        row.items[0],
        choice("Put the Rule Back", Command::Reset(figma_site))
    );
}

#[test]
fn the_menu_opens_with_a_strength_slider_once_the_strength_is_known() {
    let staged = noon().stage().unwrap();
    staged.report(reported(false, Schedule::Off));
    assert!(
        !staged
            .core()
            .menu()
            .entries
            .iter()
            .any(|entry| matches!(entry, MenuEntry::Strength { .. }))
    );

    staged.report(Fact::StrengthChanged { value: 60 });

    assert_eq!(
        staged.core().menu().entries[0],
        MenuEntry::Strength { value: 60 }
    );
}

#[test]
fn the_menu_adds_and_removes_an_ad_hoc_rule_for_the_frontmost_app() {
    let staged = on_by_the_schedule().stage().unwrap();
    staged.report(Fact::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    let row = app_row(&staged).unwrap();
    assert_eq!(row.title, "Turn Off in Figma");
    assert_eq!(row.subtitle, None);
    assert_eq!(choice_titled(&row, "Always").command, add_rule(figma()));
    send(&staged, add_rule(figma()));
    let row = app_row(&staged).unwrap();
    assert_eq!(row.title, "Turn On in Figma");
    assert_eq!(row.subtitle.as_deref(), Some("Always off: your rule"));
    assert_eq!(
        choice_titled(&row, "Remove Your Rule").command,
        Command::RemoveRule(figma())
    );
}

#[test]
fn choosing_a_menu_item_runs_its_command() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate(&staged, "com.figma.Desktop");

    let effects = staged.send(add_rule(figma())).0.effects;

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
}

#[test]
fn the_menu_shows_a_problem_line_while_any_problem_exists() {
    let staged = noon().config("websites = true\n").stage().unwrap();
    staged.report(reported(false, Schedule::Off));
    let line = |title: &str, subtitle: Option<&str>| {
        Some(ProblemLine {
            title: title.to_owned(),
            subtitle: subtitle.map(str::to_owned),
        })
    };

    assert_eq!(staged.core().menu().problem_line, None);
    staged.report(permissions(false, NotificationPermission::Granted));
    assert_eq!(
        staged.core().menu().problem_line,
        line("1 Thing Needs Attention…", Some("Accessibility"))
    );
    staged.report(permissions(false, NotificationPermission::Denied));
    assert_eq!(
        staged.core().menu().problem_line,
        line(
            "2 Things Need Attention…",
            Some("Accessibility and notifications")
        )
    );
    staged.report(permissions(true, NotificationPermission::Granted));
    assert_eq!(staged.core().menu().problem_line, None);
}

#[test]
fn a_rule_from_the_tab_keeps_www_where_nothing_but_a_suffix_would_be_left() {
    let staged = late().config("websites = true\n").stage().unwrap();
    report(&staged, true);
    activate_browser(&staged, "com.apple.Safari");
    report_host(&staged, 1, "www.com");

    let row = site_row(&staged).unwrap();
    assert_eq!(site_title(&row), ("Turn Off at This Site", Some("www.com")));
    assert_eq!(
        choice_titled(&row, "Always").command,
        add_rule(domain("www.com"))
    );
}
