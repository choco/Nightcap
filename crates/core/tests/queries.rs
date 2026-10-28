#![cfg(debug_assertions)]
//! What clients can ask: each query answers in process and over the socket alike, from the
//! snapshot the core published after its last input, and asking changes nothing (ADR-0002,
//! ADR-0010).

mod common;

use jiff::SignedDuration;
use nightcap_core::{AppCommand, Fact, Scenario, Staged};
use nightcap_protocol::{
    About, AccessibilityDetail, Answer, Check, CheckDetail, CheckState, Checks, Command,
    ConfigState, Decision, ExportFormat, Front, Inherited, Install, Lasting, Length, ListedRule,
    LoginItem, NightShift, NightShiftState, NotSelfManaged, NotificationPermission, OnOff,
    Permissions, Place, PlaceInFront, Preferences, Query, Reason, Rules, Schedule, TrialState,
    WebsiteChecking,
};

use common::{
    accessibility_granted, activate_browser, activate_browser_event, activate_event, ad_hoc,
    add_rule, adopted, app_rule, ask, at, custom_hours, domain_rule, figma_app, forced, host_event,
    listed, mail, minutes, noon, off_by_the_schedule, on_by_the_schedule, on_figma, permissions,
    report_zurich_sun, reported, send, set, until, until_turn, yours,
};

/// Every query's answer over the socket, all asked in one request, in the order asked and however
/// often one is, is the one the shell gets in process from the snapshot it takes; asking nothing
/// answers nothing.
fn answers_alike(staged: &Staged) {
    let snapshot = staged.core().snapshot();
    assert_eq!(
        ask(
            staged,
            vec![
                Query::NightShift,
                Query::Front,
                Query::Rules,
                Query::Config,
                Query::Preferences,
                Query::Checks,
                Query::Trial,
                Query::About,
                Query::Export(ExportFormat::Toml),
                Query::Rules,
            ]
        ),
        vec![
            Answer::NightShift(snapshot.night_shift()),
            Answer::Front(Box::new(snapshot.front())),
            Answer::Rules(snapshot.rules()),
            Answer::Config(Box::new(snapshot.config())),
            Answer::Preferences(snapshot.preferences()),
            Answer::Checks(snapshot.checks()),
            Answer::Trial(snapshot.trial()),
            Answer::About(snapshot.about()),
            Answer::Export(snapshot.export(ExportFormat::Toml)),
            Answer::Rules(snapshot.rules()),
        ]
    );
    assert_eq!(ask(staged, Vec::new()), Vec::new());
}

#[test]
fn night_shift_is_unknown_until_macos_reports_it() {
    let staged = noon().stage().unwrap();

    assert_eq!(
        staged.core().snapshot().night_shift(),
        NightShiftState {
            state: NightShift::Unavailable,
            strength: None,
            schedule: None,
            set_aside: false,
            scheduled: None,
            next_on: None,
            next_off: None,
        }
    );
    answers_alike(&staged);
}

#[test]
fn night_shift_answers_its_strength_its_schedule_and_the_schedules_next_turns() {
    let staged = off_by_the_schedule()
        .then(Fact::StrengthChanged { value: 60 })
        .stage()
        .unwrap();

    assert_eq!(
        staged.core().snapshot().night_shift(),
        NightShiftState {
            state: NightShift::Off,
            strength: Some(60),
            schedule: Some(custom_hours()),
            set_aside: false,
            scheduled: Some(OnOff::Off),
            next_on: Some(at("2026-10-16T22:00:00+02:00")),
            next_off: Some(at("2026-10-17T07:00:00+02:00")),
        }
    );
    answers_alike(&staged);
}

