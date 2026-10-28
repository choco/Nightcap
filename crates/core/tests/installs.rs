#![cfg(debug_assertions)]
//! What each kind of install does for itself: updating, and opening at login (ADR-0011).

mod common;

use nightcap_core::{Effect, Event, Fact, Outcome, Staged};
use nightcap_protocol::{
    Check, CheckDetail, CheckKind, CheckState, Command, Install, Instance, LoginItem,
    NotSelfManaged, OnOff, OpenAtLoginDetail, Payload, Refusal, Response,
};

use common::{check, launch, noon, refusal, send};

#[test]
fn update_in_a_self_managed_release_starts_sparkles_check() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();

    let (effects, response) = send(&staged, Command::CheckForUpdates);

    assert_eq!(effects, vec![Effect::CheckForUpdates]);
    assert_eq!(response, Response::Ok(Payload::UpdateCheckStarted));
}

#[test]
fn update_is_refused_where_nightcap_doesnt_update_itself() {
    for (instance, install, not_self_managed) in [
        (
            Instance::Release,
            Install::Managed,
            NotSelfManaged::ManagedInstall,
        ),
        (
            Instance::Dev,
            Install::SelfManaged,
            NotSelfManaged::DevBuild,
        ),
        (Instance::Dev, Install::Managed, NotSelfManaged::DevBuild),
    ] {
        let staged = noon().installed(instance, install).stage().unwrap();

        let (effects, response) = send(&staged, Command::CheckForUpdates);

        assert_eq!(effects, Vec::new());
        assert_eq!(refusal(response), Refusal::NoUpdater(not_self_managed));
    }
}

#[test]
fn the_apps_update_check_starts_sparkles_check_or_comes_back_as_a_refusal() {
    let self_managed = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    let managed = noon().stage().unwrap();

    let (started, _) = self_managed.send(Command::CheckForUpdates);
    let (refused, _) = managed.send(Command::CheckForUpdates);

    assert_eq!(
        started,
        Outcome {
            effects: vec![Effect::CheckForUpdates],
            events: Vec::new(),
        }
    );
    assert_eq!(
        refused,
        Outcome {
            effects: Vec::new(),
            events: vec![Event::Refused {
                command: Command::CheckForUpdates,
                why: Refusal::NoUpdater(NotSelfManaged::ManagedInstall),
            }],
        }
    );
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

        let (_, update) = send(&staged, Command::CheckForUpdates);
        let (_, login) = send(&staged, Command::OpenAtLogin(OnOff::On));

        let case = format!("{instance:?}, {install:?}");
        assert_eq!(matches!(update, Response::Ok(_)), manages_itself, "{case}");
        assert_eq!(matches!(login, Response::Ok(_)), manages_itself, "{case}");
        assert_eq!(
            check(&staged, CheckKind::OpenAtLogin).is_some(),
            manages_itself,
            "{case}"
        );
    }
}

fn login_item(item: LoginItem) -> Fact {
    Fact::LoginItemChanged { item }
}

#[test]
fn opening_at_login_registers_and_unregisters_the_login_item_in_a_self_managed_install() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    staged.report(login_item(LoginItem::NotRegistered));

    assert_eq!(
        login_check(&staged),
        opening(OpenAtLoginDetail::Off, CheckState::Off)
    );
    assert!(
        staged
            .send(Command::OpenAtLogin(OnOff::On))
            .0
            .effects
            .contains(&Effect::RegisterLoginItem)
    );
    assert!(staged.core().snapshot().preferences().open_at_login);
    staged.report(login_item(LoginItem::Enabled));
    assert_eq!(
        login_check(&staged),
        opening(OpenAtLoginDetail::Opens, CheckState::Ok)
    );
    assert!(
        staged
            .send(Command::OpenAtLogin(OnOff::Off))
            .0
            .effects
            .contains(&Effect::UnregisterLoginItem)
    );
    assert!(!staged.core().snapshot().preferences().open_at_login);
}

#[test]
fn managed_installs_and_dev_builds_have_no_open_at_login_check() {
    let managed = noon()
        .installed(Instance::Release, Install::Managed)
        .stage()
        .unwrap();
    managed.report(login_item(LoginItem::NotRegistered));
    assert_eq!(login_check(&managed), None);
    let (_, response) = send(&managed, Command::OpenAtLogin(OnOff::On));
    assert_eq!(
        refusal(response),
        Refusal::NoLoginItem(NotSelfManaged::ManagedInstall)
    );

    let dev = noon()
        .installed(Instance::Dev, Install::SelfManaged)
        .stage()
        .unwrap();
    dev.report(login_item(LoginItem::NotRegistered));
    assert_eq!(login_check(&dev), None);
    let (_, response) = send(&dev, Command::OpenAtLogin(OnOff::On));
    assert_eq!(
        refusal(response),
        Refusal::NoLoginItem(NotSelfManaged::DevBuild)
    );
}

/// What opening at login's check found and where that leaves it.
fn login_check(staged: &Staged) -> Option<(CheckDetail, CheckState)> {
    check(staged, CheckKind::OpenAtLogin).map(|check| (check.detail, check.state))
}

fn opening(found: OpenAtLoginDetail, state: CheckState) -> Option<(CheckDetail, CheckState)> {
    Some((CheckDetail::OpenAtLogin(found), state))
}

#[test]
fn opening_at_login_that_stops_after_the_user_chose_it_is_a_problem() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    let off = opening(OpenAtLoginDetail::Off, CheckState::Off);
    staged.report(login_item(LoginItem::NotRegistered));

    assert_eq!(login_check(&staged), off);
    _ = staged.send(Command::OpenAtLogin(OnOff::On));
    staged.report(login_item(LoginItem::Enabled));
    assert_eq!(
        login_check(&staged),
        opening(OpenAtLoginDetail::Opens, CheckState::Ok)
    );
    staged.report(login_item(LoginItem::NotRegistered));
    assert_eq!(
        login_check(&staged),
        opening(OpenAtLoginDetail::Removed, CheckState::Drifted)
    );
    assert!(
        staged
            .send(Command::OpenAtLogin(OnOff::On))
            .0
            .effects
            .contains(&Effect::RegisterLoginItem)
    );
    _ = staged.send(Command::OpenAtLogin(OnOff::Off));
    assert_eq!(login_check(&staged), off);

    let staged = staged.restart();
    staged.report(login_item(LoginItem::NotRegistered));
    assert_eq!(login_check(&staged), off);
}

#[test]
fn opening_at_login_waiting_for_approval_is_a_problem_the_user_can_accept() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    _ = staged.send(Command::OpenAtLogin(OnOff::On));
    staged.report(login_item(LoginItem::RequiresApproval));

    assert_eq!(
        login_check(&staged),
        opening(OpenAtLoginDetail::AwaitingApproval, CheckState::Drifted)
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
        check(&staged, CheckKind::OpenAtLogin).map(|check| check.state),
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
        check(&staged, CheckKind::OpenAtLogin).map(|check| check.state),
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
