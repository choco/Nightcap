#![cfg(debug_assertions)]
//! What clients can ask: each query answers in process and over the socket alike, from the
//! snapshot the core published after its last input, and asking changes nothing (ADR-0002,
//! ADR-0010).

#![expect(
    clippy::unwrap_used,
    reason = "clippy.toml lets tests unwrap, but not the helpers beside them"
)]

mod common;

use jiff::SignedDuration;
use nightcap_core::{Fact, Staged, TourApp, UiCommand};
use nightcap_protocol::{
    About, AccessibilityDetail, Check, CheckDetail, CheckState, Checks, Command, ConfigState,
    Decision, FileError, Front, Install, Instance, Lasting, Length, NightShift, NightShiftState,
    NotificationPermission, OnOff, Payload, Permissions, Place, Preferences, Query, Reason,
    Schedule, TourState, Turn, WebsiteChecking,
};

use common::{
    accessibility_granted, activate_browser, ad_hoc, add_rule, app_rule, ask, at, custom_hours,
    domain_rule, figma, figma_app, forced, mail, noon, off_by_the_schedule, on_by_the_schedule,
    on_figma, permissions, reported, send, set, until_turn, yours,
};

/// Every query's answer over the socket is the one the shell gets in process.
fn answers_alike(staged: &Staged) {
    let core = staged.core();
    assert_eq!(
        ask(staged, Query::NightShift),
        Payload::NightShift(core.night_shift())
    );
    assert_eq!(ask(staged, Query::Front), Payload::Front(core.front()));
    assert_eq!(ask(staged, Query::Rules), Payload::Rules(core.rules()));
    assert_eq!(ask(staged, Query::Config), Payload::Config(core.config()));
    assert_eq!(
        ask(staged, Query::Preferences),
        Payload::Preferences(core.preferences())
    );
    assert_eq!(ask(staged, Query::Checks), Payload::Checks(core.checks()));
    assert_eq!(ask(staged, Query::Tour), Payload::Tour(core.tour()));
    assert_eq!(ask(staged, Query::About), Payload::About(core.about()));
}

#[test]
fn night_shift_is_unknown_until_macos_reports_it() {
    let staged = noon().stage().unwrap();

    assert_eq!(
        staged.core().night_shift(),
        NightShiftState {
            state: NightShift::Unavailable,
            strength: None,
            schedule: None,
            set_aside: false,
            scheduled: None,
            next_turn: None,
        }
    );
    answers_alike(&staged);
}

#[test]
fn night_shift_answers_its_strength_its_schedule_and_the_schedules_next_turn() {
    let staged = off_by_the_schedule()
        .then(Fact::StrengthChanged { value: 60 })
        .stage()
        .unwrap();

    assert_eq!(
        staged.core().night_shift(),
        NightShiftState {
            state: NightShift::Off,
            strength: Some(60),
            schedule: Some(custom_hours()),
            set_aside: false,
            scheduled: Some(OnOff::Off),
            next_turn: Some(Turn {
                at: at("2026-10-16T22:00:00+02:00"),
                to: OnOff::On,
            }),
        }
    );
    answers_alike(&staged);
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

    let night_shift = staged.core().night_shift();

    assert!(night_shift.set_aside);
    assert_eq!(night_shift.schedule, Some(Schedule::Off));
    assert_eq!(night_shift.next_turn, None);
}

#[test]
fn front_answers_the_app_the_site_and_what_decides_there() {
    let staged = on_figma().stage().unwrap();

    assert_eq!(
        staged.core().front(),
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
        }
    );
    answers_alike(&staged);
}