#[test]
fn night_shift_answers_when_the_schedule_next_turns_it_each_way() {
    let custom = on_by_the_schedule().stage().unwrap();
    let sun = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    report_zurich_sun(&sun);
    sun.report(reported(true, Schedule::SunsetToSunrise));
    let unscheduled = noon().then(reported(false, Schedule::Off)).stage().unwrap();

    // On until 07:00, then off until 22:00 the next evening.
    let night_shift = custom.core().snapshot().night_shift();
    assert_eq!(night_shift.next_off, Some(at("2026-10-17T07:00:00+02:00")));
    assert_eq!(night_shift.next_on, Some(at("2026-10-17T22:00:00+02:00")));
    let night_shift = sun.core().snapshot().night_shift();
    assert_eq!(night_shift.next_off, Some(at("2026-10-16T05:50:47Z")));
    assert_eq!(night_shift.next_on, Some(at("2026-10-16T16:31:32Z")));
    let night_shift = unscheduled.core().snapshot().night_shift();
    assert_eq!((night_shift.next_on, night_shift.next_off), (None, None));
    assert_eq!(
        custom.core().snapshot().night_shift().next_transition(),
        Some(at("2026-10-17T07:00:00+02:00"))
    );
    assert_eq!(
        unscheduled
            .core()
            .snapshot()
            .night_shift()
            .next_transition(),
        None
    );
    answers_alike(&custom);
}

#[test]
fn night_shift_answers_sunset_to_sunrise_set_aside() {
    let staged = noon()
        .then(Fact::StatusChanged {
            enabled: false,
            schedule: Schedule::Off,
            sun_schedule_set_aside: true,
        })
        .stage()
        .unwrap();

    let night_shift = staged.core().snapshot().night_shift();

    assert!(night_shift.set_aside);
    assert_eq!(night_shift.schedule, Some(Schedule::Off));
    assert_eq!(night_shift.next_transition(), None);
}

#[test]
fn front_answers_the_app_the_site_and_what_decides_there() {
    let staged = on_figma().stage().unwrap();

    assert_eq!(
        staged.core().snapshot().front(),
        Front {
            app: Some("com.apple.Safari".parse().unwrap()),
            watching: true,
            host: Some("www.figma.com".to_owned()),
            site: Some("figma.com".parse().unwrap()),
            decision: Decision {
                state: Some(OnOff::Off),
                place: Place::Domain("figma.com".parse().unwrap()),
                reasons: vec![Reason::Rule(domain_rule("figma.com"))],
            },
            places: vec![
                PlaceInFront {
                    place: Place::Domain("figma.com".parse().unwrap()),
                    gives: OnOff::Off,
                    temporary: None,
                    indefinite: Some(domain_rule("figma.com")),
                    base: None,
                    inherits: Some(Inherited {
                        from: Place::Everywhere,
                        says: OnOff::On,
                    }),
                },
                PlaceInFront {
                    place: Place::App("com.apple.Safari".parse().unwrap()),
                    gives: OnOff::On,
                    temporary: None,
                    indefinite: None,
                    base: None,
                    inherits: Some(Inherited {
                        from: Place::Everywhere,
                        says: OnOff::On,
                    }),
                },
            ],
            everywhere: PlaceInFront {
                place: Place::Everywhere,
                gives: OnOff::On,
                temporary: None,
                indefinite: None,
                base: Some(Reason::Schedule {
                    state: OnOff::On,
                    until: Some(at("2026-10-17T07:00:00+02:00")),
                }),
                inherits: None,
            },
        }
    );
    answers_alike(&staged);
}

#[test]
fn front_answers_no_site_while_website_checking_is_off() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate_browser(&staged, "com.apple.Safari");

    let front = staged.core().snapshot().front();

    assert_eq!(front.app, Some("com.apple.Safari".parse().unwrap()));
    assert!(!front.watching);
    assert_eq!((front.host, front.site), (None, None));
    assert_eq!(
        front.decision.reasons,
        vec![Reason::Schedule {
            state: OnOff::On,
            until: Some(at("2026-10-17T07:00:00+02:00")),
        }]
    );
}

