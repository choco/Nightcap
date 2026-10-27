#![cfg(debug_assertions)]
//! What each kind of install does for itself: updating, and opening at login (ADR-0011).

mod common;

use nightcap_core::{
    Effect, Event, Fact, Fix, FixButton, LoginItem, Outcome, SettingsCheck, SettingsSwitch,
    UiCommand,
};
use nightcap_protocol::{
    Check, CheckDetail, CheckKind, CheckState, Command, ErrorCode, Install, Instance, OnOff,
    OpenAtLoginDetail, Payload, Request, Response,
};

use common::{button, launch, noon, refusal, send, settings_check};

#[test]
fn update_in_a_self_managed_release_starts_sparkles_check() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();

    let (outcome, response) = staged.request(Request::Update);

    assert_eq!(outcome.effects, vec![Effect::CheckForUpdates]);
    assert_eq!(response, Response::Ok(Payload::UpdateCheckStarted));
}

#[test]
fn update_is_refused_where_nightcap_doesnt_update_itself() {
    for (instance, install) in [
        (Instance::Release, Install::Managed),
        (Instance::Dev, Install::SelfManaged),
    ] {
        let staged = noon().installed(instance, install).stage().unwrap();

        let (outcome, response) = staged.request(Request::Update);

        assert_eq!(outcome.effects, Vec::new());
        let Response::Error(error) = response else {
            panic!("expected an error, got {response:?}");
        };
        assert_eq!(error.code, ErrorCode::NoUpdater);
    }
}

#[test]
fn a_self_managed_release_starts_its_updater_at_launch_and_registers_nothing() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();

    assert_eq!(launch(&staged).effects, vec![Effect::StartUpdater]);
}

#[test]
fn a_managed_install_leaves_starting_at_login_to_its_installer() {
    let staged = noon()
        .installed(Instance::Release, Install::Managed)
        .stage()
        .unwrap();

    assert_eq!(launch(&staged).effects, Vec::new());
}

#[test]
fn a_dev_build_never_registers_a_login_item() {
    let staged = noon()
        .installed(Instance::Dev, Install::SelfManaged)
        .stage()
        .unwrap();

    assert_eq!(launch(&staged).effects, Vec::new());
}

#[test]
fn only_an_install_that_updates_itself_opens_itself_at_login() {
    for (instance, install, manages_itself) in [
        (Instance::Release, Install::SelfManaged, true),
        (Instance::Release, Install::Managed, false),
        (Instance::Dev, Install::SelfManaged, false),
        (Instance::Dev, Install::Managed, false),
    ] {
        let staged = noon().installed(instance, install).stage().unwrap();
        staged.report(login_item(LoginItem::NotRegistered));

        let (_, update) = staged.request(Request::Update);
        let (_, login) = send(&staged, Command::OpenAtLogin(OnOff::On));

        let case = format!("{instance:?}, {install:?}");
        assert_eq!(matches!(update, Response::Ok(_)), manages_itself, "{case}");
        assert_eq!(matches!(login, Response::Ok(_)), manages_itself, "{case}");
        assert_eq!(
            staged.core().onboarding().opens_at_login.is_some(),
            manages_itself,
            "{case}"
        );
    }
}

fn login_item(item: LoginItem) -> Fact {
    Fact::LoginItemChanged { item }
}

#[test]
fn settings_switch_opening_at_login_in_a_self_managed_install() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    let switch = |on, command| {
        Some(SettingsSwitch {
            on,
            locked: false,
            command,
        })
    };
    staged.report(login_item(LoginItem::NotRegistered));

    assert_eq!(
        staged.core().settings().open_at_login,
        switch(false, Command::OpenAtLogin(OnOff::On))
    );
    assert!(
        staged
            .send(Command::OpenAtLogin(OnOff::On))
            .0
            .effects
            .contains(&Effect::RegisterLoginItem)
    );
    staged.report(login_item(LoginItem::Enabled));
    assert_eq!(
        staged.core().settings().open_at_login,
        switch(true, Command::OpenAtLogin(OnOff::Off))
    );
    assert!(
        staged
            .send(Command::OpenAtLogin(OnOff::Off))
            .0
            .effects
            .contains(&Effect::UnregisterLoginItem)
    );
}

