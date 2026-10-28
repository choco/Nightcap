//! The checks: each thing Nightcap depends on, compared with what the user set up, as the facts
//! the status carries and in the words Settings shows it in, with its fixes; and the event that
//! announces a problem when it appears.

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, Check, CheckDetail, CheckKind, CheckState, Command,
    ConfigFileDetail, FileError, NightShift, NightShiftDetail, NotificationPermission,
    NotificationsDetail, OnOff, OpenAtLoginDetail, Schedule, SetBy, TimeOfDay,
};

use crate::rules::places::is_app_or_site_rule;
use crate::server::state::Inner;
use crate::storage::ad_hoc_rules::AdHocError;
use crate::storage::config::ConfigError;
use crate::{Effect, Event, LoginItem};

/// How long sunset to sunrise can go without sun times before it's a problem. CoreBrightness
/// took 83 seconds to work them out after a login, and two minutes after Location Services came
/// back on (macOS 27).
const SUN_TIMES_PATIENCE: SignedDuration = SignedDuration::from_mins(10);

/// A check as Settings shows it, with what can be done about it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsCheck {
    pub kind: CheckKind,
    pub state: CheckState,
    pub text: String,
    /// Puts back what the user set up, or fixes what's broken.
    pub restore: Option<FixButton>,
    /// Accepts the change as the user's choice.
    pub accept: Option<FixButton>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FixButton {
    pub title: String,
    pub fix: Fix,
}

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

