#![cfg(debug_assertions)]
//! What each kind of install does for itself: updating, and opening at login (ADR-0011).

mod common;

use nightcap_core::{Effect, Event, Fix, FixButton, LoginItem, SettingsCheck, SettingsSwitch};
use nightcap_protocol::{
    CheckKind, CheckState, Command, ErrorCode, Install, Instance, OnOff, Payload, Request, Response,
};

use common::{button, launch, noon, refusal, send, settings_check};

#[test]
fn update_in_a_self_managed_release_starts_sparkles_check() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();

    let (effects, response) = staged.request(Request::Update);

    assert_eq!(effects, vec![Effect::CheckForUpdates]);
    assert_eq!(response, Response::Ok(Payload::UpdateCheckStarted));
}

#[test]
fn update_is_refused_where_nightcap_doesnt_update_itself() {
    for (instance, install) in [
        (Instance::Release, Install::Managed),
        (Instance::Dev, Install::SelfManaged),
    ] {
        let staged = noon().installed(instance, install).stage().unwrap();

        let (effects, response) = staged.request(Request::Update);

        assert_eq!(effects, Vec::new());
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

    assert_eq!(launch(&staged), vec![Effect::StartUpdater]);
}

#[test]
fn a_managed_install_leaves_starting_at_login_to_its_installer() {
    let staged = noon()
        .installed(Instance::Release, Install::Managed)
        .stage()
        .unwrap();

    assert_eq!(launch(&staged), Vec::new());
}

#[test]
fn a_dev_build_never_registers_a_login_item() {
    let staged = noon()
        .installed(Instance::Dev, Install::SelfManaged)
        .stage()
        .unwrap();

    assert_eq!(launch(&staged), Vec::new());
}

fn login_item(item: LoginItem) -> Event {
    Event::LoginItemChanged { item }
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
    staged.core().handle(login_item(LoginItem::NotRegistered));

    assert_eq!(
        staged.core().settings().open_at_login,
        switch(false, Command::OpenAtLogin(OnOff::On))
    );
    assert!(
        staged
            .core()
            .handle(Event::CommandChosen {
                command: Command::OpenAtLogin(OnOff::On),
            })
            .contains(&Effect::RegisterLoginItem)
    );
    staged.core().handle(login_item(LoginItem::Enabled));
    assert_eq!(
        staged.core().settings().open_at_login,
        switch(true, Command::OpenAtLogin(OnOff::Off))
    );
    assert!(
        staged
            .core()
            .handle(Event::CommandChosen {
                command: Command::OpenAtLogin(OnOff::Off),
            })
            .contains(&Effect::UnregisterLoginItem)
    );
}

#[test]
fn managed_installs_and_dev_builds_have_no_open_at_login_switch() {
    let managed = noon()
        .installed(Instance::Release, Install::Managed)
        .stage()
        .unwrap();
    managed.core().handle(login_item(LoginItem::NotRegistered));
    assert_eq!(managed.core().settings().open_at_login, None);

    let dev = noon()
        .installed(Instance::Dev, Install::SelfManaged)
        .stage()
        .unwrap();
    dev.core().handle(login_item(LoginItem::NotRegistered));
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
    staged.core().handle(login_item(LoginItem::NotRegistered));

    assert_eq!(settings_check(&staged, CheckKind::OpenAtLogin), off);
    staged.core().handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    staged.core().handle(login_item(LoginItem::Enabled));
    assert_eq!(
        settings_check(&staged, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Ok,
            "Nightcap opens when you log in.",
            None,
            None
        )
    );
    staged.core().handle(login_item(LoginItem::NotRegistered));
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
            .core()
            .handle(Event::FixChosen {
                fix: Fix::OpenAtLogin
            })
            .contains(&Effect::RegisterLoginItem)
    );
    staged.core().handle(Event::FixChosen {
        fix: Fix::DontOpenAtLogin,
    });
    assert_eq!(settings_check(&staged, CheckKind::OpenAtLogin), off);

    let staged = staged.restart();
    staged.core().handle(login_item(LoginItem::NotRegistered));
    assert_eq!(settings_check(&staged, CheckKind::OpenAtLogin), off);
}

#[test]
fn opening_at_login_waiting_for_approval_offers_login_items() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    staged.core().handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    staged
        .core()
        .handle(login_item(LoginItem::RequiresApproval));

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
            .core()
            .handle(Event::FixChosen {
                fix: Fix::DontOpenAtLogin
            })
            .contains(&Effect::UnregisterLoginItem)
    );
}

#[test]
fn choosing_not_to_open_at_login_leaves_an_item_that_isnt_registered_alone() {
    let staged = noon()
        .installed(Instance::Release, Install::SelfManaged)
        .stage()
        .unwrap();
    staged.core().handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    staged.core().handle(login_item(LoginItem::NotRegistered));

    let (effects, outcome) = staged.choose(&Command::OpenAtLogin(OnOff::Off));

    assert_eq!(effects, Vec::new());
    assert!(outcome.is_ok());
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

    staged.core().handle(login_item(LoginItem::Enabled));
    staged.core().handle(login_item(LoginItem::NotRegistered));
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
    staged.core().handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    staged.core().handle(login_item(LoginItem::Enabled));

    assert!(
        staged
            .core()
            .handle(login_item(LoginItem::NotRegistered))
            .contains(&Effect::Notify {
                title: "Nightcap no longer opens at login".to_owned(),
                body: "Open Nightcap’s settings to turn it back on, or to keep it off.".to_owned(),
                opens: None,
            })
    );
}