#[test]
fn front_answers_a_snoozed_rule_under_the_rule_that_decides() {
    let staged = forced();
    send(
        &staged,
        set(figma_app(), OnOff::On, Lasting::UntilScheduleTurns),
    );

    assert_eq!(
        staged.core().snapshot().front().decision,
        Decision {
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
        }
    );
}

#[test]
fn front_answers_what_a_site_that_says_nothing_inherits_from_its_browser() {
    let staged = on_by_the_schedule()
        .config("websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n")
        .then(activate_browser_event("com.apple.Safari"))
        .then(host_event(1, "www.example.com"))
        .stage()
        .unwrap();

    let site = staged.core().snapshot().front().places.remove(0);

    assert_eq!(site.place, Place::Domain("example.com".parse().unwrap()));
    assert_eq!(
        site.inherits,
        Some(Inherited {
            from: Place::App("com.apple.Safari".parse().unwrap()),
            says: OnOff::Off,
        })
    );
    assert_eq!(site.gives, OnOff::Off);
}

#[test]
fn front_answers_an_external_on_over_the_rule_it_snoozed() {
    let staged = noon()
        .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
        .then(reported(false, custom_hours()))
        .then(activate_event("com.figma.Desktop"))
        .then(reported(true, custom_hours()))
        .stage()
        .unwrap();

    let figma = staged.core().snapshot().front().places.remove(0);

    assert_eq!(figma.place, figma_app());
    assert_eq!(
        figma.temporary,
        Some(adopted(
            figma_app(),
            OnOff::On,
            Some("2026-10-17T07:00:00+02:00")
        ))
    );
    assert_eq!(figma.gives, OnOff::On);
    assert_eq!(figma.indefinite, Some(app_rule("com.figma.Desktop")));
}

#[test]
fn rules_answers_the_apps_and_sites_rules_with_their_snoozes_apart_from_the_temporary_rules() {
    let staged = forced();
    let mail_app = Place::from(mail());
    let until_morning = || {
        yours(
            figma_app(),
            OnOff::On,
            until_turn("2026-10-17T07:00:00+02:00"),
        )
    };
    let for_an_hour = || {
        yours(
            Place::Everywhere,
            OnOff::On,
            until("2026-10-17T00:00:00+02:00"),
        )
    };
    send(&staged, add_rule(mail()));
    send(
        &staged,
        set(figma_app(), OnOff::On, Lasting::UntilScheduleTurns),
    );
    // Mail's rule says off too, so nothing snoozes it.
    send(&staged, set(mail_app.clone(), OnOff::Off, minutes(30)));
    send(&staged, Command::Off);
    send(&staged, set(Place::Everywhere, OnOff::On, minutes(60)));
    let snoozed = |rule, snooze| ListedRule {
        rule,
        snooze: Some(snooze),
    };

    assert_eq!(
        staged.core().snapshot().rules(),
        Rules {
            rules: vec![
                snoozed(app_rule("com.figma.Desktop"), until_morning()),
                listed(ad_hoc(mail())),
            ],
            temporary: vec![
                until_morning(),
                yours(mail_app, OnOff::Off, until("2026-10-16T23:30:00+02:00")),
                for_an_hour(),
            ],
        }
    );
    assert_eq!(
        staged.core().snapshot().front().everywhere.indefinite,
        Some(yours(Place::Everywhere, OnOff::Off, Length::Indefinite))
    );
    answers_alike(&staged);
}

#[test]
fn config_answers_what_it_sets_and_where_it_is() {
    let staged = noon()
        .config("websites = true\nlengths = [\"15m\", \"1h30m\"]\n[night_shift]\nstrength = 40\nschedule = \"22:00-07:00\"\n")
        .stage()
        .unwrap();

    let config = staged.core().snapshot().config();

    assert_eq!(
        config,
        ConfigState {
            path: staged.core().snapshot().config().path,
            strength: Some(40),
            schedule: Some(custom_hours()),
            websites: Some(OnOff::On),
            lengths: vec![SignedDuration::from_mins(15), SignedDuration::from_mins(90)],
        }
    );
    answers_alike(&staged);
}

