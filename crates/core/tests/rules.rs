#![cfg(debug_assertions)]
//! Rules: the config's and the ad-hoc ones, their snoozes, and graduation (ADR-0006, ADR-0007).

mod common;

use nightcap_core::{Effect, Event, File, MenuEntry, RowImage};
use nightcap_protocol::{
    CheckDetail, CheckKind, Command, ConfigFileDetail, Decision, ErrorCode, ExportFormat,
    FileError, Length, OnOff, Payload, Place, Reason, Request, Response, Rule, RuleIdentity,
    RuleSource,
};

use common::{
    activate, activate_event, ad_hoc, add_rule, app_row, app_rule, at, books, choice_titled,
    custom_hours, domain_rule, figma, figma_app, forced, in_books_at_noon, kept_ad_hoc, late,
    launch, layer, mail, minutes, noon, on_by_the_schedule, refusal, reported, rules, send, set,
    status,
};

#[test]
fn a_config_rule_turns_night_shift_off_while_its_app_is_frontmost() {
    let staged = on_by_the_schedule()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();

    let effects = activate(&staged, "com.figma.Desktop");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    let status = status(&staged);
    assert_eq!(status.frontmost_app.as_deref(), Some("com.figma.Desktop"));
    assert_eq!(
        status.decision,
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
    staged.core().handle(reported(false, custom_hours()));

    let effects = activate(&staged, "com.apple.Terminal");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
}

#[test]
fn an_invalid_config_at_launch_runs_without_config_rules_and_says_so() {
    let staged = noon()
        .config("[rules.off]\napps = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();

    let effects = launch(&staged);

    let [Effect::Notify { body, .. }] = effects.as_slice() else {
        panic!("expected one notification, got {effects:?}");
    };
    assert!(body.contains("apps"), "{body}");
    let status = status(&staged);
    assert_eq!(status.rules, Vec::new());
    assert!(
        status
            .config_error
            .is_some_and(|error| error.contains("apps"))
    );
}

#[test]
fn reload_applies_a_changed_config() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate(&staged, "com.figma.Desktop");

    staged.write_config(Some("[rules.off]\napp = [\"com.figma.Desktop\"]\n"));
    let (effects, response) = staged.request(Request::Reload);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    let Response::Ok(Payload::Status(status)) = response else {
        panic!("expected a status, got {response:?}");
    };
    assert_eq!(status.rules, vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn reloading_an_invalid_config_is_refused_and_keeps_the_rules_in_force() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();

    staged.write_config(Some("[rules.off]\napp = \"com.figma.Desktop\"\n"));
    let (effects, response) = staged.request(Request::Reload);

    assert_eq!(effects, Vec::new());
    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    assert_eq!(error.code, ErrorCode::InvalidConfig);
    assert_eq!(
        error.message,
        format!(
            "{}: line 2: invalid type: string \"com.figma.Desktop\", expected a sequence",
            staged.core().settings().config_path
        )
    );
    let status = status(&staged);
    assert_eq!(status.rules, vec![app_rule("com.figma.Desktop")]);
    assert_eq!(status.config_error, Some(error.message));
}

#[test]
fn setting_a_rules_place_the_other_way_snoozes_the_rule_for_that_length() {
    let staged = forced();

    let (effects, response) = send(
        &staged,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::On),
            place: figma_app(),
            reasons: vec![
                Reason::Hold(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
                Reason::Rule(app_rule("com.figma.Desktop")),
            ],
        }))
    );
}

#[test]
fn a_snoozed_rule_comes_back_when_its_snooze_ends() {
    let staged = forced();
    send(&staged, set(figma_app(), OnOff::On, minutes(30)));
    staged.core().handle(reported(true, custom_hours()));

    staged.set_now(at("2026-10-16T23:30:00+02:00"));
    let effects = staged.core().handle(Event::TimerFired);

    assert!(effects.contains(&Effect::SetNightShift { enabled: false }));
    let status = status(&staged);
    assert_eq!(status.layers, Vec::new());
    assert_eq!(status.decision.place, figma_app());
}

#[test]
fn a_snooze_stays_with_its_rule_when_another_app_comes_to_the_front() {
    let staged = forced();
    send(
        &staged,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );
    staged.core().handle(reported(true, custom_hours()));

    assert_eq!(activate(&staged, "com.apple.Terminal"), Vec::new());
    assert_eq!(activate(&staged, "com.figma.Desktop"), Vec::new());
    assert_eq!(status(&staged).decision.state, Some(OnOff::On));
}

