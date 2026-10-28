#![cfg(debug_assertions)]
//! What the CLI's queries and commands answer.

mod common;

use nightcap_core::{AppCommand, Fact, Staged};
use nightcap_protocol::{
    Command, Decision, Length, NightShift, NotificationPermission, OnOff, Payload, Permissions,
    Place, Reason, Response, RuleAdded, RuleIdentity, Schedule, SetBy,
};

use common::{
    ad_hoc, add_rule, app_rule, at, noon, on_by_the_schedule, reported, rules, send, yours,
};

#[test]
fn night_shift_is_answered_as_last_observed() {
    let staged = noon().stage().unwrap();

    staged.report(reported(true, Schedule::Off));

    assert_eq!(staged.core().night_shift().state, NightShift::On);
}

#[test]
fn the_strength_is_answered_as_last_read() {
    let staged = noon().stage().unwrap();
    assert_eq!(staged.core().night_shift().strength, None);

    staged.report(Fact::StrengthChanged { value: 60 });

    assert_eq!(staged.core().night_shift().strength, Some(60));
}

#[test]
fn a_command_answers_with_the_resulting_decision() {
    let staged = on_by_the_schedule().stage().unwrap();

    let (_, response) = send(&staged, Command::Off);

    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: Place::Everywhere,
            reasons: vec![
                Reason::Rule(yours(Place::Everywhere, OnOff::Off, Length::Indefinite)),
                Reason::Schedule {
                    state: OnOff::On,
                    until: Some(at("2026-10-17T07:00:00+02:00")),
                },
            ],
        }))
    );
}

#[test]
fn an_added_domain_is_normalized() {
    let staged = noon().stage().unwrap();
    let add = r#"{"command":{"add_rule":{"identity":"domain:Figma.COM","says":"off"}}}"#;

    staged.request(serde_json::from_str(add).unwrap());

    assert_eq!(
        rules(&staged),
        vec![ad_hoc(RuleIdentity::Domain("figma.com".parse().unwrap()))]
    );
}

#[test]
fn adding_a_rule_answers_whether_it_was_new_changed_or_already_said_so() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    let figma = || RuleIdentity::App("com.figma.Desktop".parse().unwrap());
    let site = || RuleIdentity::Domain("figma.com".parse().unwrap());
    let added = |staged: &Staged, identity, says| {
        let (_, response) = send(staged, Command::AddRule { identity, says });
        let Response::Ok(Payload::RuleAdded { rule, .. }) = response else {
            panic!("expected what adding did, got {response:?}");
        };
        rule
    };

    assert_eq!(added(&staged, site(), OnOff::Off), RuleAdded::New);
    assert_eq!(
        added(&staged, site(), OnOff::Off),
        RuleAdded::AlreadySays { set_by: SetBy::You }
    );
    assert_eq!(
        added(&staged, site(), OnOff::On),
        RuleAdded::Changed { set_by: SetBy::You }
    );
    assert_eq!(
        added(&staged, figma(), OnOff::Off),
        RuleAdded::AlreadySays {
            set_by: SetBy::Config
        }
    );
    let books = || RuleIdentity::App("com.apple.iBooksX".parse().unwrap());
    staged.send_app(AppCommand::StartTour);
    assert_eq!(added(&staged, books(), OnOff::Off), RuleAdded::New);
    assert_eq!(
        added(&staged, books(), OnOff::On),
        RuleAdded::Changed {
            set_by: SetBy::Tour
        }
    );
}

#[test]
fn the_rules_query_lists_every_rule_with_its_source() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();

    send(
        &staged,
        add_rule(RuleIdentity::Domain("figma.com".parse().unwrap())),
    );

    assert_eq!(
        rules(&staged),
        vec![
            app_rule("com.figma.Desktop"),
            ad_hoc(RuleIdentity::Domain("figma.com".parse().unwrap())),
        ]
    );
}

#[test]
fn the_checks_answer_the_last_permission_change() {
    let staged = noon().stage().unwrap();

    staged.report(Fact::PermissionsChanged {
        accessibility: true,
        notifications: NotificationPermission::Denied,
    });

    assert_eq!(
        staged.core().checks().permissions,
        Permissions {
            accessibility: true,
            notifications: NotificationPermission::Denied,
            login_item: None,
        }
    );
}
