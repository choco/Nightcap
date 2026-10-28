//! The checks: each thing Nightcap depends on, compared with what the user set up, as the facts
//! the checks query answers, which each client words itself; what each fix does; and the event
//! that announces a problem when it appears.

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, Check, CheckDetail, CheckKind, CheckState, Command,
    ConfigFileDetail, FileError, LoginItem, NightShift, NightShiftDetail, NotificationPermission,
    NotificationsDetail, OnOff, OpenAtLoginDetail, Schedule, SetBy,
};

use crate::rules::places::is_app_or_site_rule;
use crate::server::state::Inner;
use crate::storage::ad_hoc_rules::AdHocError;
use crate::storage::config::ConfigError;
use crate::{Effect, Event};

/// How long sunset to sunrise can go without sun times before it's a problem. CoreBrightness
/// took 83 seconds to work them out after a login, and two minutes after Location Services came
/// back on (macOS 27).
const SUN_TIMES_PATIENCE: SignedDuration = SignedDuration::from_mins(10);

/// An action Settings offers with a problem: a restore, an accept, or reloading a broken file.
/// Which one a check offers, and in what words, is the shell's; what it does is the core's,
/// short of the four the shell carries out itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Fix {
    /// The shell's: the macOS prompt while Nightcap has no entry in the list, else the pane.
    AskForAccessibility,
    /// The shell's: the macOS prompt while notifications were never asked for, else Nightcap's
    /// page in Notification settings.
    AllowNotifications,
    /// The shell's: Login Items in System Settings, where macOS asks for its approval.
    OpenLoginItems,
    /// The shell's: Location Services in System Settings.
    OpenLocationServices,
    /// The core's, like the rest: sent back as `UiCommand::Fix`.
    StopCheckingWebsites,
    KeepNotificationsOff,
    KeepScheduleOff,
    OpenAtLogin,
    DontOpenAtLogin,
    Reload,
}

/// Whether a turn tells the problems that appear in it.
pub(crate) enum Telling {
    /// A fact's turn, which tells each one.
    Aloud,
    /// The turn of a command, a CLI request or quitting, which tells none: what its input
    /// causes is the user's own doing, in front of them already, and is never told. The
    /// problems in `before`, there before its input, wait for the next turn that tells.
    Quietly { before: Vec<CheckKind> },
}

impl Inner {
    /// How a turn whose input is about to change the core at `now` tells the problems that
    /// appear in it: aloud, or quietly, as the user's own doing.
    pub(crate) fn telling(&self, aloud: bool, now: Timestamp) -> Telling {
        if aloud {
            Telling::Aloud
        } else {
            Telling::Quietly {
                before: self
                    .problems(now)
                    .iter()
                    .map(|check| check.detail.kind())
                    .collect(),
            }
        }
    }

    /// Announces each problem once as it appears at `now`, as `telling` allows. A problem that
    /// ends and comes back is announced again.
    pub(crate) fn announce(&mut self, telling: &Telling, now: Timestamp, events: &mut Vec<Event>) {
        let problems = self.problems(now);
        self.grants
            .announced
            .retain(|kind| problems.iter().any(|check| check.detail.kind() == *kind));
        for check in problems {
            let kind = check.detail.kind();
            if self.grants.announced.contains(&kind) {
                continue;
            }
            match telling {
                Telling::Aloud => {
                    self.grants.announced.push(kind);
                    if announced(&check.detail) {
                        events.push(Event::ProblemAppeared { check });
                    }
                }
                Telling::Quietly { before } if !before.contains(&kind) => {
                    self.grants.announced.push(kind);
                }
                Telling::Quietly { .. } => {}
            }
        }
    }

    /// The checks that need the user at `now`: drift or broken.
    fn problems(&self, now: Timestamp) -> Vec<Check> {
        self.checks(now)
            .into_iter()
            .filter(|check| matches!(check.state, CheckState::Drifted | CheckState::Broken))
            .collect()
    }

    /// Carries out a fix the user chose in Settings; the shell carries out its own.
    pub(crate) fn run_fix(&mut self, fix: Fix, effects: &mut Vec<Effect>) {
        match fix {
            Fix::StopCheckingWebsites => self.fix_by(&Command::Websites(OnOff::Off), effects),
            Fix::DontOpenAtLogin => self.fix_by(&Command::OpenAtLogin(OnOff::Off), effects),
            Fix::KeepNotificationsOff => self.grants.notifications_wanted = false,
            Fix::KeepScheduleOff => self.grants.schedule_off_accepted = true,
            Fix::OpenAtLogin => effects.push(Effect::RegisterLoginItem),
            // An invalid config shows in its check again.
            Fix::Reload => _ = self.reload(effects),
            Fix::AskForAccessibility
            | Fix::AllowNotifications
            | Fix::OpenLoginItems
            | Fix::OpenLocationServices => {
                tracing::debug!("{fix:?} is the shell's to carry out");
            }
        }
    }

    /// Carries out a fix by running `command`.
    fn fix_by(&mut self, command: &Command, effects: &mut Vec<Effect>) {
        if let Err(error) = self.run_command(command, effects) {
            tracing::warn!("a fix was refused: {:?}", error.code);
        }
    }
}