#[test]
fn reset_puts_a_snoozed_rule_back_early() {
    let staged = forced();
    send(
        &staged,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
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
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn adding_the_rule_a_snoozed_app_already_has_ends_the_snooze() {
    let staged = forced();
    send(&staged, set(figma_app(), OnOff::On, minutes(30)));
    staged.core().handle(reported(true, custom_hours()));

    let (effects, response) = send(&staged, add_rule(figma()));

    assert!(effects.contains(&Effect::SetNightShift { enabled: false }));
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }))
    );
    assert_eq!(status(&staged).layers, Vec::new());
}

#[test]
fn domains_in_the_config_are_normalized_and_checked() {
    let staged = noon()
        .config("[rules.off]\ndomain = [\"Bücher.DE.\"]\n")
        .stage()
        .unwrap();

    assert_eq!(status(&staged).rules, vec![domain_rule("xn--bcher-kva.de")]);
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
        let Response::Error(error) = response else {
            panic!("{entry:?} was accepted: {response:?}");
        };
        assert_eq!(error.code, ErrorCode::InvalidConfig, "{entry:?}");
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
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(ad_hoc(figma()))],
        }))
    );
    let staged = staged.restart_at(at("2026-10-16T23:10:00+02:00"));
    assert_eq!(rules(&staged), vec![ad_hoc(figma())]);
}

