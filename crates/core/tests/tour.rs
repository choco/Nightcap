#![cfg(debug_assertions)]
//! The tour: its stand-in, its rules, and the facts and titles its steps follow (ADR-0016).

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "clippy.toml lets tests unwrap and panic, but not the helpers beside them"
)]

mod common;

use nightcap_core::{
    Effect, Fact, File, MenuItem, MenuPath, PlaceRow, RowItem, Scenario, Staged, TourApp,
    TourFacts, TourPlace, TourSite, TourTitles, UiCommand, tour_titles,
};
use nightcap_protocol::{
    Command, Decision, ErrorCode, ExportFormat, Lasting, Length, OnOff, Payload, Place, Reason,
    Request, Response, Rule, RuleIdentity, SetBy,
};

use common::{
    activate, activate_browser, activate_browser_event, ad_hoc, add_rule, app_row, app_rule, at,
    choice_titled, domain, everywhere_row, figma, figma_app, kept_ad_hoc, late, mail, minutes,
    off_by_the_schedule, on_by_the_schedule, refusal, report, report_host, resettable, rules, send,
    set, site_row, titles, until, yours,
};

const SAMPLE: &str = "io.github.choco.nightcap.tour-sample";

fn sample() -> RuleIdentity {
    RuleIdentity::App(SAMPLE.parse().unwrap())
}

fn sample_rule() -> Rule {
    Rule {
        place: Place::from(sample()),
        says: OnOff::Off,
        length: Length::Indefinite,
        set_by: SetBy::Tour,
    }
}

const SITE: &str = "nightcap.choco.me";

fn tour_sample() -> TourApp {
    TourApp {
        bundle_id: SAMPLE.parse().unwrap(),
        name: "Tour Sample".to_owned(),
    }
}

fn tour_site() -> TourSite {
    TourSite {
        domain: SITE.parse().unwrap(),
        browser: "com.apple.Safari".parse().unwrap(),
    }
}

/// The app the tour's steps give as an example of a rule that says on.
fn mail_app() -> TourApp {
    TourApp {
        bundle_id: "com.apple.mail".parse().unwrap(),
        name: "Mail".to_owned(),
    }
}

fn start_tour(staged: &Staged) -> Vec<Effect> {
    staged.send_ui(tour_started()).effects
}

/// The tour starts with its sample, its site and Mail as the example.
fn tour_started() -> UiCommand {
    UiCommand::StartTour {
        sample: tour_sample(),
        site: Some(tour_site()),
        example: mail_app(),
    }
}

/// Night Shift on for the tour, over the custom hours' off until 22:00.
fn on_for_the_tour() -> Decision {
    Decision {
        state: Some(OnOff::On),
        place: Place::Everywhere,
        reasons: vec![
            Reason::Tour,
            Reason::Schedule {
                state: OnOff::Off,
                until: Some(at("2026-10-16T22:00:00+02:00")),
            },
        ],
    }
}

#[test]
fn a_tour_started_with_night_shift_off_by_the_schedule_turns_it_on_for_the_tour() {
    let staged = off_by_the_schedule().stage().unwrap();

    assert_eq!(
        start_tour(&staged),
        vec![Effect::SetNightShift { enabled: true }]
    );
    assert_eq!(report(&staged, true), Vec::new());

    assert_eq!(staged.core().front().decision, on_for_the_tour());
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn the_tours_end_turns_night_shift_back_off_following_the_schedule() {
    let staged = off_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    report(&staged, true);

    assert_eq!(
        staged.send_ui(UiCommand::EndTour).effects,
        vec![Effect::SetNightShift { enabled: false }]
    );
    assert_eq!(report(&staged, false), Vec::new());

    assert_eq!(
        staged.core().front().decision.reasons,
        vec![Reason::Schedule {
            state: OnOff::Off,
            until: Some(at("2026-10-16T22:00:00+02:00")),
        }]
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn the_tour_sets_aside_everywheres_indefinite_rule_and_puts_it_back() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    report(&staged, false);

    assert_eq!(
        start_tour(&staged),
        vec![Effect::SetNightShift { enabled: true }]
    );
    report(&staged, true);
    assert_eq!(
        staged.send_ui(UiCommand::EndTour).effects,
        vec![Effect::SetNightShift { enabled: false }]
    );

    assert_eq!(
        resettable(&staged),
        vec![yours(Place::Everywhere, OnOff::Off, Length::Indefinite)]
    );
}

#[test]
fn the_tour_sets_aside_everywheres_rule_until_a_time_and_puts_it_back_with_its_end() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, set(Place::Everywhere, OnOff::Off, minutes(60)));
    report(&staged, false);

    assert_eq!(
        start_tour(&staged),
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    report(&staged, true);
    assert_eq!(
        staged.send_ui(UiCommand::EndTour).effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T00:00:00+02:00").into()
            },
        ]
    );

    assert_eq!(
        staged.core().front().decision.reasons[0],
        Reason::Rule(yours(
            Place::Everywhere,
            OnOff::Off,
            until("2026-10-17T00:00:00+02:00")
        ))
    );
}