#[test]
fn managed_installs_and_dev_builds_have_no_open_at_login_switch() {
    let managed = noon()
        .installed(Instance::Release, Install::Managed)
        .stage()
        .unwrap();
    managed.report(login_item(LoginItem::NotRegistered));
    assert_eq!(managed.core().settings().open_at_login, None);

    let dev = noon()
        .installed(Instance::Dev, Install::SelfManaged)
        .stage()
        .unwrap();
    dev.report(login_item(LoginItem::NotRegistered));
    assert_eq!(dev.core().settings().open_at_login, None);
    let (_, response) = send(&dev, Command::OpenAtLogin(OnOff::On));
    assert_eq!(refusal(response), ErrorCode::NoLoginItem);
}

fn login_check(
    state: CheckState,
    text: &str,
    restore: Option<FixButton>,
    accept: Option<FixButton>,
) -> Option<SettingsCheck> {
    Some(SettingsCheck {
        kind: CheckKind::OpenAtLogin,
        state,
        text: text.to_owned(),
        restore,
        accept,
    })
}

#[test]
fn opening_at_login_that_stops_after_the_user_chose_it_is_a_problem() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    let off = login_check(
        CheckState::Off,
        "Nightcap doesn’t open at login.",
        None,
        None,
    );
    staged.report(login_item(LoginItem::NotRegistered));

    assert_eq!(settings_check(&staged, CheckKind::OpenAtLogin), off);
    _ = staged.send(Command::OpenAtLogin(OnOff::On));
    staged.report(login_item(LoginItem::Enabled));
    assert_eq!(
        settings_check(&staged, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Ok,
            "Nightcap opens when you log in.",
            None,
            None
        )
    );
    staged.report(login_item(LoginItem::NotRegistered));
    assert_eq!(
        settings_check(&staged, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Drifted,
            "Nightcap no longer opens at login: it was removed in System Settings.",
            button("Open at Login", Fix::OpenAtLogin),
            button("Don’t Open at Login", Fix::DontOpenAtLogin),
        )
    );
    assert!(
        staged
            .send_ui(UiCommand::Fix(Fix::OpenAtLogin))
            .effects
            .contains(&Effect::RegisterLoginItem)
    );
    staged.send_ui(UiCommand::Fix(Fix::DontOpenAtLogin));
    assert_eq!(settings_check(&staged, CheckKind::OpenAtLogin), off);

    let staged = staged.restart();
    staged.report(login_item(LoginItem::NotRegistered));
    assert_eq!(settings_check(&staged, CheckKind::OpenAtLogin), off);
}

#[test]
fn opening_at_login_waiting_for_approval_offers_login_items() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    _ = staged.send(Command::OpenAtLogin(OnOff::On));
    staged.report(login_item(LoginItem::RequiresApproval));

    assert_eq!(
        settings_check(&staged, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Drifted,
            "macOS is waiting for your approval in Login Items.",
            button("Open Login Items…", Fix::OpenLoginItems),
            button("Don’t Open at Login", Fix::DontOpenAtLogin),
        )
    );
    assert!(
        staged
            .send_ui(UiCommand::Fix(Fix::DontOpenAtLogin))
            .effects
            .contains(&Effect::UnregisterLoginItem)
    );
}

#[test]
fn choosing_not_to_open_at_login_leaves_an_item_that_isnt_registered_alone() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    _ = staged.send(Command::OpenAtLogin(OnOff::On));
    staged.report(login_item(LoginItem::NotRegistered));

    let (outcome, answer) = staged.send(Command::OpenAtLogin(OnOff::Off));

    assert_eq!(outcome, Outcome::default());
    assert!(answer.is_ok());
    assert_eq!(
        settings_check(&staged, CheckKind::OpenAtLogin).map(|check| check.state),
        Some(CheckState::Off)
    );
}

#[test]
fn a_login_item_added_in_system_settings_becomes_the_users_choice() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();

    staged.report(login_item(LoginItem::Enabled));
    staged.report(login_item(LoginItem::NotRegistered));
    assert_eq!(
        settings_check(&staged, CheckKind::OpenAtLogin).map(|check| check.state),
        Some(CheckState::Drifted)
    );
}

#[test]
fn opening_at_login_removed_in_system_settings_is_announced() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    _ = staged.send(Command::OpenAtLogin(OnOff::On));
    staged.report(login_item(LoginItem::Enabled));

    assert!(
        staged
            .report(login_item(LoginItem::NotRegistered))
            .events
            .contains(&Event::ProblemAppeared {
                check: Check {
                    detail: CheckDetail::OpenAtLogin(OpenAtLoginDetail::Removed),
                    state: CheckState::Drifted,
                },
            })
    );
}
