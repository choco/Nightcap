#![cfg(debug_assertions)]
//! The tour: its stand-in, its rules, and what its steps follow (ADR-0016).

#![expect(
    clippy::unwrap_used,
    reason = "clippy.toml lets tests unwrap, but not the helpers beside them"
)]

mod common;

use nightcap_core::{Effect, Fact, File, Scenario, Staged, UiCommand};
use nightcap_protocol::{
    Command, Decision, ErrorCode, ExportFormat, Lasting, Length, OnOff, Payload, Place, Reason,
    Request, Response, Rule, RuleIdentity, SetBy, TourState,
};

use common::{
    activate, activate_browser_event, ad_hoc, add_rule, app_rule, at, figma, figma_app,
    kept_ad_hoc, late, minutes, off_by_the_schedule, on_by_the_schedule, refusal, report,
    report_host, resettable, rules, send, set, until, until_turn, yours,
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

fn start_tour(staged: &Staged) -> Vec<Effect> {
    staged.send_ui(UiCommand::StartTour).effects
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

// What the tour's steps follow, through the queries (ADR-0016).

fn sample_place() -> Place {
    Place::App(SAMPLE.parse().unwrap())
}

#[test]
fn no_tour_runs_before_it_starts_or_once_it_ends() {
    let staged = on_by_the_schedule().stage().unwrap();
    assert_eq!(staged.core().tour(), None);

    start_tour(&staged);
    assert!(staged.core().tour().is_some());
    staged.send_ui(UiCommand::EndTour);
    assert_eq!(staged.core().tour(), None);

    start_tour(&staged);
    staged.stop();
    assert_eq!(staged.core().tour(), None);
}

#[test]
fn a_snooze_of_the_samples_rule_ends_while_the_sample_is_in_the_background() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    activate(&staged, SAMPLE);
    staged.send(add_rule(sample())).1.unwrap();
    staged
        .send(set(sample_place(), OnOff::On, minutes(30)))
        .1
        .unwrap();
    activate(&staged, "com.apple.mail");
    assert_eq!(
        resettable(&staged),
        vec![yours(
            sample_place(),
            OnOff::On,
            until("2026-10-16T23:30:00+02:00")
        )]
    );

    staged.set_now(at("2026-10-16T23:30:00+02:00"));
    staged.report(Fact::TimerFired);

    assert_eq!(resettable(&staged), Vec::new());
    assert_eq!(rules(&staged), vec![sample_rule()]);
}

#[test]
fn a_rule_and_a_snooze_for_the_sample_from_the_cli_are_the_tours_to_follow() {
    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);

    send(&staged, add_rule(sample()));
    assert_eq!(rules(&staged), vec![sample_rule()]);
    send(
        &staged,
        set(sample_place(), OnOff::On, Lasting::UntilScheduleTurns),
    );

    assert_eq!(
        resettable(&staged),
        vec![yours(
            sample_place(),
            OnOff::On,
            until_turn("2026-10-17T07:00:00+02:00")
        )]
    );
    assert_eq!(rules(&staged), vec![sample_rule()]);
}

/// At 23:00 with Night Shift on by the custom hours and website checking on, the tour started
/// and Safari in front.
fn tour_in_safari() -> Scenario {
    on_by_the_schedule()
        .config("websites = true\n")
        .then(UiCommand::StartTour)
        .then(activate_browser_event("com.apple.Safari"))
}

#[test]
fn the_site_is_in_front_on_its_host_and_its_www_and_not_on_another_tab() {
    let staged = tour_in_safari().stage().unwrap();
    let site = || staged.core().front().site.map(String::from);
    assert_eq!(site(), None);

    report_host(&staged, 1, SITE);
    assert_eq!(site().as_deref(), Some(SITE));
    report_host(&staged, 1, "www.nightcap.choco.me");
    assert_eq!(site().as_deref(), Some(SITE));
    report_host(&staged, 1, "example.com");
    assert_eq!(site().as_deref(), Some("example.com"));
    assert_eq!(
        staged.core().front().app,
        Some("com.apple.Safari".parse().unwrap())
    );
}

#[test]
fn the_tour_stands_in_as_night_shift_was_when_it_started_whatever_the_switch_does() {
    let everywhere_off = |staged: &Staged| {
        let decision = staged.core().front().decision;
        (decision.place, decision.state) == (Place::Everywhere, Some(OnOff::Off))
    };
    let staged = off_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    report(&staged, true);
    assert_eq!(staged.core().tour(), Some(TourState { stands_in: true }));
    send(&staged, Command::Toggle);
    report(&staged, false);
    assert_eq!(staged.core().tour(), Some(TourState { stands_in: true }));
    assert!(everywhere_off(&staged));

    let staged = on_by_the_schedule().stage().unwrap();
    start_tour(&staged);
    assert_eq!(staged.core().tour(), Some(TourState { stands_in: false }));
    send(&staged, Command::Toggle);
    report(&staged, false);
    assert_eq!(staged.core().tour(), Some(TourState { stands_in: false }));
    assert!(everywhere_off(&staged));
}
