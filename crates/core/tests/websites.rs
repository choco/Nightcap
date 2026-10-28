#![cfg(debug_assertions)]
//! Website checking: watching the front browser's host for the domain rules (ADR-0009, ADR-0012).

#![expect(
    clippy::unwrap_used,
    reason = "clippy.toml lets tests unwrap, but not the helpers beside them"
)]

mod common;

use nightcap_core::{Effect, Fact, Staged};
use nightcap_protocol::{
    Command, Lasting, NotificationPermission, OnOff, Place, Reason, Refusal, RuleIdentity,
    WebsiteChecking,
};

use common::{
    accessibility_granted, activate, activate_browser, add_rule, custom_hours, domain_rule, late,
    noon, on_by_the_schedule, on_figma, refusal, report_host, reported, resettable, send, set,
    until_turn, watch, with_figma_domain, yours,
};

/// Website checking is useless without Accessibility, so tests about it start granted.
fn grant_accessibility(staged: &Staged) {
    staged.report(accessibility_granted());
}

#[test]
fn a_domain_rule_turns_night_shift_off_on_its_site() {
    let staged = with_figma_domain().stage().unwrap();

    assert_eq!(
        activate_browser(&staged, "com.apple.Safari"),
        vec![watch("com.apple.Safari", 1)]
    );
    assert_eq!(
        report_host(&staged, 1, "www.figma.com"),
        vec![Effect::SetNightShift { enabled: false }]
    );

    assert!(staged.core().snapshot().front().watching);
    assert_eq!(
        staged.core().snapshot().front().host.as_deref(),
        Some("www.figma.com")
    );
    assert_eq!(
        staged.core().snapshot().front().decision.reasons,
        vec![Reason::Rule(domain_rule("figma.com"))]
    );
}

#[test]
fn a_host_from_an_older_watch_is_dropped() {
    let staged = with_figma_domain().stage().unwrap();
    activate_browser(&staged, "com.apple.Safari");
    assert_eq!(
        activate_browser(&staged, "com.google.Chrome"),
        vec![watch("com.google.Chrome", 2)]
    );

    assert_eq!(report_host(&staged, 1, "www.figma.com"), Vec::new());

    assert_eq!(staged.core().snapshot().front().host, None);
}

#[test]
fn leaving_the_browser_stops_watching_and_lifts_the_force() {
    let staged = on_figma().stage().unwrap();

    assert_eq!(
        activate(&staged, "com.apple.Terminal"),
        vec![
            Effect::StopWatching,
            Effect::SetNightShift { enabled: true }
        ]
    );
}

#[test]
fn with_website_checking_on_the_front_browser_is_watched_without_any_domain_rule() {
    let staged = late().config("websites = true\n").stage().unwrap();
    grant_accessibility(&staged);
    staged.report(reported(true, custom_hours()));

    assert_eq!(
        activate_browser(&staged, "com.apple.Safari"),
        vec![watch("com.apple.Safari", 1)]
    );
}

#[test]
fn a_new_watch_leaves_night_shift_alone_until_its_first_read() {
    let staged = on_figma().stage().unwrap();

    // Without a host yet, Chrome alone wouldn't force Night Shift off, but switching it on until
    // the first read would only flash it.
    assert_eq!(
        activate_browser(&staged, "com.google.Chrome"),
        vec![watch("com.google.Chrome", 2)]
    );
    assert_eq!(report_host(&staged, 2, "www.figma.com"), Vec::new());
}

#[test]
fn an_unreadable_host_leaves_only_app_rules() {
    let staged = on_figma().stage().unwrap();

    let effects = staged
        .report(Fact::HostChanged {
            generation: 1,
            host: None,
        })
        .effects;

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert!(staged.core().snapshot().front().watching);
    assert_eq!(staged.core().snapshot().front().host, None);
}

