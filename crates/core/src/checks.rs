//! The checks: each thing Nightcap depends on, compared with what the user set up, in the words
//! Settings shows it in, with its fixes; the menu's problem line; and the notification that
//! announces a problem when it appears.

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    Check, CheckKind, CheckState, Command, NightShift, NotificationPermission, OnOff, RuleSource,
    Schedule, TimeOfDay,
};

use crate::rows::capitalized;
use crate::{Core, Effect, LoginItem, State};

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
    /// The core's, like the rest: sent back as `Event::FixChosen`.
    StopCheckingWebsites,
    KeepNotificationsOff,
    KeepScheduleOff,
    OpenAtLogin,
    DontOpenAtLogin,
    Reload,
}

/// The menu's line about problems, above the Night Shift item. Choosing it opens Settings, and
/// while it's set the shell marks the icon too.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProblemLine {
    pub title: String,
    pub subtitle: Option<String>,
}

impl Core {
    /// Posts one notification when a problem appears, unless `aloud` is false because the user
    /// caused it themselves. A problem that ends and comes back is announced again.
    pub(crate) fn announce(&self, state: &mut State, aloud: bool, effects: &mut Vec<Effect>) {
        let problems: Vec<_> = state
            .checks(self.clock.now())
            .into_iter()
            .filter(|check| matches!(check.state, CheckState::Drifted | CheckState::Broken))
            .map(|check| check.kind)
            .collect();
        state
            .grants
            .announced
            .retain(|kind| problems.contains(kind));
        for kind in problems {
            if state.grants.announced.contains(&kind) {
                continue;
            }
            state.grants.announced.push(kind);
            if let Some((title, body)) = state.notice(kind).filter(|_| aloud) {
                effects.push(Effect::Notify {
                    title,
                    body,
                    opens: None,
                });
            }
        }
    }

    /// Carries out a fix the user chose in Settings; the shell carries out its own.
    pub(crate) fn run_fix(&self, state: &mut State, fix: Fix) -> Vec<Effect> {
        let mut effects = Vec::new();
        match fix {
            Fix::StopCheckingWebsites => {
                if let Err(error) =
                    self.run_command(state, &Command::Websites(OnOff::Off), &mut effects)
                {
                    tracing::warn!("a fix was refused: {:?}", error.code);
                }
            }
            Fix::KeepNotificationsOff => state.grants.notifications_wanted = false,
            Fix::KeepScheduleOff => state.grants.schedule_off_accepted = true,
            Fix::OpenAtLogin => effects.push(Effect::RegisterLoginItem),
            Fix::DontOpenAtLogin => {
                state.grants.open_at_login = false;
                // An item waiting for approval would still ask for it.
                if state.grants.login_item != Some(LoginItem::NotRegistered) {
                    effects.push(Effect::UnregisterLoginItem);
                }
            }
            // An invalid config shows in its check again.
            Fix::Reload => _ = self.reload(state, &mut effects),
            Fix::AskForAccessibility
            | Fix::AllowNotifications
            | Fix::OpenLoginItems
            | Fix::OpenLocationServices => {
                tracing::debug!("{fix:?} is the shell's to carry out");
            }
        }
        effects
    }
}

/// The grants and the login item as macOS last reported them, and sunset to sunrise set aside,
/// each with the choice its check compares it with; and the problems already announced.
pub(crate) struct Grants {
    /// None until the shell first reports it.
    pub(crate) accessibility: Option<bool>,
    pub(crate) notifications: NotificationPermission,
    /// See `state::Persisted::notifications_wanted`.
    pub(crate) notifications_wanted: bool,
    /// None until the shell first reports it.
    pub(crate) login_item: Option<LoginItem>,
    /// See `state::Persisted::open_at_login`.
    pub(crate) open_at_login: bool,
    /// A self-managed release, which opens itself at login if the user wants (ADR-0011).
    pub(crate) opens_itself_at_login: bool,
    /// macOS has sunset to sunrise set aside, and the schedule is off.
    pub(crate) schedule_set_aside: bool,
    /// See `state::Persisted::schedule_off_accepted`.
    pub(crate) schedule_off_accepted: bool,
    /// See `state::Persisted::announced`.
    pub(crate) announced: Vec<CheckKind>,
}

