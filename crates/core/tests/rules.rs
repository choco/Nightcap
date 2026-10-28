#![cfg(debug_assertions)]
//! Rules: the config's and the ad-hoc ones, their snoozes, and graduation (ADR-0006, ADR-0007).

mod common;

use nightcap_core::{Effect, Event, Fact, File};
use nightcap_protocol::{
    Check, CheckDetail, CheckKind, Command, ConfigFileDetail, Decision, ExportFormat, FileError,
    Lasting, Length, OnOff, Payload, Place, Problem, Reason, Refusal, Request, Response, Rule,
    RuleAdded, RuleIdentity, SetBy,
};

use common::{
    activate, activate_event, ad_hoc, add_rule, app_rule, at, books, custom_hours, domain_rule,
    figma, figma_app, forced, in_books_at_noon, kept_ad_hoc, late, launch, mail, minutes, noon,
    on_by_the_schedule, refusal, reported, resettable, rules, send, set, until_turn, yours,
};

#[test]
fn a_config_rule_turns_night_shift_off_while_its_app_is_frontmost() {
    let staged = on_by_the_schedule()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();

    let effects = activate(&staged, "com.figma.Desktop");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(
        staged.core().front().app,
        Some("com.figma.Desktop".parse().unwrap())
    );
    assert_eq!(
        staged.core().front().decision,
        Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }
    );
}

#[test]
fn leaving_the_app_hands_night_shift_back_to_the_schedule() {
    let staged = on_by_the_schedule()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();
    activate(&staged, "com.figma.Desktop");
    staged.report(reported(false, custom_hours()));

    let effects = activate(&staged, "com.apple.Terminal");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
}

#[test]
fn an_invalid_config_at_launch_runs_without_config_rules_and_says_so() {
    let staged = noon()
        .config("[rules.off]\napps = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();

    let outcome = launch(&staged);

    let [
        Event::ProblemAppeared {
            check:
                Check {
                    detail:
                        CheckDetail::ConfigFile(ConfigFileDetail::Error(FileError::Malformed {
                            message,
                            ..
                        })),
                    ..
                },
        },
    ] = outcome.events.as_slice()
    else {
        panic!("expected one announcement, got {outcome:?}");
    };
    assert!(message.contains("apps"), "{message}");
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(
        staged.core().config().error,
        Some(FileError::Malformed {
            path: staged.core().config().path,
            line: Some(2),
            message: message.clone(),
        })
    );
}

#[test]
fn reload_applies_a_changed_config() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate(&staged, "com.figma.Desktop");

    staged.write_config(Some("[rules.off]\napp = [\"com.figma.Desktop\"]\n"));
    let (outcome, response) = staged.request(Request::Reload);

    assert_eq!(
        outcome.effects,
        vec![Effect::SetNightShift { enabled: false }]
    );
    assert_eq!(
        response,
        Response::Ok(Payload::Rules(vec![app_rule("com.figma.Desktop")]))
    );
}

#[test]
fn reloading_an_invalid_config_is_refused_and_keeps_the_rules_in_force() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();

    staged.write_config(Some("[rules.off]\napp = \"com.figma.Desktop\"\n"));
    let (outcome, response) = staged.request(Request::Reload);

    assert_eq!(outcome.effects, Vec::new());
    let malformed = FileError::Malformed {
        path: staged.core().config().path,
        line: Some(2),
        message: "invalid type: string \"com.figma.Desktop\", expected a sequence".to_owned(),
    };
    assert_eq!(refusal(response), Refusal::InvalidConfig(malformed));
    assert_eq!(rules(&staged), vec![app_rule("com.figma.Desktop")]);
    assert_eq!(
        staged.core().config().error,
        Some(FileError::Malformed {
            path: staged.core().config().path,
            line: Some(2),
            message: "invalid type: string \"com.figma.Desktop\", expected a sequence".to_owned(),
        })
    );
}