#[test]
fn while_the_tour_stands_in_everywheres_row_offers_no_way_back_to_the_schedule() {
    let staged = off_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    report(&staged, true);

    send(
        &staged,
        set(Place::Everywhere, OnOff::On, Lasting::UntilScheduleTurns),
    );
    send(&staged, set(Place::Everywhere, OnOff::Off, minutes(30)));

    // With nothing else to offer, the row is left out.
    assert!(
        everywhere_row(&staged)
            .is_none_or(|row| !titles(&row).contains(&"Back to the Schedule".to_owned()))
    );
}

#[test]
fn while_the_tour_stands_in_a_tap_off_and_on_returns_to_the_tours_on() {
    let staged = off_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    report(&staged, true);

    let (effects, _) = send(&staged, Command::Toggle);
    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    report(&staged, false);
    let (effects, response) = send(&staged, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, Response::Ok(Payload::Decision(on_for_the_tour())));
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_tour_started_with_night_shift_on_changes_nothing() {
    let staged = on_by_the_schedule().stage().unwrap();

    assert_eq!(start_tour(&staged), Vec::new());
    assert!(matches!(
        staged.core().front().decision.reasons[..],
        [Reason::Schedule { .. }]
    ));
    assert_eq!(staged.send_ui(UiCommand::EndTour).effects, Vec::new());
}

#[test]
fn adding_the_samples_rule_mid_tour_forces_night_shift_off_while_it_is_in_front() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    assert_eq!(activate(&staged, SAMPLE), Vec::new());

    let (effects, response) = send(&staged, add_rule(sample()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: Place::App(SAMPLE.parse().unwrap()),
            reasons: vec![Reason::Rule(sample_rule())],
        }))
    );
    report(&staged, false);
    assert_eq!(
        activate(&staged, "com.apple.mail"),
        vec![Effect::SetNightShift { enabled: true }]
    );
}

#[test]
fn outside_a_tour_the_sample_matches_no_rule() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    send(&staged, add_rule(sample()));
    activate(&staged, "com.apple.mail");
    staged.send_ui(UiCommand::EndTour);

    assert_eq!(activate(&staged, SAMPLE), Vec::new());
}

#[test]
fn removing_the_samples_rule_mid_tour_drops_it() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    activate(&staged, SAMPLE);
    send(&staged, add_rule(sample()));
    report(&staged, false);

    let (effects, response) = send(&staged, Command::RemoveRule(sample()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert!(matches!(response, Response::Ok(_)), "{response:?}");
    assert_eq!(rules(&staged), Vec::new());
}

#[test]
fn the_menu_offers_to_add_the_samples_rule_and_then_to_remove_it() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    staged.report(Fact::AppActivated {
        bundle_id: Some(SAMPLE.to_owned()),
        name: Some("Tour Sample".to_owned()),
        is_browser: false,
    });
    let row = app_row(&staged).unwrap();
    assert_eq!(row.title, "Turn Off in Tour Sample");
    assert_eq!(choice_titled(&row, "Always").command, add_rule(sample()));

    send(&staged, add_rule(sample()));

    let row = app_row(&staged).unwrap();
    assert_eq!(row.title, "Turn On in Tour Sample");
    assert_eq!(row.subtitle.as_deref(), Some("Always off: the tour’s rule"));
    assert_eq!(
        choice_titled(&row, "Remove Your Rule").command,
        Command::RemoveRule(sample())
    );
}