#[test]
fn removing_an_ad_hoc_rule_lifts_it() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate(&staged, "com.figma.Desktop");
    send(&staged, add_rule(figma()));
    staged.core().handle(reported(false, custom_hours()));

    let (effects, _) = send(&staged, Command::RemoveRule(figma()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(rules(&staged), Vec::new());
}

#[test]
fn a_rule_that_doesnt_exist_is_refused() {
    let staged = noon().stage().unwrap();

    let (_, removed) = send(&staged, Command::RemoveRule(figma()));

    assert_eq!(refusal(removed), ErrorCode::NoSuchRule);
}

#[test]
fn ad_hoc_rules_from_a_newer_build_are_left_alone() {
    let newer = "version = 3\n[rules.off]\napp = [\"com.figma.Desktop\"]\n";
    let staged = noon().rules(newer).stage().unwrap();

    let (_, response) = send(&staged, add_rule(figma()));

    assert_eq!(refusal(response), ErrorCode::AdHocRulesOff);
    assert!(status(&staged).ad_hoc_error.is_some());
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(staged.file(File::Rules).as_deref(), Some(newer));
}

#[test]
fn ad_hoc_rules_with_an_app_no_rule_could_name_are_left_alone() {
    let invalid = "version = 2\n[rules.off]\napp = [\"com.figma.Desktop \"]\n";
    let staged = noon().rules(invalid).stage().unwrap();

    let status = status(&staged);

    assert!(
        status
            .ad_hoc_error
            .is_some_and(|error| error.contains("\"com.figma.Desktop \" isn't a bundle ID")),
    );
    assert_eq!(status.rules, Vec::new());
    assert_eq!(staged.file(File::Rules).as_deref(), Some(invalid));
}

#[test]
fn an_app_whose_bundle_id_no_rule_could_name_counts_as_having_none() {
    let staged = on_by_the_schedule().stage().unwrap();

    activate(&staged, "unity.Team Cherry.Hollow Knight");

    assert_eq!(status(&staged).frontmost_app, None);
    let app_rows = staged.core().menu().entries.into_iter().filter(|entry| {
        matches!(entry, MenuEntry::Row { row } if matches!(row.image, RowImage::App { .. }))
    });
    assert_eq!(app_rows.count(), 0);
}

#[test]
fn a_config_rule_cant_be_replaced_or_removed() {
    let staged = forced();

    for command in [
        Command::RemoveRule(figma()),
        Command::AddRule {
            identity: figma(),
            says: OnOff::On,
        },
    ] {
        let (effects, response) = send(&staged, command.clone());
        assert_eq!(effects, Vec::new(), "{command:?}");
        let Response::Error(error) = response else {
            panic!("{command:?} was accepted: {response:?}");
        };
        assert_eq!(error.code, ErrorCode::ConfigRule, "{command:?}");
        assert!(
            error.message.contains("change it there"),
            "{}",
            error.message
        );
    }
    assert_eq!(rules(&staged), vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn removing_a_config_rule_from_the_menu_is_refused() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .stage()
        .unwrap();

    let (effects, outcome) = staged.choose(&Command::RemoveRule(figma()));

    assert_eq!(effects, Vec::new());
    assert_eq!(outcome.unwrap_err().code, ErrorCode::ConfigRule);
    assert_eq!(rules(&staged), vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn always_from_the_menu_while_rules_toml_is_from_a_newer_nightcap_is_refused() {
    let newer = "version = 3\n[rules.off]\napp = [\"com.apple.mail\"]\n";
    let staged = late()
        .rules(newer)
        .then(reported(true, custom_hours()))
        .then(activate_event("com.figma.Desktop"))
        .stage()
        .unwrap();
    let always = choice_titled(&app_row(&staged).unwrap(), "Always").command;

    let (effects, outcome) = staged.choose(&always);

    assert_eq!(effects, Vec::new());
    assert_eq!(outcome.unwrap_err().code, ErrorCode::AdHocRulesOff);
    assert_eq!(rules(&staged), Vec::new());
    assert_eq!(staged.file(File::Rules).as_deref(), Some(newer));
}

#[test]
fn always_makes_the_users_rule_say_that_way_replacing_one_that_said_the_other() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate(&staged, "com.apple.mail");
    send(&staged, add_rule(mail()));
    send(&staged, set(Place::from(mail()), OnOff::On, minutes(30)));

    send(
        &staged,
        Command::AddRule {
            identity: mail(),
            says: OnOff::On,
        },
    );

    let status = status(&staged);
    assert_eq!(
        status.rules,
        vec![Rule {
            identity: mail(),
            source: RuleSource::AdHoc,
            says: OnOff::On,
        }]
    );
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn adding_your_rule_again_from_settings_ends_its_snooze_and_leaves_rules_toml_as_it_is() {
    let written = "version = 2\n[rules.off]\napp = [\"com.apple.mail\"]\n";
    let staged = on_by_the_schedule().rules(written).stage().unwrap();
    activate(&staged, "com.apple.mail");
    staged.core().handle(reported(false, custom_hours()));
    staged
        .choose(&set(Place::from(mail()), OnOff::On, minutes(30)))
        .1
        .unwrap();
    staged.core().handle(reported(true, custom_hours()));

    let (effects, outcome) = staged.choose(&add_rule(mail()));

    assert!(effects.contains(&Effect::SetNightShift { enabled: false }));
    assert_eq!(outcome.unwrap().place, Place::from(mail()));
    assert_eq!(status(&staged).layers, Vec::new());
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
    staged.core().handle(reported(true, custom_hours()));

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
        identity,
        source: RuleSource::Config,
        says: OnOff::On,
    };
    assert_eq!(
        status(&staged).rules,
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

    for (config, said) in [
        (
            "[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.figma.desktop\"]\n",
            "both rules.off and rules.on",
        ),
        (
            "[rules]\napp = [\"com.figma.Desktop\"]\n",
            "under [rules.off] and [rules.on]",
        ),
        ("[menu]\nlengths = [\"45m\", \"forever\"]\n", "menu.lengths"),
        ("[menu]\nlengths = [\"30s\"]\n", "menu.lengths"),
        ("[menu]\nlengths = []\n", "menu.lengths"),
        (
            "[rules.off]\napp = [\"com.figma.Desktop \"]\n",
            "\"com.figma.Desktop \" isn't a bundle ID",
        ),
        ("[rules.on]\napp = [\"\"]\n", "\"\" isn't a bundle ID"),
        (
            "[rules.off]\ndomain = [\"https://figma.com\"]\n",
            "\"https://figma.com\" isn't a domain",
        ),
    ] {
        staged.write_config(Some(config));
        let (_, response) = staged.request(Request::Reload);
        let Response::Error(error) = response else {
            panic!("{config:?} was accepted: {response:?}");
        };
        assert_eq!(error.code, ErrorCode::InvalidConfig, "{config:?}");
        assert!(
            error.message.contains(said),
            "{config:?}: {}",
            error.message
        );
    }
}

#[test]
fn a_config_that_cant_be_names_the_line_of_what_cant_be() {
    let staged = noon().stage().unwrap();
    let path = staged.core().settings().config_path;

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
        ("\n[menu]\nlengths = [\"45m\", \"forever\"]\n", 3),
    ] {
        staged.write_config(Some(config));
        let (_, response) = staged.request(Request::Reload);

        let Response::Error(error) = response else {
            panic!("{config:?} was accepted: {response:?}");
        };
        assert!(
            error.message.starts_with(&format!("{path}: line {line}: ")),
            "{config:?}: {}",
            error.message
        );
        let found = status(&staged)
            .checks
            .into_iter()
            .map(|check| check.detail)
            .find(|detail| detail.kind() == CheckKind::ConfigFile);
        assert!(
            matches!(
                &found,
                Some(CheckDetail::ConfigFile(ConfigFileDetail::Error(FileError::Invalid {
                    line: Some(at),
                    ..
                }))) if *at == line
            ),
            "{config:?}: {found:?}"
        );
    }
}