#[test]
fn hosts_compare_in_normal_form() {
    let staged = with_figma_domain().stage().unwrap();
    activate_browser(&staged, "com.apple.Safari");

    assert_eq!(
        report_host(&staged, 1, "WWW.FIGMA.COM."),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn a_browser_whose_place_says_off_is_watched_while_a_site_in_it_could_say_on() {
    let staged = late()
        .config("websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n")
        .stage()
        .unwrap();
    grant_accessibility(&staged);
    staged.report(reported(true, custom_hours()));
    let safari_app = Place::App("com.apple.Safari".parse().unwrap());

    assert_eq!(
        activate_browser(&staged, "com.apple.Safari"),
        vec![Effect::SetNightShift { enabled: false }]
    );
    let (effects, _) = send(
        &staged,
        set(safari_app.clone(), OnOff::On, Lasting::UntilScheduleTurns),
    );
    assert!(effects.contains(&watch("com.apple.Safari", 1)));
    let (effects, _) = send(&staged, Command::Reset(safari_app));
    assert!(effects.contains(&Effect::StopWatching));
    let (effects, _) = send(
        &staged,
        Command::AddRule {
            identity: RuleIdentity::Domain("youtube.com".parse().unwrap()),
            says: OnOff::On,
        },
    );
    assert!(effects.contains(&watch("com.apple.Safari", 2)));
}

#[test]
fn snoozing_a_domain_rule_lifts_it_and_keeps_watching() {
    let staged = on_figma().stage().unwrap();
    let figma_site = Place::Domain("figma.com".parse().unwrap());

    let (effects, _) = send(
        &staged,
        set(figma_site.clone(), OnOff::On, Lasting::UntilScheduleTurns),
    );

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        resettable(&staged),
        vec![yours(
            figma_site,
            OnOff::On,
            until_turn("2026-10-17T07:00:00+02:00")
        )]
    );
    assert!(staged.core().snapshot().front().watching);
}

#[test]
fn website_checking_is_off_until_turned_on() {
    let staged = on_by_the_schedule()
        .config("[rules.off]\ndomain = [\"figma.com\"]\n")
        .stage()
        .unwrap();

    assert_eq!(activate_browser(&staged, "com.apple.Safari"), Vec::new());
    assert_eq!(
        staged.core().snapshot().preferences().website_checking,
        WebsiteChecking {
            state: OnOff::Off,
            from_config: false,
        }
    );

    let (effects, _) = send(&staged, Command::Websites(OnOff::On));

    assert!(
        effects.contains(&watch("com.apple.Safari", 1)),
        "{effects:?}"
    );
}

#[test]
fn the_config_decides_website_checking_when_it_sets_it() {
    let staged = noon().config("websites = true\n").stage().unwrap();

    let (_, response) = send(&staged, Command::Websites(OnOff::Off));

    assert_eq!(refusal(response), Refusal::WebsiteCheckingInConfig);
    assert_eq!(
        staged.core().snapshot().preferences().website_checking,
        WebsiteChecking {
            state: OnOff::On,
            from_config: true,
        }
    );
}

#[test]
fn turning_website_checking_on_without_accessibility_asks_for_it() {
    let staged = noon().stage().unwrap();
    staged.report(Fact::PermissionsChanged {
        accessibility: false,
        notifications: NotificationPermission::Granted,
    });

    let (effects, _) = send(&staged, Command::Websites(OnOff::On));

    assert_eq!(effects, vec![Effect::AskForAccessibility]);
}

fn safari() -> RuleIdentity {
    RuleIdentity::App("com.apple.Safari".parse().unwrap())
}

#[test]
fn a_browser_its_own_app_rule_forces_off_is_not_watched() {
    let staged = on_figma().stage().unwrap();

    let (effects, _) = send(&staged, add_rule(safari()));
    assert!(effects.contains(&Effect::StopWatching));
    assert_eq!(staged.core().snapshot().front().site, None);
    assert_eq!(staged.core().snapshot().front().host, None);
    let (effects, _) = send(&staged, Command::RemoveRule(safari()));
    assert!(effects.contains(&watch("com.apple.Safari", 2)));
}