impl Inner {
    /// Announces each problem once as it appears, unless `aloud` is false because the user
    /// caused it themselves. A problem that ends and comes back is announced again.
    pub(crate) fn announce(&mut self, aloud: bool, events: &mut Vec<Event>) {
        let problems: Vec<_> = self
            .checks(self.clock.now())
            .into_iter()
            .filter(|check| matches!(check.state, CheckState::Drifted | CheckState::Broken))
            .collect();
        self.grants
            .announced
            .retain(|kind| problems.iter().any(|check| check.detail.kind() == *kind));
        for check in problems {
            let kind = check.detail.kind();
            if self.grants.announced.contains(&kind) {
                continue;
            }
            self.grants.announced.push(kind);
            if aloud && announced(&check.detail) {
                events.push(Event::ProblemAppeared { check });
            }
        }
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
    /// The checks as Settings shows them, each with its fixes.
    pub(crate) fn settings_checks(&self, now: Timestamp) -> Vec<SettingsCheck> {
        self.checks(now)
            .into_iter()
            .map(|check| {
                let (restore, accept) = self.fixes(&check.detail);
                SettingsCheck {
                    kind: check.detail.kind(),
                    state: check.state,
                    text: sentence(&check.detail),
                    restore,
                    accept,
                }
            })
            .collect()
    }

    /// What Settings offers for what a check found: putting back what the user set up, and
    /// accepting the change as their choice.
    fn fixes(&self, detail: &CheckDetail) -> (Option<FixButton>, Option<FixButton>) {
        let button = |title: &str, fix| {
            Some(FixButton {
                title: title.to_owned(),
                fix,
            })
        };
        let dont_open_at_login = || button("Don’t Open at Login", Fix::DontOpenAtLogin);
        match detail {
            CheckDetail::Accessibility(AccessibilityDetail::NotAllowed) => (
                button("Allow…", Fix::AskForAccessibility),
                // Only the config can turn off what the config turned on.
                button("Stop Checking Websites", Fix::StopCheckingWebsites)
                    .filter(|_| self.facts.config_websites.is_none()),
            ),
            CheckDetail::Notifications(NotificationsDetail::SwitchedOff) => (
                button("Allow Notifications…", Fix::AllowNotifications),
                button("Keep Notifications Off", Fix::KeepNotificationsOff),
            ),
            CheckDetail::Notifications(NotificationsDetail::Off) => (
                button("Allow Notifications…", Fix::AllowNotifications),
                None,
            ),
            CheckDetail::NightShift(NightShiftDetail::SetAside) => (
                button("Open Location Services…", Fix::OpenLocationServices),
                button("Keep the Schedule Off", Fix::KeepScheduleOff),
            ),
            // Registering again doesn't help an item that's waiting for approval.
            CheckDetail::OpenAtLogin(OpenAtLoginDetail::AwaitingApproval) => (
                button("Open Login Items…", Fix::OpenLoginItems),
                dont_open_at_login(),
            ),
            CheckDetail::OpenAtLogin(OpenAtLoginDetail::Removed) => (
                button("Open at Login", Fix::OpenAtLogin),
                dont_open_at_login(),
            ),
            CheckDetail::ConfigFile(ConfigFileDetail::Error(_)) => {
                (button("Reload", Fix::Reload), None)
            }
            _ => (None, None),
        }
    }

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

/// What a check found, in the words Settings shows it in.
fn sentence(detail: &CheckDetail) -> String {
    match detail {
        CheckDetail::NightShift(found) => night_shift_sentence(found),
        CheckDetail::Accessibility(found) => match found {
            AccessibilityDetail::Allowed => "Accessibility is allowed.",
            AccessibilityDetail::NotAllowed => {
                "Accessibility isn’t allowed, so website checking can’t read the front tab."
            }
            AccessibilityDetail::NotNeeded => "Not needed while website checking is off.",
        }
        .to_owned(),
        CheckDetail::Notifications(found) => match found {
            NotificationsDetail::Allowed => "Allowed.",
            NotificationsDetail::SwitchedOff => {
                "Notifications were allowed, but they’re switched off in System Settings."
            }
            NotificationsDetail::Off => "Notifications are off.",
        }
        .to_owned(),
        CheckDetail::OpenAtLogin(found) => match found {
            OpenAtLoginDetail::Opens => "Nightcap opens when you log in.",
            OpenAtLoginDetail::AwaitingApproval => {
                "macOS is waiting for your approval in Login Items."
            }
            OpenAtLoginDetail::Removed => {
                "Nightcap no longer opens at login: it was removed in System Settings."
            }
            OpenAtLoginDetail::Off => "Nightcap doesn’t open at login.",
        }
        .to_owned(),
        CheckDetail::ConfigFile(found) => match found {
            ConfigFileDetail::Valid { rules: 0 } => "Valid, with no rules.".to_owned(),
            ConfigFileDetail::Valid { rules: 1 } => "Valid, with 1 rule.".to_owned(),
            ConfigFileDetail::Valid { rules } => format!("Valid, with {rules} rules."),
            ConfigFileDetail::Missing => "No config file.".to_owned(),
            ConfigFileDetail::Error(error) => in_the_file(error),
        },
        CheckDetail::AdHocRules(found) => match found {
            AdHocRulesDetail::On { rules: 0 } => "No ad-hoc rules.".to_owned(),
            AdHocRulesDetail::On { rules: 1 } => "1 ad-hoc rule.".to_owned(),
            AdHocRulesDetail::On { rules } => format!("{rules} ad-hoc rules."),
            AdHocRulesDetail::Off(error) => format!("Ad-hoc rules are off: {}.", located(error)),
        },
    }
}

/// Night Shift's check, in the words Settings shows it in.
fn night_shift_sentence(found: &NightShiftDetail) -> String {
    let time = |at: TimeOfDay| format!("{:02}:{:02}", at.hour, at.minute);
    match found {
        NightShiftDetail::Available { schedule } => match schedule {
            None => "Available.".to_owned(),
            Some(Schedule::Off) => "Available, with no schedule.".to_owned(),
            Some(Schedule::SunsetToSunrise) => {
                "Available, following its schedule: sunset to sunrise.".to_owned()
            }
            Some(Schedule::Custom { from, to }) => format!(
                "Available, following its schedule: {} to {}.",
                time(*from),
                time(*to)
            ),
        },
        NightShiftDetail::Unavailable => "Night Shift isn’t available on this Mac.".to_owned(),
        NightShiftDetail::SetAside => {
            "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on."
                .to_owned()
        }
        NightShiftDetail::NoSunTimes { .. } => {
            "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. Custom hours work without them."
                .to_owned()
        }
    }
}

/// Where in the file and what, in a sentence without the path, which Settings shows above the
/// config's check: "Line 4: unknown field `domian`, expected `app` or `domain`." An invalid entry
/// or value with no line keeps its first letter: it starts with the key or entry it names, as
/// the file writes it.
fn in_the_file(error: &FileError) -> String {
    match error {
        FileError::Unreadable { message, .. } => format!("Couldn't read it: {message}."),
        FileError::Malformed {
            line: Some(line),
            message,
            ..
        }
        | FileError::Invalid {
            line: Some(line),
            message,
            ..
        } => format!("Line {line}: {message}."),
        FileError::Malformed {
            line: None,
            message,
            ..
        } => format!("{}.", capitalized(message)),
        FileError::Invalid {
            line: None,
            message,
            ..
        } => format!("{message}."),
        FileError::Newer { version, .. } => {
            format!("It’s version {version}, from a newer Nightcap than this one.")
        }
    }
}

/// What's wrong with a file, starting from its path: "/…/rules.toml: line 3: invalid type".
fn located(error: &FileError) -> String {
    match error {
        FileError::Unreadable { path, message } => format!("couldn't read {path}: {message}"),
        FileError::Malformed {
            path,
            line: Some(line),
            message,
        }
        | FileError::Invalid {
            path,
            line: Some(line),
            message,
        } => format!("{path}: line {line}: {message}"),
        FileError::Malformed {
            path,
            line: None,
            message,
        }
        | FileError::Invalid {
            path,
            line: None,
            message,
        } => format!("{path}: {message}"),
        FileError::Newer { path, version } => {
            format!("{path} is version {version}, from a newer Nightcap than this one")
        }
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

/// `text` with its first letter capitalized.
fn capitalized(text: &str) -> String {
    let mut letters = text.chars();
    letters
        .next()
        .map(char::to_uppercase)
        .into_iter()
        .flatten()
        .chain(letters)
        .collect()
}