impl Inner {
    /// Everything Nightcap depends on, compared with what the user set up.
    pub(crate) fn checks(&self, now: Timestamp) -> Vec<Check> {
        let look = self.places.look(&self.facts.world(now));
        let rules_from = |set_by| {
            look.rules
                .iter()
                .filter(|(rule, _)| is_app_or_site_rule(rule) && rule.set_by == set_by)
                .count()
        };
        let sun_times_waited = self
            .reconciling
            .sun_times_awaited_since()
            .map(|since| now.duration_since(since))
            .filter(|waited| *waited >= SUN_TIMES_PATIENCE);
        let mut found = vec![CheckDetail::NightShift(
            if self.facts.night_shift == NightShift::Unavailable {
                NightShiftDetail::Unavailable
            } else if self.grants.schedule_set_aside && !self.grants.schedule_off_accepted {
                NightShiftDetail::SetAside
            } else if let Some(waited) = sun_times_waited {
                NightShiftDetail::NoSunTimes { waited }
            } else {
                NightShiftDetail::Available {
                    schedule: self.facts.timeline.schedule.map(Schedule::from),
                }
            },
        )];
        // Nothing is said about the permissions until the shell first reports them.
        if let Some(granted) = self.grants.accessibility {
            found.push(CheckDetail::Accessibility(
                match (self.facts.website_checking(), granted) {
                    (false, _) => AccessibilityDetail::NotNeeded,
                    (true, true) => AccessibilityDetail::Allowed,
                    (true, false) => AccessibilityDetail::NotAllowed,
                },
            ));
            found.push(CheckDetail::Notifications(
                match (
                    self.grants.notifications == NotificationPermission::Granted,
                    self.grants.notifications_wanted,
                ) {
                    (true, _) => NotificationsDetail::Allowed,
                    (false, true) => NotificationsDetail::SwitchedOff,
                    (false, false) => NotificationsDetail::Off,
                },
            ));
        }
        if let Some(item) = self.grants.login_item.filter(|_| self.manages_itself) {
            found.push(CheckDetail::OpenAtLogin(
                match (item, self.grants.open_at_login) {
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
                (None, true) => ConfigFileDetail::Valid {
                    rules: rules_from(SetBy::Config),
                },
                (None, false) => ConfigFileDetail::Missing,
            },
        ));
        found.push(CheckDetail::AdHocRules(match &self.loaded.ad_hoc_error {
            Some(error) => AdHocRulesDetail::Off(error.into()),
            None => AdHocRulesDetail::On {
                rules: rules_from(SetBy::You),
            },
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
/// broken what can only be fixed.
fn state(detail: &CheckDetail) -> CheckState {
    match detail {
        CheckDetail::NightShift(NightShiftDetail::Available { .. })
        | CheckDetail::Accessibility(AccessibilityDetail::Allowed)
        | CheckDetail::Notifications(NotificationsDetail::Allowed)
        | CheckDetail::OpenAtLogin(OpenAtLoginDetail::Opens)
        | CheckDetail::ConfigFile(ConfigFileDetail::Valid { .. })
        | CheckDetail::AdHocRules(AdHocRulesDetail::On { .. }) => CheckState::Ok,
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
        CheckDetail::NightShift(
            NightShiftDetail::Unavailable | NightShiftDetail::NoSunTimes { .. },
        )
        | CheckDetail::ConfigFile(ConfigFileDetail::Error(_))
        | CheckDetail::AdHocRules(AdHocRulesDetail::Off(_)) => CheckState::Broken,
    }
}

/// Whether a problem with what a check found is announced as it appears on its own. Night Shift
/// being unavailable is a fact about the Mac, switched-off notifications can't announce
/// themselves, and a login item waiting for approval follows the user's own choice, which macOS
/// announces itself.
fn announced(detail: &CheckDetail) -> bool {
    match detail {
        CheckDetail::NightShift(
            NightShiftDetail::SetAside | NightShiftDetail::NoSunTimes { .. },
        )
        | CheckDetail::Accessibility(AccessibilityDetail::NotAllowed)
        | CheckDetail::OpenAtLogin(OpenAtLoginDetail::Removed)
        | CheckDetail::ConfigFile(ConfigFileDetail::Error(_))
        | CheckDetail::AdHocRules(AdHocRulesDetail::Off(_)) => true,
        CheckDetail::NightShift(
            NightShiftDetail::Unavailable | NightShiftDetail::Available { .. },
        )
        | CheckDetail::Accessibility(
            AccessibilityDetail::Allowed | AccessibilityDetail::NotNeeded,
        )
        | CheckDetail::Notifications(_)
        | CheckDetail::OpenAtLogin(
            OpenAtLoginDetail::AwaitingApproval | OpenAtLoginDetail::Opens | OpenAtLoginDetail::Off,
        )
        | CheckDetail::ConfigFile(ConfigFileDetail::Valid { .. } | ConfigFileDetail::Missing)
        | CheckDetail::AdHocRules(AdHocRulesDetail::On { .. }) => false,
    }
}

impl From<&ConfigError> for FileError {
    fn from(error: &ConfigError) -> Self {
        match error {
            ConfigError::Unreadable { path, error } => Self::Unreadable {
                path: path.display().to_string(),
                message: error.to_string(),
            },
            ConfigError::Malformed { path, error } => Self::Malformed {
                path: path.display().to_string(),
                line: error.line,
                message: error.message.clone(),
            },
            ConfigError::Invalid {
                path,
                line,
                problem,
            } => Self::Invalid {
                path: path.display().to_string(),
                line: Some(*line),
                message: problem.to_string(),
            },
        }
    }
}

impl From<&AdHocError> for FileError {
    fn from(error: &AdHocError) -> Self {
        match error {
            AdHocError::Unreadable { path, error } => Self::Unreadable {
                path: path.display().to_string(),
                message: error.to_string(),
            },
            AdHocError::Malformed { path, error } => Self::Malformed {
                path: path.display().to_string(),
                line: error.line,
                message: error.message.clone(),
            },
            AdHocError::Newer { path, version } => Self::Newer {
                path: path.display().to_string(),
                version: *version,
            },
            AdHocError::Invalid { path, error } => Self::Invalid {
                path: path.display().to_string(),
                line: None,
                message: error.to_string(),
            },
        }
    }
}