#[test]
fn setting_a_rules_place_the_other_way_snoozes_the_rule_for_that_length() {
    let staged = forced();

    let (effects, response) = send(
        &staged,
        set(figma_app(), OnOff::On, Lasting::UntilScheduleTurns),
    );

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::On),
            place: figma_app(),
            reasons: vec![
                Reason::Rule(yours(
                    figma_app(),
                    OnOff::On,
                    until_turn("2026-10-17T07:00:00+02:00")
                )),
                Reason::Rule(app_rule("com.figma.Desktop")),
            ],
        }))
    );
}

#[test]
fn a_snoozed_rule_comes_back_when_its_snooze_ends() {
    let staged = forced();
    send(&staged, set(figma_app(), OnOff::On, minutes(30)));
    staged.report(reported(true, custom_hours()));

    staged.set_now(at("2026-10-16T23:30:00+02:00"));
    let effects = staged.report(Fact::TimerFired).effects;

    assert!(effects.contains(&Effect::SetNightShift { enabled: false }));
    assert_eq!(resettable(&staged), Vec::new());
    assert_eq!(staged.core().front().decision.place, figma_app());
}

#[test]
fn a_snooze_stays_with_its_rule_when_another_app_comes_to_the_front() {
    let staged = forced();
    send(
        &staged,
        set(figma_app(), OnOff::On, Lasting::UntilScheduleTurns),
    );
    staged.report(reported(true, custom_hours()));

    assert_eq!(activate(&staged, "com.apple.Terminal"), Vec::new());
    assert_eq!(activate(&staged, "com.figma.Desktop"), Vec::new());
    assert_eq!(staged.core().front().decision.state, Some(OnOff::On));
}