impl Grants {
    pub(crate) fn login_item_changed(&mut self, item: LoginItem) {
        tracing::info!("login item: {item:?}");
        self.login_item = Some(item);
        // An enabled item becomes the user's wish, however it got enabled.
        if item == LoginItem::Enabled {
            self.open_at_login = true;
        }
    }

    pub(crate) fn permissions_changed(
        &mut self,
        accessibility: bool,
        notifications: NotificationPermission,
    ) {
        tracing::info!(
            "permissions: accessibility {accessibility}, notifications {notifications:?}"
        );
        self.accessibility = Some(accessibility);
        self.notifications = notifications;
        // Allowed notifications become the user's wish, however they got allowed.
        if notifications == NotificationPermission::Granted {
            self.notifications_wanted = true;
        }
    }

    /// Whether macOS has sunset to sunrise set aside, as a status callback reports it with the
    /// schedule off. Accepting it ends once it isn't.
    pub(crate) fn schedule_reported(&mut self, set_aside: bool) {
        self.schedule_set_aside = set_aside;
        if !self.schedule_set_aside {
            self.schedule_off_accepted = false;
        }
    }
}

impl State {
    /// The checks as Settings shows them, each with its fixes.
    pub(crate) fn settings_checks(&self, now: Timestamp) -> Vec<SettingsCheck> {
        self.checks(now)
            .into_iter()
            .map(|check| {
                let (restore, accept) = self.fixes(&check);
                SettingsCheck {
                    kind: check.kind,
                    state: check.state,
                    text: check.text,
                    restore,
                    accept,
                }
            })
            .collect()
    }

    /// What Settings offers for a check: putting back what the user set up, and accepting
    /// the change as their choice.
    fn fixes(&self, check: &Check) -> (Option<FixButton>, Option<FixButton>) {
        let button = |title: &str, fix| {
            Some(FixButton {
                title: title.to_owned(),
                fix,
            })
        };
        match (check.kind, check.state) {
            (CheckKind::Accessibility, CheckState::Drifted) => (
                button("Allow…", Fix::AskForAccessibility),
                // Only the config can turn off what the config turned on.
                button("Stop Checking Websites", Fix::StopCheckingWebsites)
                    .filter(|_| self.facts.config_websites.is_none()),
            ),
            (CheckKind::Notifications, CheckState::Drifted) => (
                button("Allow Notifications…", Fix::AllowNotifications),
                button("Keep Notifications Off", Fix::KeepNotificationsOff),
            ),
            (CheckKind::Notifications, CheckState::Off) => (
                button("Allow Notifications…", Fix::AllowNotifications),
                None,
            ),
            (CheckKind::NightShift, CheckState::Drifted) => (
                button("Open Location Services…", Fix::OpenLocationServices),
                button("Keep the Schedule Off", Fix::KeepScheduleOff),
            ),
            (CheckKind::OpenAtLogin, CheckState::Drifted) => (
                // Registering again doesn't help an item that's waiting for approval.
                if self.grants.login_item == Some(LoginItem::RequiresApproval) {
                    button("Open Login Items…", Fix::OpenLoginItems)
                } else {
                    button("Open at Login", Fix::OpenAtLogin)
                },
                button("Don’t Open at Login", Fix::DontOpenAtLogin),
            ),
            (CheckKind::ConfigFile, CheckState::Broken) => (button("Reload", Fix::Reload), None),
            _ => (None, None),
        }
    }