#[test]
fn config_answers_the_default_lengths_without_a_file() {
    let config = noon().stage().unwrap().core().snapshot().config();

    assert_eq!(
        config.lengths,
        [30, 60, 120].map(SignedDuration::from_mins).to_vec()
    );
    assert_eq!((config.strength, config.schedule), (None, None));
}

#[test]
fn preferences_answer_the_users_choices() {
    let staged = noon()
        .then(permissions(true, NotificationPermission::Granted))
        .then(Command::Websites(OnOff::On))
        .stage()
        .unwrap();

    assert_eq!(
        staged.core().snapshot().preferences(),
        Preferences {
            website_checking: WebsiteChecking {
                state: OnOff::On,
                from_config: false,
            },
            notifications_wanted: true,
            open_at_login: false,
        }
    );
    answers_alike(&staged);
}

#[test]
fn preferences_answer_website_checking_as_the_config_sets_it() {
    let staged = noon().config("websites = false\n").stage().unwrap();

    assert_eq!(
        staged.core().snapshot().preferences().website_checking,
        WebsiteChecking {
            state: OnOff::Off,
            from_config: true,
        }
    );
}

#[test]
fn checks_answer_each_check_and_the_grants_behind_them() {
    let staged = noon()
        .config("websites = true\n")
        .then(permissions(false, NotificationPermission::Denied))
        .stage()
        .unwrap();

    let Checks {
        checks,
        permissions,
    } = staged.core().snapshot().checks();

    assert!(checks.contains(&Check {
        detail: CheckDetail::Accessibility(AccessibilityDetail::NotAllowed),
        state: CheckState::Drifted,
    }));
    assert_eq!(
        permissions,
        Permissions {
            accessibility: false,
            notifications: NotificationPermission::Denied,
            login_item: None,
        }
    );
    answers_alike(&staged);
}

#[test]
fn checks_answer_the_grants_and_the_login_item_once_reported() {
    let staged = noon().stage().unwrap();
    assert_eq!(
        staged.core().snapshot().checks().permissions,
        Permissions {
            accessibility: false,
            notifications: NotificationPermission::NotRequested,
            login_item: None,
        }
    );

    staged.report(accessibility_granted());
    staged.report(Fact::LoginItemChanged {
        item: LoginItem::RequiresApproval,
    });

    assert_eq!(
        staged.core().snapshot().checks().permissions,
        Permissions {
            accessibility: true,
            notifications: NotificationPermission::Granted,
            login_item: Some(LoginItem::RequiresApproval),
        }
    );
    answers_alike(&staged);
}

fn begin_trial(staged: &Staged, stands_in: bool) {
    staged.send_app(AppCommand::BeginTrial { stands_in });
}

#[test]
fn trial_answers_whether_one_runs_and_stands_in_for_the_schedule() {
    let off = off_by_the_schedule().stage().unwrap();
    assert_eq!(off.core().snapshot().trial(), None);

    begin_trial(&off, true);

    assert_eq!(
        off.core().snapshot().trial(),
        Some(TrialState { stands_in: true })
    );
    answers_alike(&off);
    off.send_app(AppCommand::EndTrial);
    assert_eq!(off.core().snapshot().trial(), None);
    let on = on_by_the_schedule().stage().unwrap();
    begin_trial(&on, false);
    assert_eq!(
        on.core().snapshot().trial(),
        Some(TrialState { stands_in: false })
    );
}

#[test]
fn about_answers_the_version_and_the_install() {
    let managed = noon().stage().unwrap();
    let self_managed = noon().installed(Install::SelfManaged).stage().unwrap();

    assert_eq!(
        managed.core().snapshot().about(),
        About {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: Install::NotSelfManaged(NotSelfManaged::ManagedInstall),
        }
    );
    assert_eq!(
        self_managed.core().snapshot().about().install,
        Install::SelfManaged
    );
    answers_alike(&managed);
}