#[test]
fn reset_puts_a_snoozed_rule_back_early() {
    let staged = forced();
    send(
        &staged,
        set(figma_app(), OnOff::On, Lasting::UntilScheduleTurns),
    );

    let (_, response) = send(&staged, Command::Reset(figma_app()));

    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }))
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn adding_the_rule_a_snoozed_app_already_has_ends_the_snooze() {
    let staged = forced();
    send(&staged, set(figma_app(), OnOff::On, minutes(30)));
    staged.report(reported(true, custom_hours()));

    let (effects, response) = send(&staged, add_rule(figma()));

    assert!(effects.contains(&Effect::SetNightShift { enabled: false }));
    assert_eq!(
        response,
        Response::Ok(Payload::RuleAdded {
            rule: RuleAdded::AlreadySays {
                set_by: SetBy::Config
            },
            decision: Decision {
                state: Some(OnOff::Off),
                place: figma_app(),
                reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
            },
        })
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn domains_in_the_config_are_normalized_and_checked() {
    let staged = noon()
        .config("[rules.off]\ndomain = [\"Bücher.DE.\"]\n")
        .stage()
        .unwrap();

    assert_eq!(rules(&staged), vec![domain_rule("xn--bcher-kva.de")]);
    for entry in [
        "https://figma.com",
        "figma.com/files",
        "figma.com:443",
        "*.figma.com",
        "",
    ] {
        staged.write_config(Some(
            format!("[rules.off]\ndomain = [{entry:?}]\n").as_str(),
        ));
        let (_, response) = staged.request(Request::Reload);
        assert!(
            matches!(&response, Response::Refused(Refusal::InvalidConfig(_))),
            "{entry:?} was accepted: {response:?}"
        );
    }
}

#[test]
fn an_added_rule_applies_at_once_and_survives_a_restart() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate(&staged, "com.figma.Desktop");

    let (effects, response) = send(&staged, add_rule(figma()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(
        response,
        Response::Ok(Payload::RuleAdded {
            rule: RuleAdded::New,
            decision: Decision {
                state: Some(OnOff::Off),
                place: figma_app(),
                reasons: vec![Reason::Rule(ad_hoc(figma()))],
            },
        })
    );
    let staged = staged.restart_at(at("2026-10-16T23:10:00+02:00"));
    assert_eq!(rules(&staged), vec![ad_hoc(figma())]);
}

#[test]
fn a_rule_the_app_adds_applies_at_once() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate(&staged, "com.figma.Desktop");

    let effects = staged.send(add_rule(figma())).0.effects;

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
}

#[test]
fn removing_an_ad_hoc_rule_lifts_it() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate(&staged, "com.figma.Desktop");
    send(&staged, add_rule(figma()));
    staged.report(reported(false, custom_hours()));

    let (effects, _) = send(&staged, Command::RemoveRule(figma()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(rules(&staged), Vec::new());
}

#[test]
fn a_rule_that_doesnt_exist_is_refused() {
    let staged = noon().stage().unwrap();

    let (_, removed) = send(&staged, Command::RemoveRule(figma()));

    assert_eq!(refusal(removed), Refusal::NoSuchRule { identity: figma() });
}

#[test]
fn ad_hoc_rules_from_a_newer_build_are_left_alone() {
    let newer = "version = 3\n[rules.off]\napp = [\"com.figma.Desktop\"]\n";
    let staged = noon().rules(newer).stage().unwrap();

    let (_, response) = send(&staged, add_rule(figma()));

    let why = refusal(response);
    assert!(
        matches!(
            &why,
            Refusal::AdHocRulesOff(FileError::Newer { version: 3, .. })
        ),
        "{why:?}"
    );
    assert_eq!(
        Some(why),
        staged
            .core()
            .config()
            .ad_hoc_error
            .map(Refusal::AdHocRulesOff)
    );
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(staged.file(File::Rules).as_deref(), Some(newer));
}

#[test]
fn ad_hoc_rules_with_an_app_no_rule_could_name_are_left_alone_and_name_its_line() {
    let invalid = "version = 2\n[rules.off]\napp = [\n  \"com.figma.Desktop\",\n  \"com.figma.Desktop \",\n]\n";
    let staged = noon().rules(invalid).stage().unwrap();

    assert!(
        matches!(
            &staged.core().config().ad_hoc_error,
            Some(FileError::Invalid {
                line: 5,
                problem: Problem::NotABundleId { entry },
                ..
            }) if entry == "com.figma.Desktop "
        ),
        "{:?}",
        staged.core().config().ad_hoc_error
    );
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(staged.file(File::Rules).as_deref(), Some(invalid));
}

#[test]
fn an_app_whose_bundle_id_no_rule_could_name_counts_as_having_none() {
    let staged = on_by_the_schedule().stage().unwrap();

    activate(&staged, "unity.Team Cherry.Hollow Knight");

    assert_eq!(staged.core().front().app, None);
}

#[test]
fn a_config_rule_cant_be_replaced_or_removed() {
    let staged = forced();

    for (command, why) in [
        (
            Command::RemoveRule(figma()),
            Refusal::ConfigRule { identity: figma() },
        ),
        (
            Command::AddRule {
                identity: figma(),
                says: OnOff::On,
            },
            Refusal::ConfigSays {
                identity: figma(),
                says: OnOff::Off,
            },
        ),
    ] {
        let (effects, response) = send(&staged, command.clone());
        assert_eq!(effects, Vec::new(), "{command:?}");
        assert_eq!(refusal(response), why, "{command:?}");
    }
    assert_eq!(rules(&staged), vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn removing_a_config_rule_from_the_app_is_refused() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();

    let (outcome, answer) = staged.send(Command::RemoveRule(figma()));

    assert_eq!(outcome.effects, Vec::new());
    assert_eq!(answer, Err(Refusal::ConfigRule { identity: figma() }));
    assert_eq!(rules(&staged), vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn adding_a_rule_from_the_app_while_rules_toml_is_from_a_newer_nightcap_is_refused() {
    let newer = "version = 3\n[rules.off]\napp = [\"com.apple.mail\"]\n";
    let staged = late()
        .rules(newer)
        .then(reported(true, custom_hours()))
        .then(activate_event("com.figma.Desktop"))
        .stage()
        .unwrap();
    let (outcome, answer) = staged.send(add_rule(figma()));

    assert_eq!(outcome.effects, Vec::new());
    assert!(
        matches!(
            answer,
            Err(Refusal::AdHocRulesOff(FileError::Newer { version: 3, .. }))
        ),
        "{answer:?}"
    );
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(staged.file(File::Rules).as_deref(), Some(newer));
}

#[test]
fn adding_a_rule_makes_the_users_rule_say_that_way_replacing_one_that_said_the_other() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate(&staged, "com.apple.mail");
    send(&staged, add_rule(mail()));
    send(&staged, set(Place::from(mail()), OnOff::On, minutes(30)));

    let (_, response) = send(
        &staged,
        Command::AddRule {
            identity: mail(),
            says: OnOff::On,
        },
    );

    assert!(
        matches!(
            response,
            Response::Ok(Payload::RuleAdded {
                rule: RuleAdded::Changed { set_by: SetBy::You },
                ..
            })
        ),
        "{response:?}"
    );
    assert_eq!(
        rules(&staged),
        vec![Rule {
            says: OnOff::On,
            ..ad_hoc(mail())
        }]
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn adding_your_rule_again_ends_its_snooze_and_leaves_rules_toml_as_it_is() {
    let written = "version = 2\n[rules.off]\napp = [\"com.apple.mail\"]\n";
    let staged = on_by_the_schedule().rules(written).stage().unwrap();
    activate(&staged, "com.apple.mail");
    staged.report(reported(false, custom_hours()));
    staged
        .send(set(Place::from(mail()), OnOff::On, minutes(30)))
        .1
        .unwrap();
    staged.report(reported(true, custom_hours()));

    let (outcome, answer) = staged.send(add_rule(mail()));

    assert!(
        outcome
            .effects
            .contains(&Effect::SetNightShift { enabled: false })
    );
    assert!(
        matches!(
            &answer,
            Ok(Payload::RuleAdded {
                rule: RuleAdded::AlreadySays { set_by: SetBy::You },
                decision,
            }) if decision.place == Place::from(mail())
        ),
        "{answer:?}"
    );
    assert_eq!(resettable(&staged), Vec::new());
    assert_eq!(staged.file(File::Rules).as_deref(), Some(written));
}

#[test]
fn an_app_rule_that_says_on_turns_night_shift_on_by_day() {
    let staged = in_books_at_noon().stage().unwrap();

    let (effects, _) = send(
        &staged,
        Command::AddRule {
            identity: books(),
            says: OnOff::On,
        },
    );
    staged.report(reported(true, custom_hours()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        activate(&staged, "com.apple.mail"),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn an_ad_hoc_rule_graduates_whatever_either_says() {
    let staged = noon().stage().unwrap();
    send(
        &staged,
        Command::AddRule {
            identity: figma(),
            says: OnOff::On,
        },
    );

    staged.write_config(Some("[rules.off]\napp = [\"com.figma.desktop\"]\n"));
    staged.request(Request::Reload);

    assert_eq!(rules(&staged), vec![app_rule("com.figma.desktop")]);
    assert_eq!(kept_ad_hoc(&staged), Vec::new());
}

#[test]
fn export_writes_both_lists() {
    let staged = noon().stage().unwrap();
    send(&staged, add_rule(figma()));
    send(
        &staged,
        Command::AddRule {
            identity: RuleIdentity::Domain("youtube.com".parse().unwrap()),
            says: OnOff::On,
        },
    );

    assert_eq!(
        staged.core().export(ExportFormat::Nix),
        "{ off = { app = [ \"com.figma.Desktop\" ]; domain = [ ]; }; on = { app = [ ]; domain = [ \"youtube.com\" ]; }; }\n"
    );
    assert_eq!(
        staged.core().export(ExportFormat::Toml),
        "[rules.off]\napp = [\"com.figma.Desktop\"]\n\n[rules.on]\ndomain = [\"youtube.com\"]\n"
    );
}

#[test]
fn rules_are_listed_by_what_they_say() {
    let staged = noon().config("[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.apple.iBooksX\"]\ndomain = [\"youtube.com\"]\n").stage().unwrap();

    let on = |identity| Rule {
        place: Place::from(identity),
        says: OnOff::On,
        length: Length::Indefinite,
        set_by: SetBy::Config,
    };
    assert_eq!(
        rules(&staged),
        vec![
            app_rule("com.figma.Desktop"),
            on(books()),
            on(RuleIdentity::Domain("youtube.com".parse().unwrap())),
        ]
    );
}

#[test]
fn a_config_that_cant_be_is_refused() {
    let staged = noon().stage().unwrap();

    let lengths = |lengths: &[&str]| Problem::NotLengths {
        lengths: lengths.iter().map(|length| (*length).to_owned()).collect(),
    };
    for (config, why) in [
        (
            "[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.figma.desktop\"]\n",
            Problem::BothWays { identity: figma() },
        ),
        (
            "[rules]\napp = [\"com.figma.Desktop\"]\n",
            Problem::UnsaidRules,
        ),
        (
            "lengths = [\"45m\", \"forever\"]\n",
            lengths(&["45m", "forever"]),
        ),
        ("lengths = [\"30s\"]\n", lengths(&["30s"])),
        ("lengths = []\n", lengths(&[])),
        ("[menu]\nlengths = [\"45m\"]\n", Problem::LengthsUnderMenu),
        (
            "[rules.off]\napp = [\"com.figma.Desktop \"]\n",
            Problem::NotABundleId {
                entry: "com.figma.Desktop ".to_owned(),
            },
        ),
        (
            "[rules.on]\napp = [\"\"]\n",
            Problem::NotABundleId {
                entry: String::new(),
            },
        ),
        (
            "[rules.off]\ndomain = [\"https://figma.com\"]\n",
            Problem::NotADomain {
                entry: "https://figma.com".to_owned(),
            },
        ),
    ] {
        staged.write_config(Some(config));
        let (_, response) = staged.request(Request::Reload);
        let Response::Refused(Refusal::InvalidConfig(FileError::Invalid { problem, .. })) =
            response
        else {
            panic!("{config:?} was accepted: {response:?}");
        };
        assert_eq!(problem, why, "{config:?}");
    }
}

#[test]
fn a_config_that_cant_be_names_the_line_of_what_cant_be() {
    let staged = noon().stage().unwrap();
    let path = staged.core().config().path;

    for (config, line) in [
        (
            "[rules.off]\napp = [\"com.figma.Desktop\"]\n\n[rules.on]\napp = [\n  \"com.apple.Photos\",\n  \"com.figma.desktop\",\n]\n",
            7,
        ),
        (
            "websites = true\n[rules]\napp = [\"com.figma.Desktop\"]\n",
            3,
        ),
        (
            "[rules.off]\napp = [\n  \"com.figma.Desktop\",\n  \"com.figma.Desktop \",\n]\n",
            4,
        ),
        (
            "[rules.on]\ndomain = [\"figma.com\"]\n[rules.off]\ndomain = [\"https://figma.com\"]\n",
            4,
        ),
        (
            "[night_shift]\nschedule = \"22:00-07:00\"\nstrength = 101\n",
            3,
        ),
        ("[night_shift]\nschedule = \"dusk\"\n", 2),
        ("\nlengths = [\"45m\", \"forever\"]\n", 2),
        ("websites = true\n\n[menu]\nlengths = [\"45m\"]\n", 3),
    ] {
        staged.write_config(Some(config));
        let (_, response) = staged.request(Request::Reload);

        assert!(
            matches!(
                &response,
                Response::Refused(Refusal::InvalidConfig(FileError::Invalid {
                    path: at_path,
                    line: at,
                    ..
                })) if *at == line && *at_path == path
            ),
            "{config:?}: {response:?}"
        );
        let found = staged
            .core()
            .checks()
            .checks
            .into_iter()
            .map(|check| check.detail)
            .find(|detail| detail.kind() == CheckKind::ConfigFile);
        assert!(
            matches!(
                &found,
                Some(CheckDetail::ConfigFile(ConfigFileDetail::Error(FileError::Invalid {
                    line: at,
                    ..
                }))) if *at == line
            ),
            "{config:?}: {found:?}"
        );
    }
}