    /// The menu's problem line, from the checks.
    pub(crate) fn problem_line(&self, now: Timestamp) -> Option<ProblemLine> {
        let problems: Vec<_> = self
            .checks(now)
            .into_iter()
            .filter(|check| matches!(check.state, CheckState::Drifted | CheckState::Broken))
            .map(|check| check.kind)
            .collect();
        if problems.is_empty() {
            return None;
        }
        let names: Vec<_> = problems
            .iter()
            .map(|kind| {
                match kind {
                    CheckKind::NightShift => "Night Shift",
                    CheckKind::Accessibility => "Accessibility",
                    CheckKind::Notifications => "notifications",
                    CheckKind::OpenAtLogin => "opening at login",
                    CheckKind::ConfigFile => "the config file",
                    CheckKind::AdHocRules => "ad-hoc rules",
                }
                .to_owned()
            })
            .collect();
        Some(ProblemLine {
            title: match problems.len() {
                1 => "1 Thing Needs Attention…".to_owned(),
                count => format!("{count} Things Need Attention…"),
            },
            subtitle: Some(capitalized(&listed(&names))),
        })
    }

    /// Everything Nightcap depends on, compared with what the user set up.
    pub(crate) fn checks(&self, now: Timestamp) -> Vec<Check> {
        let look = self.places.look(&self.facts.world(now));
        let rules_from = |source| {
            look.rules
                .iter()
                .filter(|(rule, _)| rule.source == source)
                .count()
        };
        let check = |kind, state, text: &str| Check {
            kind,
            state,
            text: text.to_owned(),
        };
        let mut checks = vec![if self.facts.night_shift == NightShift::Unavailable {
            check(
                CheckKind::NightShift,
                CheckState::Broken,
                "Night Shift isn’t available on this Mac.",
            )
        } else if self.grants.schedule_set_aside && !self.grants.schedule_off_accepted {
            check(
                CheckKind::NightShift,
                CheckState::Drifted,
                "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on.",
            )
        } else if self
            .reconciling
            .sun_times_awaited_since()
            .is_some_and(|since| now.duration_since(since) >= SUN_TIMES_PATIENCE)
        {
            check(
                CheckKind::NightShift,
                CheckState::Broken,
                "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. Custom hours work without them.",
            )
        } else {
            let time = |at: TimeOfDay| format!("{:02}:{:02}", at.hour, at.minute);
            check(
                CheckKind::NightShift,
                CheckState::Ok,
                &match self.facts.timeline.schedule.map(Schedule::from) {
                    None => "Available.".to_owned(),
                    Some(Schedule::Off) => "Available, with no schedule.".to_owned(),
                    Some(Schedule::SunsetToSunrise) => {
                        "Available, following its schedule: sunset to sunrise.".to_owned()
                    }
                    Some(Schedule::Custom { from, to }) => format!(
                        "Available, following its schedule: {} to {}.",
                        time(from),
                        time(to)
                    ),
                },
            )
        }];
        // Nothing is said about the permissions until the shell first reports them.
        if let Some(granted) = self.grants.accessibility {
            checks.push(match (self.facts.website_checking(), granted) {
                (false, _) => check(
                    CheckKind::Accessibility,
                    CheckState::Off,
                    "Not needed while website checking is off.",
                ),
                (true, true) => check(
                    CheckKind::Accessibility,
                    CheckState::Ok,
                    "Accessibility is allowed.",
                ),
                (true, false) => check(
                    CheckKind::Accessibility,
                    CheckState::Drifted,
                    "Accessibility isn’t allowed, so website checking can’t read the front tab.",
                ),
            });
            checks.push(
                match (
                    self.grants.notifications == NotificationPermission::Granted,
                    self.grants.notifications_wanted,
                ) {
                    (true, _) => check(CheckKind::Notifications, CheckState::Ok, "Allowed."),
                    (false, true) => check(
                        CheckKind::Notifications,
                        CheckState::Drifted,
                        "Notifications were allowed, but they’re switched off in System Settings.",
                    ),
                    (false, false) => check(
                        CheckKind::Notifications,
                        CheckState::Off,
                        "Notifications are off.",
                    ),
                },
            );
        }
        if let Some(item) = self
            .grants
            .login_item
            .filter(|_| self.grants.opens_itself_at_login)
        {
            checks.push(match (item, self.grants.open_at_login) {
                (LoginItem::Enabled, _) => check(
                    CheckKind::OpenAtLogin,
                    CheckState::Ok,
                    "Nightcap opens when you log in.",
                ),
                (LoginItem::RequiresApproval, true) => check(
                    CheckKind::OpenAtLogin,
                    CheckState::Drifted,
                    "macOS is waiting for your approval in Login Items.",
                ),
                (LoginItem::NotRegistered, true) => check(
                    CheckKind::OpenAtLogin,
                    CheckState::Drifted,
                    "Nightcap no longer opens at login: it was removed in System Settings.",
                ),
                (_, false) => check(
                    CheckKind::OpenAtLogin,
                    CheckState::Off,
                    "Nightcap doesn’t open at login.",
                ),
            });
        }
        checks.push(
            match (&self.loaded.config_error, self.loaded.config_found) {
                (Some(error), _) => {
                    check(CheckKind::ConfigFile, CheckState::Broken, &error.sentence())
                }
                (None, true) => check(
                    CheckKind::ConfigFile,
                    CheckState::Ok,
                    &match rules_from(RuleSource::Config) {
                        0 => "Valid, with no rules.".to_owned(),
                        1 => "Valid, with 1 rule.".to_owned(),
                        count => format!("Valid, with {count} rules."),
                    },
                ),
                (None, false) => check(CheckKind::ConfigFile, CheckState::Off, "No config file."),
            },
        );
        checks.push(
            match (&self.loaded.ad_hoc_error, rules_from(RuleSource::AdHoc)) {
                (Some(error), _) => check(
                    CheckKind::AdHocRules,
                    CheckState::Broken,
                    &format!("Ad-hoc rules are off: {error}."),
                ),
                (None, 0) => check(CheckKind::AdHocRules, CheckState::Ok, "No ad-hoc rules."),
                (None, 1) => check(CheckKind::AdHocRules, CheckState::Ok, "1 ad-hoc rule."),
                (None, count) => check(
                    CheckKind::AdHocRules,
                    CheckState::Ok,
                    &format!("{count} ad-hoc rules."),
                ),
            },
        );
        checks
    }

