#![cfg(debug_assertions)]
//! What the CLI's queries and commands answer.

mod common;

use nightcap_core::Fact;
use nightcap_protocol::{
    Command, Decision, Length, NightShift, NotificationPermission, OnOff, Payload, Permissions,
    Place, Reason, Response, RuleIdentity, Schedule,
};

use common::{
    ad_hoc, add_rule, app_rule, at, noon, on_by_the_schedule, reported, rules, send, status, yours,
};

#[test]
fn the_status_reports_night_shift_as_last_observed() {
    let staged = noon().stage().unwrap();

    staged.report(reported(true, Schedule::Off));

    assert_eq!(status(&staged).night_shift, NightShift::On);
}

#[test]
fn the_status_reports_the_strength_as_last_read() {
    let staged = noon().stage().unwrap();
    assert_eq!(status(&staged).strength, None);

    staged.report(Fact::StrengthChanged { value: 60 });

    assert_eq!(status(&staged).strength, Some(60));
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
fn the_status_reports_the_last_permission_change() {
    let staged = noon().stage().unwrap();

    staged.report(Fact::PermissionsChanged {
        accessibility: true,
        notifications: NotificationPermission::Denied,
    });

    assert_eq!(
        status(&staged).permissions,
        Permissions {
            accessibility: true,
            notifications: NotificationPermission::Denied,
        }
    );
}