#[test]
fn rules_added_mid_tour_stay_out_of_settings_and_files_and_go_with_their_snoozes() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    let site = RuleIdentity::Domain("choco.me".parse().unwrap());
    for identity in [sample(), site.clone()] {
        let (_, response) = send(&staged, add_rule(identity));
        assert!(matches!(response, Response::Ok(_)), "{response:?}");
    }
    let (_, response) = send(
        &staged,
        set(
            Place::App(SAMPLE.parse().unwrap()),
            OnOff::On,
            Lasting::UntilScheduleTurns,
        ),
    );
    assert!(matches!(response, Response::Ok(_)), "{response:?}");

    assert_eq!(
        rules(&staged),
        vec![
            sample_rule(),
            Rule {
                place: Place::from(site),
                ..sample_rule()
            },
        ]
    );
    assert!(staged.core().settings().rules.is_empty());
    assert!(!staged.core().export(ExportFormat::Toml).contains(SAMPLE));
    let kept = staged.file(File::Rules).unwrap_or_default();
    assert!(
        !kept.contains(SAMPLE) && !kept.contains("choco.me"),
        "{kept}"
    );
    let saved = staged.file(File::State).unwrap();
    assert!(!saved.contains(SAMPLE), "{saved}");

    staged.send_ui(UiCommand::EndTour);
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn adding_a_rule_that_exists_mid_tour_leaves_it_as_it_is() {
    let staged = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    report(&staged, true);
    start_tour(&staged);

    send(&staged, add_rule(figma()));

    assert_eq!(rules(&staged), vec![app_rule("com.figma.Desktop")]);
    staged.send_ui(UiCommand::EndTour);
    assert_eq!(rules(&staged), vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn adding_the_rule_a_snoozed_app_has_mid_tour_ends_the_snooze_only_while_it_runs() {
    let staged = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    report(&staged, true);
    send(&staged, set(figma_app(), OnOff::On, minutes(30)));
    start_tour(&staged);

    send(&staged, add_rule(figma()));
    assert_eq!(resettable(&staged), Vec::new());
    staged.send_ui(UiCommand::EndTour);

    assert_eq!(
        resettable(&staged),
        vec![yours(
            figma_app(),
            OnOff::On,
            until("2026-10-16T23:30:00+02:00")
        )]
    );
}

#[test]
fn a_config_rule_reloaded_mid_tour_replaces_the_tours_rule_for_its_app() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    send(&staged, add_rule(sample()));
    staged.write_config(Some(format!("[rules.on]\napp = [\"{SAMPLE}\"]\n").as_str()));

    staged.request(Request::Reload);

    let config_rule = Rule {
        says: OnOff::On,
        set_by: SetBy::Config,
        ..sample_rule()
    };
    assert_eq!(rules(&staged), vec![config_rule]);
    let (_, response) = send(&staged, Command::RemoveRule(sample()));
    assert_eq!(refusal(response), ErrorCode::ConfigRule);
}

#[test]
fn removing_your_own_rule_mid_tour_lasts_only_while_it_runs() {
    let staged = on_by_the_schedule().stage().unwrap();
    let mail = RuleIdentity::App("com.apple.mail".parse().unwrap());
    send(&staged, add_rule(mail.clone()));
    start_tour(&staged);

    send(&staged, Command::RemoveRule(mail.clone()));
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(kept_ad_hoc(&staged), vec![ad_hoc(mail.clone())]);
    staged.send_ui(UiCommand::EndTour);

    assert_eq!(rules(&staged), vec![ad_hoc(mail)]);
}

#[test]
fn turning_your_own_rule_the_other_way_mid_tour_lasts_only_while_it_runs() {
    let staged = on_by_the_schedule().stage().unwrap();
    let mail = RuleIdentity::App("com.apple.mail".parse().unwrap());
    send(&staged, add_rule(mail.clone()));
    start_tour(&staged);

    send(
        &staged,
        Command::AddRule {
            identity: mail.clone(),
            says: OnOff::On,
        },
    );
    assert_eq!(
        rules(&staged),
        vec![Rule {
            says: OnOff::On,
            ..ad_hoc(mail.clone())
        }]
    );
    assert!(
        staged
            .core()
            .export(ExportFormat::Toml)
            .contains("[rules.off]")
    );
    staged.send_ui(UiCommand::EndTour);

    assert_eq!(rules(&staged), vec![ad_hoc(mail)]);
}

#[test]
fn snoozes_and_the_switch_mid_tour_last_only_while_it_runs() {
    let staged = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    report(&staged, true);
    start_tour(&staged);

    let figma = Place::App("com.figma.Desktop".parse().unwrap());
    send(&staged, set(figma, OnOff::On, minutes(30)));
    send(&staged, Command::Off);
    assert_eq!(resettable(&staged).len(), 2);
    staged.send_ui(UiCommand::EndTour);

    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_restart_after_a_crash_mid_tour_puts_night_shift_back_where_the_tour_found_it() {
    let crashed = on_by_the_schedule().stage().unwrap();
    send(&crashed, Command::Off);
    report(&crashed, false);
    start_tour(&crashed);
    report(&crashed, true);

    let staged = crashed.restart_at(at("2026-10-16T23:10:00+02:00"));
    let effects = report(&staged, true);

    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(
        resettable(&staged),
        vec![yours(Place::Everywhere, OnOff::Off, Length::Indefinite)]
    );
}

#[test]
fn quitting_mid_tour_ends_it() {
    let staged = off_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    report(&staged, true);

    assert_eq!(
        staged.stop().effects,
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn starting_the_tour_again_keeps_what_the_first_start_set_aside() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    report(&staged, false);
    start_tour(&staged);
    report(&staged, true);

    assert_eq!(start_tour(&staged), Vec::new());
    staged.send_ui(UiCommand::EndTour);

    assert_eq!(
        resettable(&staged),
        vec![yours(Place::Everywhere, OnOff::Off, Length::Indefinite)]
    );
}

// What the tour's steps follow and the titles they quote (ADR-0016).

/// The tour's facts, whose switch is always the Night Shift item's.
fn tour_facts(staged: &Staged) -> TourFacts {
    let facts = staged.core().tour_report().expect("a tour runs").facts;
    assert_eq!(facts.everywhere_on, staged.core().menu().night_shift.on);
    facts
}

fn away() -> TourPlace {
    TourPlace {
        in_front: false,
        rule: None,
        snoozed: false,
    }
}

fn bring(staged: &Staged, app: &TourApp) -> Vec<Effect> {
    staged
        .report(Fact::AppActivated {
            bundle_id: Some(app.bundle_id.to_string()),
            name: Some(app.name.clone()),
            is_browser: false,
        })
        .effects
}

fn choose(staged: &Staged, command: Command) -> Vec<Effect> {
    staged.send(command).0.effects
}

/// The row's item that sends a command `wanted` picks out, which it must have.
fn item_sending(row: &PlaceRow, wanted: impl Fn(&Command) -> bool) -> MenuItem {
    row.items
        .iter()
        .find_map(|item| match item {
            RowItem::Choice { item } if wanted(&item.command) => Some(item.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no such item in {:?}", titles(row)))
}

fn snoozes(command: &Command) -> bool {
    matches!(
        command,
        Command::Set {
            length: Lasting::For(_),
            ..
        }
    )
}

fn sample_place() -> Place {
    Place::App(SAMPLE.parse().unwrap())
}

#[test]
fn the_tour_reports_nothing_before_it_starts_or_once_it_ends() {
    let staged = on_by_the_schedule().stage().unwrap();
    assert_eq!(staged.core().tour_report(), None);

    start_tour(&staged);
    assert!(staged.core().tour_report().is_some());
    staged.send_ui(UiCommand::EndTour);
    assert_eq!(staged.core().tour_report(), None);

    start_tour(&staged);
    staged.stop();
    assert_eq!(staged.core().tour_report(), None);
}

#[test]
fn the_samples_facts_follow_it_to_the_front_into_its_rule_and_away() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    assert_eq!(tour_facts(&staged).sample, away());

    bring(&staged, &tour_sample());
    assert_eq!(
        tour_facts(&staged).sample,
        TourPlace {
            in_front: true,
            ..away()
        }
    );
    let always = choice_titled(&app_row(&staged).unwrap(), "Always");
    choose(&staged, always.command);
    assert_eq!(
        tour_facts(&staged).sample,
        TourPlace {
            in_front: true,
            rule: Some(OnOff::Off),
            snoozed: false,
        }
    );
    bring(&staged, &mail_app());

    assert_eq!(
        tour_facts(&staged).sample,
        TourPlace {
            rule: Some(OnOff::Off),
            ..away()
        }
    );
}

#[test]
fn a_snooze_from_the_samples_row_and_putting_the_rule_back_show_in_its_facts() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    bring(&staged, &tour_sample());
    choose(&staged, add_rule(sample()));

    let snooze = item_sending(&app_row(&staged).unwrap(), snoozes);
    choose(&staged, snooze.command);
    let snoozed = TourPlace {
        in_front: true,
        rule: Some(OnOff::Off),
        snoozed: true,
    };
    assert_eq!(tour_facts(&staged).sample, snoozed);
    let back = item_sending(&app_row(&staged).unwrap(), |command| {
        *command == Command::Reset(sample_place())
    });
    choose(&staged, back.command);

    assert_eq!(
        tour_facts(&staged).sample,
        TourPlace {
            snoozed: false,
            ..snoozed
        }
    );
}

#[test]
fn a_snooze_that_ends_while_the_sample_is_in_the_background_shows_in_its_facts() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    bring(&staged, &tour_sample());
    choose(&staged, add_rule(sample()));
    choose(&staged, set(sample_place(), OnOff::On, minutes(30)));
    bring(&staged, &mail_app());
    assert!(tour_facts(&staged).sample.snoozed);

    staged.set_now(at("2026-10-16T23:30:00+02:00"));
    staged.report(Fact::TimerFired);

    assert_eq!(
        tour_facts(&staged).sample,
        TourPlace {
            rule: Some(OnOff::Off),
            ..away()
        }
    );
}

#[test]
fn a_rule_and_a_snooze_for_the_sample_from_the_cli_show_in_its_facts() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);

    send(&staged, add_rule(sample()));
    assert_eq!(
        tour_facts(&staged).sample,
        TourPlace {
            rule: Some(OnOff::Off),
            ..away()
        }
    );
    send(
        &staged,
        set(sample_place(), OnOff::On, Lasting::UntilScheduleTurns),
    );

    assert_eq!(
        tour_facts(&staged).sample,
        TourPlace {
            rule: Some(OnOff::Off),
            snoozed: true,
            ..away()
        }
    );
}

/// At 23:00 with Night Shift on by the custom hours and website checking on, the tour started
/// and Safari in front.
fn tour_in_safari() -> Scenario {
    on_by_the_schedule()
        .config("websites = true\n")
        .then(tour_started())
        .then(activate_browser_event("com.apple.Safari"))
}

#[test]
fn the_site_is_in_front_on_its_host_and_its_www_and_not_on_another_tab() {
    let staged = tour_in_safari().stage().unwrap();
    assert_eq!(tour_facts(&staged).site, Some(away()));
    let in_front = TourPlace {
        in_front: true,
        ..away()
    };

    report_host(&staged, 1, SITE);
    assert_eq!(tour_facts(&staged).site, Some(in_front.clone()));
    report_host(&staged, 1, "www.nightcap.choco.me");
    assert_eq!(tour_facts(&staged).site, Some(in_front));
    report_host(&staged, 1, "example.com");
    assert_eq!(tour_facts(&staged).site, Some(away()));
    assert_eq!(tour_facts(&staged).sample, away());
}

#[test]
fn the_sites_own_rule_shows_in_its_facts_and_a_tap_changes_only_the_switch() {
    let staged = tour_in_safari().stage().unwrap();
    report_host(&staged, 1, "www.nightcap.choco.me");
    let always = choice_titled(&site_row(&staged).unwrap(), "Always");
    assert_eq!(always.command, add_rule(domain(SITE)));
    choose(&staged, always.command);
    let ruled = tour_facts(&staged);
    assert_eq!(
        ruled.site,
        Some(TourPlace {
            in_front: true,
            rule: Some(OnOff::Off),
            snoozed: false,
        })
    );
    assert!(ruled.everywhere_on);

    choose(&staged, Command::Toggle);

    assert_eq!(
        tour_facts(&staged),
        TourFacts {
            everywhere_on: false,
            ..ruled
        }
    );
}

#[test]
fn a_tour_without_a_site_step_reports_no_site() {
    let staged = on_by_the_schedule().stage().unwrap();

    staged.send_ui(UiCommand::StartTour {
        sample: tour_sample(),
        site: None,
        example: mail_app(),
    });

    let tour = staged.core().tour_report().unwrap();
    assert_eq!(tour.facts.site, None);
    assert_eq!(tour.titles.add_site_rule, None);
}

#[test]
fn the_tour_stands_in_as_night_shift_was_when_it_started_whatever_the_switch_does() {
    let staged = off_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    report(&staged, true);
    assert!(tour_facts(&staged).stands_in);
    choose(&staged, Command::Toggle);
    report(&staged, false);
    let tapped = tour_facts(&staged);
    assert!(tapped.stands_in);
    assert!(!tapped.everywhere_on);

    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    assert!(!tour_facts(&staged).stands_in);
    choose(&staged, Command::Toggle);
    report(&staged, false);
    let tapped = tour_facts(&staged);
    assert!(!tapped.stands_in);
    assert!(!tapped.everywhere_on);
}

#[test]
fn the_tours_titles_are_the_menus() {
    let path = |row: &str, item: &str| MenuPath {
        row: row.to_owned(),
        item: item.to_owned(),
    };

    assert_eq!(
        tour_titles(tour_sample(), Some(tour_site()), mail_app()),
        TourTitles {
            add_sample_rule: path("Turn Off in Tour Sample", "Always"),
            snooze_sample: "Turn On in Tour Sample".to_owned(),
            put_sample_rule_back: path("Turn Off in Tour Sample", "Put the Rule Back"),
            add_site_rule: Some(path("Turn Off at This Site", "Always")),
            add_example_rule: path("Turn On in Mail", "Always"),
        }
    );
}

#[test]
fn each_title_the_tour_quotes_is_where_the_menu_shows_it_in_the_situation_its_step_describes() {
    let staged = off_by_the_schedule()
        .config("websites = true\n")
        .stage()
        .unwrap();
    start_tour(&staged);
    report(&staged, true);
    let titles = staged.core().tour_report().unwrap().titles;
    let quoted = |row: &PlaceRow, wanted: &dyn Fn(&Command) -> bool| {
        let item = item_sending(row, wanted);
        (
            MenuPath {
                row: row.title.clone(),
                item: item.title,
            },
            item.command,
        )
    };
    let rule_for = |identity: RuleIdentity| move |command: &Command| matches!(command, Command::AddRule { identity: ruled, .. } if *ruled == identity);

    bring(&staged, &tour_sample());
    let (shown, always) = quoted(&app_row(&staged).unwrap(), &rule_for(sample()));
    assert_eq!(shown, titles.add_sample_rule);
    choose(&staged, always);
    let row = app_row(&staged).unwrap();
    assert_eq!(row.title, titles.snooze_sample);
    choose(&staged, item_sending(&row, snoozes).command);
    let (shown, _) = quoted(&app_row(&staged).unwrap(), &|command| {
        *command == Command::Reset(sample_place())
    });
    assert_eq!(shown, titles.put_sample_rule_back);

    activate_browser(&staged, "com.apple.Safari");
    report_host(&staged, 1, SITE);
    let (shown, _) = quoted(&site_row(&staged).unwrap(), &rule_for(domain(SITE)));
    assert_eq!(Some(shown), titles.add_site_rule);

    choose(&staged, Command::Off);
    report(&staged, false);
    bring(&staged, &mail_app());
    let (shown, _) = quoted(&app_row(&staged).unwrap(), &rule_for(mail()));
    assert_eq!(shown, titles.add_example_rule);
}