    /// The notification for a problem that appeared on its own; Night Shift being unavailable is
    /// a fact about the Mac, and switched-off notifications can't announce themselves.
    fn notice(&self, kind: CheckKind) -> Option<(String, String)> {
        match kind {
            CheckKind::NightShift if self.facts.night_shift == NightShift::Unavailable => None,
            CheckKind::NightShift if self.grants.schedule_set_aside => Some((
                "Sunset to sunrise needs Location Services".to_owned(),
                "macOS turned Night Shift’s schedule off. Open Nightcap’s settings to turn Location Services back on, or to keep the schedule off."
                    .to_owned(),
            )),
            CheckKind::NightShift => Some((
                "Nightcap can’t follow sunset to sunrise".to_owned(),
                "macOS hasn’t worked out sunrise and sunset. Custom hours in Night Shift’s settings work without them."
                    .to_owned(),
            )),
            CheckKind::Accessibility => Some((
                "Website checking needs Accessibility".to_owned(),
                "Open Nightcap’s settings to allow it, or to stop checking websites.".to_owned(),
            )),
            CheckKind::ConfigFile => self
                .loaded
                .config_error
                .as_ref()
                .map(|error| ("Nightcap’s config is invalid".to_owned(), error.to_string())),
            CheckKind::AdHocRules => self
                .loaded
                .ad_hoc_error
                .as_ref()
                .map(|error| ("Ad-hoc rules are off".to_owned(), error.to_string())),
            // Waiting for approval follows the user's own choice, which macOS announces itself.
            CheckKind::OpenAtLogin => {
                (self.grants.login_item == Some(LoginItem::NotRegistered)).then(|| {
                    (
                        "Nightcap no longer opens at login".to_owned(),
                        "Open Nightcap’s settings to turn it back on, or to keep it off."
                            .to_owned(),
                    )
                })
            }
            CheckKind::Notifications => None,
        }
    }
}

/// "a", "a and b", "a, b and c".
fn listed(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