#[test]
fn front_answers_no_site_while_website_checking_is_off() {
    let staged = on_by_the_schedule().stage().unwrap();
    activate_browser(&staged, "com.apple.Safari");

    let front = staged.core().front();

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
        staged.core().front().decision,
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
fn rules_answers_every_rule_at_every_place() {
    let staged = forced();
    send(&staged, add_rule(mail()));
    send(
        &staged,
        set(figma_app(), OnOff::On, Lasting::UntilScheduleTurns),
    );
    send(&staged, Command::Off);

    assert_eq!(
        staged.core().rules(),
        vec![
            app_rule("com.figma.Desktop"),
            ad_hoc(mail()),
            yours(
                figma_app(),
                OnOff::On,
                until_turn("2026-10-17T07:00:00+02:00")
            ),
            yours(Place::Everywhere, OnOff::Off, Length::Indefinite),
        ]
    );
    answers_alike(&staged);
}

#[test]
fn config_answers_what_it_sets_and_where_it_is() {
    let staged = noon()
        .config("websites = true\nlengths = [\"15m\", \"1h30m\"]\n[night_shift]\nstrength = 40\nschedule = \"22:00-07:00\"\n")
        .stage()
        .unwrap();

    let config = staged.core().config();

    assert_eq!(
        config,
        ConfigState {
            path: staged.core().settings().config_path,
            found: true,
            error: None,
            strength: Some(40),
            schedule: Some(custom_hours()),
            websites: Some(OnOff::On),
            lengths: vec![SignedDuration::from_mins(15), SignedDuration::from_mins(90)],
            ad_hoc_error: None,
        }
    );
    answers_alike(&staged);
}

#[test]
fn config_answers_the_default_lengths_without_a_file() {
    let config = noon().stage().unwrap().core().config();

    assert!(!config.found);
    assert_eq!(
        config.lengths,
        [30, 60, 120].map(SignedDuration::from_mins).to_vec()
    );
    assert_eq!((config.strength, config.schedule), (None, None));
}

#[test]
fn config_answers_why_it_or_rules_toml_cant_be_used() {
    let staged = noon()
        .config("[rules.off]\napps = [\"com.figma.Desktop\"]\n")
        .rules("version = 3\n")
        .stage()
        .unwrap();

    let config = staged.core().config();

    assert!(matches!(
        config.error,
        Some(FileError::Malformed { line: Some(2), .. })
    ));
    assert!(matches!(
        config.ad_hoc_error,
        Some(FileError::Newer { version: 3, .. })
    ));
}

#[test]
fn preferences_answer_the_users_choices() {
    let staged = noon()
        .then(permissions(true, NotificationPermission::Granted))
        .then(UiCommand::FinishOnboarding { websites: true })
        .stage()
        .unwrap();

    assert_eq!(
        staged.core().preferences(),
        Preferences {
            website_checking: WebsiteChecking {
                state: OnOff::On,
                from_config: false,
            },
            notifications_wanted: true,
            open_at_login: false,
            onboarding_completed: 1,
            onboarding_finished: true,
        }
    );
    answers_alike(&staged);
}

#[test]
fn preferences_answer_website_checking_as_the_config_sets_it() {
    let staged = noon().config("websites = false\n").stage().unwrap();

    let preferences = staged.core().preferences();

    assert_eq!(
        preferences.website_checking,
        WebsiteChecking {
            state: OnOff::Off,
            from_config: true,
        }
    );
    assert_eq!(
        (
            preferences.onboarding_completed,
            preferences.onboarding_finished
        ),
        (0, false)
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
    } = staged.core().checks();

    assert!(checks.contains(&Check {
        detail: CheckDetail::Accessibility(AccessibilityDetail::NotAllowed),
        state: CheckState::Drifted,
    }));
    assert_eq!(
        permissions,
        Permissions {
            accessibility: false,
            notifications: NotificationPermission::Denied,
        }
    );
    answers_alike(&staged);
}

#[test]
fn checks_answer_the_grants_once_reported() {
    let staged = noon().stage().unwrap();
    assert_eq!(
        staged.core().checks().permissions,
        Permissions {
            accessibility: false,
            notifications: NotificationPermission::NotRequested,
        }
    );

    staged.report(accessibility_granted());

    assert_eq!(
        staged.core().checks().permissions,
        Permissions {
            accessibility: true,
            notifications: NotificationPermission::Granted,
        }
    );
}

fn start_tour(staged: &Staged) {
    let app = |bundle_id: &str, name: &str| TourApp {
        bundle_id: bundle_id.parse().unwrap(),
        name: name.to_owned(),
    };
    staged.send_ui(UiCommand::StartTour {
        sample: app("io.github.choco.nightcap.tour-sample", "Tour Sample"),
        site: None,
        example: app("com.apple.mail", "Mail"),
    });
}

#[test]
fn tour_answers_whether_one_runs_and_stands_in_for_the_schedule() {
    let off = off_by_the_schedule().stage().unwrap();
    assert_eq!(off.core().tour(), None);

    start_tour(&off);

    assert_eq!(off.core().tour(), Some(TourState { stands_in: true }));
    answers_alike(&off);
    off.send_ui(UiCommand::EndTour);
    assert_eq!(off.core().tour(), None);
    let on = on_by_the_schedule().stage().unwrap();
    start_tour(&on);
    assert_eq!(on.core().tour(), Some(TourState { stands_in: false }));
}

#[test]
fn about_answers_the_version_and_the_install() {
    let managed = noon().stage().unwrap();
    let self_managed = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();

    assert_eq!(
        managed.core().about(),
        About {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: Install::Managed,
        }
    );
    assert_eq!(self_managed.core().about().install, Install::SelfManaged);
    answers_alike(&managed);
}

#[test]
fn a_query_answers_as_of_the_last_input() {
    let staged = noon().stage().unwrap();
    assert_eq!(staged.core().rules(), Vec::new());

    send(&staged, add_rule(figma()));

    assert_eq!(staged.core().rules(), vec![ad_hoc(figma())]);
    staged.report(reported(true, custom_hours()));
    assert_eq!(staged.core().night_shift().state, NightShift::On);
}
