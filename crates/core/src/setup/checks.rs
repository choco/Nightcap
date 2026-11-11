//! The checks: each thing Nightcap depends on, compared with what the user set up, as the facts
//! the checks query answers, from which each client words them, and the app picks their fixes
//! and announces the problems that appear.

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, Check, CheckDetail, CheckState, ConfigFileDetail,
    Install, LoginItem, NightShift, NightShiftDetail, NotificationPermission, NotificationsDetail,
    OpenAtLoginDetail,
};

use crate::server::state::Inner;

/// How long sunset to sunrise can go without sun times before it's a problem. CoreBrightness
/// took 83 seconds to work them out after a login, and two minutes after Location Services came
/// back on (macOS 27).
pub(crate) const SUN_TIMES_PATIENCE: SignedDuration = SignedDuration::from_mins(10);

impl Inner {
    /// Everything Nightcap depends on, compared with what the user set up.
    pub(crate) fn checks(&self, now: Timestamp) -> Vec<Check> {
        let sun_times_overdue = self
            .facts
            .sun_times_awaited_since
            .is_some_and(|since| now.duration_since(since) >= SUN_TIMES_PATIENCE);
        let mut found = vec![CheckDetail::NightShift(
            if self.facts.night_shift == NightShift::Unavailable {
                NightShiftDetail::Unavailable
            } else if self.setup.schedule_set_aside && !self.setup.schedule_off_accepted {
                NightShiftDetail::SetAside
            } else if sun_times_overdue {
                NightShiftDetail::NoSunTimes
            } else {
                NightShiftDetail::Available
            },
        )];
        // Nothing is said about the permissions until the shell first reports them.
        if let Some(granted) = self.setup.accessibility {
            found.push(CheckDetail::Accessibility(
                match (self.facts.website_checking(), granted) {
                    (false, _) => AccessibilityDetail::NotNeeded,
                    (true, true) => AccessibilityDetail::Allowed,
                    (true, false) => AccessibilityDetail::NotAllowed,
                },
            ));
            found.push(CheckDetail::Notifications(
                match (
                    self.setup.notifications == NotificationPermission::Granted,
                    self.setup.notifications_wanted,
                ) {
                    (true, _) => NotificationsDetail::Allowed,
                    (false, true) => NotificationsDetail::SwitchedOff,
                    (false, false) => NotificationsDetail::Off,
                },
            ));
        }
        if let Some(item) = self
            .setup
            .login_item
            .filter(|_| self.install == Install::SelfManaged)
        {
            found.push(CheckDetail::OpenAtLogin(
                match (item, self.setup.open_at_login) {
                    (LoginItem::Enabled, _) => OpenAtLoginDetail::Opens,
                    (LoginItem::RequiresApproval, true) => OpenAtLoginDetail::AwaitingApproval,
                    (LoginItem::NotRegistered, true) => OpenAtLoginDetail::Removed,
                    (_, false) => OpenAtLoginDetail::Off,
                },
            ));
        }
        found.push(CheckDetail::ConfigFile(
            match (&self.loaded.config_error, self.loaded.config_found) {
                (Some(error), _) => ConfigFileDetail::Error(error.into()),
                (None, true) => ConfigFileDetail::Valid,
                (None, false) => ConfigFileDetail::Missing,
            },
        ));
        found.push(CheckDetail::AdHocRules(match &self.loaded.ad_hoc_error {
            Some(error) => AdHocRulesDetail::Off(error.into()),
            None => AdHocRulesDetail::On,
        }));
        found
            .into_iter()
            .map(|detail| Check {
                state: state(&detail),
                detail,
            })
            .collect()
    }
}

/// Where what a check found leaves it: drift is what the user set up that stopped working, and
/// broken what can only be fixed. The CLI's `check()` (`crates/cli/src/wording.rs`) words only
/// the details this makes problems, and Swift's `Notice(problem:)` only those the app announces,
/// so moving a detail into a problem here means adding it there.
fn state(detail: &CheckDetail) -> CheckState {
    match detail {
        CheckDetail::NightShift(NightShiftDetail::Available)
        | CheckDetail::Accessibility(AccessibilityDetail::Allowed)
        | CheckDetail::Notifications(NotificationsDetail::Allowed)
        | CheckDetail::OpenAtLogin(OpenAtLoginDetail::Opens)
        | CheckDetail::ConfigFile(ConfigFileDetail::Valid)
        | CheckDetail::AdHocRules(AdHocRulesDetail::On) => CheckState::Ok,
        CheckDetail::Accessibility(AccessibilityDetail::NotNeeded)
        | CheckDetail::Notifications(NotificationsDetail::Off)
        | CheckDetail::OpenAtLogin(OpenAtLoginDetail::Off)
        | CheckDetail::ConfigFile(ConfigFileDetail::Missing) => CheckState::Off,
        CheckDetail::NightShift(NightShiftDetail::SetAside)
        | CheckDetail::Accessibility(AccessibilityDetail::NotAllowed)
        | CheckDetail::Notifications(NotificationsDetail::SwitchedOff)
        | CheckDetail::OpenAtLogin(
            OpenAtLoginDetail::AwaitingApproval | OpenAtLoginDetail::Removed,
        ) => CheckState::Drifted,
        CheckDetail::NightShift(NightShiftDetail::Unavailable | NightShiftDetail::NoSunTimes)
        | CheckDetail::ConfigFile(ConfigFileDetail::Error(_))
        | CheckDetail::AdHocRules(AdHocRulesDetail::Off(_)) => CheckState::Broken,
    }
}
