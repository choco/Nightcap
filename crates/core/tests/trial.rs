#![cfg(debug_assertions)]
//! A trial: its stand-in, its rules, what its end puts back, and what it answers (ADR-0016).

#![expect(
    clippy::unwrap_used,
    reason = "clippy.toml lets tests unwrap, but not the helpers beside them"
)]

mod common;

use nightcap_core::{AppCommand, Effect, Fact, File, Scenario, Staged};
use nightcap_protocol::{
    Command, Decision, ExportFormat, Lasting, Length, OnOff, Payload, Place, Reason, Refusal,
    Request, Response, Rule, RuleAdded, RuleIdentity, SetBy, TrialState,
};

use common::{
    activate, activate_browser_event, ad_hoc, add_rule, app_rule, at, figma, figma_app,
    kept_ad_hoc, late, minutes, off_by_the_schedule, on_by_the_schedule, refusal, report,
    report_host, resettable, rules, send, set, until, until_turn, yours,
};

/// The app the trials here give a rule.
const NOTES: &str = "com.apple.Notes";

fn notes() -> RuleIdentity {
    RuleIdentity::App(NOTES.parse().unwrap())
}

fn notes_app() -> Place {
    Place::App(NOTES.parse().unwrap())
}

/// Notes' rule added during a trial, which says off.
fn notes_rule() -> Rule {
    Rule {
        place: notes_app(),
        says: OnOff::Off,
        length: Length::Indefinite,
        set_by: SetBy::Trial,
    }
}

const SITE: &str = "docs.example.org";

fn begin_trial(staged: &Staged, stands_in: bool) -> Vec<Effect> {
    staged
        .send_app(AppCommand::BeginTrial { stands_in })
        .effects
}

fn end_trial(staged: &Staged) -> Vec<Effect> {
    staged.send_app(AppCommand::EndTrial).effects
}

/// Night Shift on by the stand-in, over the custom hours' off until 22:00.
fn on_by_the_stand_in() -> Decision {
    Decision {
        state: Some(OnOff::On),
        place: Place::Everywhere,
        reasons: vec![
            Reason::StandIn,
            Reason::Schedule {
                state: OnOff::Off,
                until: Some(at("2026-10-16T22:00:00+02:00")),
            },
        ],
    }
}

#[test]
fn a_trial_begun_with_the_stand_in_while_night_shift_is_off_turns_it_on() {
    let staged = off_by_the_schedule().stage().unwrap();

    assert_eq!(
        begin_trial(&staged, true),
        vec![Effect::SetNightShift { enabled: true }]
    );
    assert_eq!(report(&staged, true), Vec::new());

    assert_eq!(
        staged.core().snapshot().front().decision,
        on_by_the_stand_in()
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_trial_begun_without_the_stand_in_while_night_shift_is_off_leaves_it_off() {
    let staged = off_by_the_schedule().stage().unwrap();

    assert_eq!(begin_trial(&staged, false), Vec::new());

    assert_eq!(
        staged.core().snapshot().front().decision.reasons,
        vec![Reason::Schedule {
            state: OnOff::Off,
            until: Some(at("2026-10-16T22:00:00+02:00")),
        }]
    );
    assert_eq!(
        staged.core().snapshot().trial(),
        Some(TrialState { stands_in: false })
    );
    assert_eq!(end_trial(&staged), Vec::new());
}

#[test]
fn the_trials_end_turns_night_shift_back_off_following_the_schedule() {
    let staged = off_by_the_schedule().stage().unwrap();
    begin_trial(&staged, true);
    report(&staged, true);

    assert_eq!(
        end_trial(&staged),
        vec![Effect::SetNightShift { enabled: false }]
    );
    assert_eq!(report(&staged, false), Vec::new());

    assert_eq!(
        staged.core().snapshot().front().decision.reasons,
        vec![Reason::Schedule {
            state: OnOff::Off,
            until: Some(at("2026-10-16T22:00:00+02:00")),
        }]
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn the_stand_in_sets_aside_everywheres_indefinite_rule_and_the_end_puts_it_back() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    report(&staged, false);

    assert_eq!(
        begin_trial(&staged, true),
        vec![Effect::SetNightShift { enabled: true }]
    );
    report(&staged, true);
    assert_eq!(
        end_trial(&staged),
        vec![Effect::SetNightShift { enabled: false }]
    );

    assert_eq!(
        resettable(&staged),
        vec![yours(Place::Everywhere, OnOff::Off, Length::Indefinite)]
    );
}

#[test]
fn the_stand_in_sets_aside_everywheres_rule_until_a_time_and_the_end_puts_it_back_with_its_end() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, set(Place::Everywhere, OnOff::Off, minutes(60)));
    report(&staged, false);

    assert_eq!(
        begin_trial(&staged, true),
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    report(&staged, true);
    assert_eq!(
        end_trial(&staged),
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T00:00:00+02:00").into()
            },
        ]
    );

    assert_eq!(
        staged.core().snapshot().front().decision.reasons[0],
        Reason::Rule(yours(
            Place::Everywhere,
            OnOff::Off,
            until("2026-10-17T00:00:00+02:00")
        ))
    );
}

#[test]
fn while_a_trial_stands_in_a_tap_off_and_on_returns_to_its_on() {
    let staged = off_by_the_schedule().stage().unwrap();
    begin_trial(&staged, true);
    report(&staged, true);

    let (effects, _) = send(&staged, Command::Toggle);
    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    report(&staged, false);
    let (effects, response) = send(&staged, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(on_by_the_stand_in()))
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_trial_begun_without_the_stand_in_while_night_shift_is_on_changes_nothing() {
    let staged = on_by_the_schedule().stage().unwrap();

    assert_eq!(begin_trial(&staged, false), Vec::new());
    assert!(matches!(
        staged.core().snapshot().front().decision.reasons[..],
        [Reason::Schedule { .. }]
    ));
    assert_eq!(end_trial(&staged), Vec::new());
}

#[test]
fn a_rule_added_mid_trial_forces_night_shift_off_while_its_app_is_in_front() {
    let staged = on_by_the_schedule().stage().unwrap();
    begin_trial(&staged, false);
    assert_eq!(activate(&staged, NOTES), Vec::new());

    let (effects, response) = send(&staged, add_rule(notes()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(
        response,
        Response::Ok(Payload::RuleAdded {
            rule: RuleAdded::New {
                set_by: SetBy::Trial
            },
            decision: Decision {
                state: Some(OnOff::Off),
                place: notes_app(),
                reasons: vec![Reason::Rule(notes_rule())],
            },
        })
    );
    report(&staged, false);
    assert_eq!(
        activate(&staged, "com.apple.mail"),
        vec![Effect::SetNightShift { enabled: true }]
    );
}

#[test]
fn turning_a_trial_rule_the_other_way_mid_trial_answers_that_the_trials_rule_changed() {
    let staged = on_by_the_schedule().stage().unwrap();
    begin_trial(&staged, false);
    send(&staged, add_rule(notes()));

    let (_, response) = send(
        &staged,
        Command::AddRule {
            identity: notes(),
            says: OnOff::On,
        },
    );

    assert!(
        matches!(
            response,
            Response::Ok(Payload::RuleAdded {
                rule: RuleAdded::Changed {
                    set_by: SetBy::Trial
                },
                ..
            })
        ),
        "{response:?}"
    );
}

#[test]
fn once_a_trial_ends_the_rule_added_during_it_matches_nothing() {
    let staged = on_by_the_schedule().stage().unwrap();
    begin_trial(&staged, false);
    send(&staged, add_rule(notes()));
    activate(&staged, "com.apple.mail");
    end_trial(&staged);

    assert_eq!(activate(&staged, NOTES), Vec::new());
}

#[test]
fn removing_a_trial_rule_mid_trial_drops_it() {
    let staged = on_by_the_schedule().stage().unwrap();
    begin_trial(&staged, false);
    activate(&staged, NOTES);
    send(&staged, add_rule(notes()));
    report(&staged, false);

    let (effects, response) = send(&staged, Command::RemoveRule(notes()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert!(matches!(response, Response::Ok(_)), "{response:?}");
    assert_eq!(rules(&staged), Vec::new());
}

#[test]
fn rules_added_mid_trial_stay_out_of_exports_and_files_and_go_with_their_snoozes() {
    let staged = on_by_the_schedule().stage().unwrap();
    begin_trial(&staged, false);
    let site = RuleIdentity::Domain("choco.me".parse().unwrap());
    for identity in [notes(), site.clone()] {
        let (_, response) = send(&staged, add_rule(identity));
        assert!(matches!(response, Response::Ok(_)), "{response:?}");
    }
    let (_, response) = send(
        &staged,
        set(notes_app(), OnOff::On, Lasting::UntilScheduleTurns),
    );
    assert!(matches!(response, Response::Ok(_)), "{response:?}");

    assert_eq!(
        rules(&staged),
        vec![
            notes_rule(),
            Rule {
                place: Place::from(site),
                ..notes_rule()
            },
        ]
    );
    assert!(
        !staged
            .core()
            .snapshot()
            .export(ExportFormat::Toml)
            .contains(NOTES)
    );
    let kept = staged.file(File::Rules).unwrap_or_default();
    assert!(
        !kept.contains(NOTES) && !kept.contains("choco.me"),
        "{kept}"
    );
    let saved = staged.file(File::State).unwrap();
    assert!(!saved.contains(NOTES), "{saved}");

    end_trial(&staged);
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn adding_a_rule_that_exists_mid_trial_leaves_it_as_it_is() {
    let staged = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    report(&staged, true);
    begin_trial(&staged, false);

    send(&staged, add_rule(figma()));

    assert_eq!(rules(&staged), vec![app_rule("com.figma.Desktop")]);
    end_trial(&staged);
    assert_eq!(rules(&staged), vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn adding_the_rule_a_snoozed_app_has_mid_trial_ends_the_snooze_only_while_it_runs() {
    let staged = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    report(&staged, true);
    send(&staged, set(figma_app(), OnOff::On, minutes(30)));
    begin_trial(&staged, false);

    send(&staged, add_rule(figma()));
    assert_eq!(resettable(&staged), Vec::new());
    end_trial(&staged);

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
fn a_config_rule_reloaded_mid_trial_replaces_the_trials_rule_for_its_app() {
    let staged = on_by_the_schedule().stage().unwrap();
    begin_trial(&staged, false);
    send(&staged, add_rule(notes()));
    staged.write_config(Some(format!("[rules.on]\napp = [\"{NOTES}\"]\n").as_str()));

    staged.request(Request::Command(Command::Reload));

    let config_rule = Rule {
        says: OnOff::On,
        set_by: SetBy::Config,
        ..notes_rule()
    };
    assert_eq!(rules(&staged), vec![config_rule]);
    let (_, response) = send(&staged, Command::RemoveRule(notes()));
    assert_eq!(refusal(response), Refusal::ConfigRule { identity: notes() });
}

#[test]
fn removing_your_own_rule_mid_trial_lasts_only_while_it_runs() {
    let staged = on_by_the_schedule().stage().unwrap();
    let mail = RuleIdentity::App("com.apple.mail".parse().unwrap());
    send(&staged, add_rule(mail.clone()));
    begin_trial(&staged, false);

    send(&staged, Command::RemoveRule(mail.clone()));
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(kept_ad_hoc(&staged), vec![ad_hoc(mail.clone())]);
    end_trial(&staged);

    assert_eq!(rules(&staged), vec![ad_hoc(mail)]);
}

#[test]
fn turning_your_own_rule_the_other_way_mid_trial_lasts_only_while_it_runs() {
    let staged = on_by_the_schedule().stage().unwrap();
    let mail = RuleIdentity::App("com.apple.mail".parse().unwrap());
    send(&staged, add_rule(mail.clone()));
    begin_trial(&staged, false);

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
            .snapshot()
            .export(ExportFormat::Toml)
            .contains("[rules.off]")
    );
    end_trial(&staged);

    assert_eq!(rules(&staged), vec![ad_hoc(mail)]);
}

#[test]
fn snoozes_and_the_switch_mid_trial_last_only_while_it_runs() {
    let staged = late()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    report(&staged, true);
    begin_trial(&staged, false);

    send(&staged, set(figma_app(), OnOff::On, minutes(30)));
    send(&staged, Command::Off);
    assert_eq!(resettable(&staged).len(), 2);
    end_trial(&staged);

    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_restart_after_a_crash_mid_trial_puts_night_shift_back_where_the_trial_found_it() {
    let crashed = on_by_the_schedule().stage().unwrap();
    send(&crashed, Command::Off);
    report(&crashed, false);
    begin_trial(&crashed, true);
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
fn quitting_mid_trial_ends_it() {
    let staged = off_by_the_schedule().stage().unwrap();
    begin_trial(&staged, true);
    report(&staged, true);

    assert_eq!(
        staged.stop().effects,
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn beginning_a_trial_while_one_runs_keeps_the_first_and_what_it_set_aside() {
    let staged = on_by_the_schedule().stage().unwrap();
    send(&staged, Command::Off);
    report(&staged, false);
    begin_trial(&staged, true);
    report(&staged, true);

    assert_eq!(begin_trial(&staged, false), Vec::new());
    assert_eq!(
        staged.core().snapshot().trial(),
        Some(TrialState { stands_in: true })
    );
    end_trial(&staged);

    assert_eq!(
        resettable(&staged),
        vec![yours(Place::Everywhere, OnOff::Off, Length::Indefinite)]
    );
}

// What a trial answers, through the queries (ADR-0016).

#[test]
fn no_trial_runs_before_it_begins_or_once_it_ends() {
    let staged = on_by_the_schedule().stage().unwrap();
    assert_eq!(staged.core().snapshot().trial(), None);

    begin_trial(&staged, false);
    assert!(staged.core().snapshot().trial().is_some());
    end_trial(&staged);
    assert_eq!(staged.core().snapshot().trial(), None);

    begin_trial(&staged, false);
    staged.stop();
    assert_eq!(staged.core().snapshot().trial(), None);
}

#[test]
fn a_snooze_of_a_trial_rule_ends_while_its_app_is_in_the_background() {
    let staged = on_by_the_schedule().stage().unwrap();
    begin_trial(&staged, false);
    activate(&staged, NOTES);
    staged.send(add_rule(notes())).1.unwrap();
    staged
        .send(set(notes_app(), OnOff::On, minutes(30)))
        .1
        .unwrap();
    activate(&staged, "com.apple.mail");
    assert_eq!(
        resettable(&staged),
        vec![yours(
            notes_app(),
            OnOff::On,
            until("2026-10-16T23:30:00+02:00")
        )]
    );

    staged.set_now(at("2026-10-16T23:30:00+02:00"));
    staged.report(Fact::TimerFired);

    assert_eq!(resettable(&staged), Vec::new());
    assert_eq!(rules(&staged), vec![notes_rule()]);
}

#[test]
fn mid_trial_a_rule_from_the_cli_is_the_trials_and_its_snooze_the_users() {
    let staged = on_by_the_schedule().stage().unwrap();
    begin_trial(&staged, false);

    send(&staged, add_rule(notes()));
    assert_eq!(rules(&staged), vec![notes_rule()]);
    send(
        &staged,
        set(notes_app(), OnOff::On, Lasting::UntilScheduleTurns),
    );

    assert_eq!(
        resettable(&staged),
        vec![yours(
            notes_app(),
            OnOff::On,
            until_turn("2026-10-17T07:00:00+02:00")
        )]
    );
    assert_eq!(rules(&staged), vec![notes_rule()]);
}

/// At 23:00 with Night Shift on by the custom hours and website checking on, a trial begun and
/// Safari in front.
fn trial_in_safari() -> Scenario {
    on_by_the_schedule()
        .config("websites = true\n")
        .then(AppCommand::BeginTrial { stands_in: false })
        .then(activate_browser_event("com.apple.Safari"))
}

#[test]
fn mid_trial_the_site_in_front_is_the_tabs_host_short_of_www() {
    let staged = trial_in_safari().stage().unwrap();
    let site = || staged.core().snapshot().front().site.map(String::from);
    assert_eq!(site(), None);

    report_host(&staged, 1, SITE);
    assert_eq!(site().as_deref(), Some(SITE));
    report_host(&staged, 1, &format!("www.{SITE}"));
    assert_eq!(site().as_deref(), Some(SITE));
    report_host(&staged, 1, "example.com");
    assert_eq!(site().as_deref(), Some("example.com"));
    assert_eq!(
        staged.core().snapshot().front().app,
        Some("com.apple.Safari".parse().unwrap())
    );
}

#[test]
fn a_trial_stands_in_as_it_was_asked_whatever_the_switch_does() {
    let everywhere_off = |staged: &Staged| {
        let decision = staged.core().snapshot().front().decision;
        (decision.place, decision.state) == (Place::Everywhere, Some(OnOff::Off))
    };
    let staged = off_by_the_schedule().stage().unwrap();
    begin_trial(&staged, true);
    report(&staged, true);
    assert_eq!(
        staged.core().snapshot().trial(),
        Some(TrialState { stands_in: true })
    );
    send(&staged, Command::Toggle);
    report(&staged, false);
    assert_eq!(
        staged.core().snapshot().trial(),
        Some(TrialState { stands_in: true })
    );
    assert!(everywhere_off(&staged));

    let staged = on_by_the_schedule().stage().unwrap();
    begin_trial(&staged, false);
    assert_eq!(
        staged.core().snapshot().trial(),
        Some(TrialState { stands_in: false })
    );
    send(&staged, Command::Toggle);
    report(&staged, false);
    assert_eq!(
        staged.core().snapshot().trial(),
        Some(TrialState { stands_in: false })
    );
    assert!(everywhere_off(&staged));
}
