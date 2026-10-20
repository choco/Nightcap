uniffi::setup_scaffolding!();

mod adhoc;
mod config;
mod domain;
mod engine;
mod ipc;
mod logging;
mod schedule;
mod state;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::time::SystemTime;

use etcetera::BaseStrategy;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use nightcap_protocol::{
    Check, CheckKind, CheckState, Command, Decision, ErrorCode, ErrorReply, ExportFormat, Install,
    Instance, NightShift, NotificationPermission, OnOff, Payload, Permissions, Query, Reason,
    Request, Response, Rule, RuleIdentity, RuleSource, Schedule, Status, TimeOfDay, Verdict,
    WebsiteChecking,
};

use config::NightShiftConfig;
use engine::{Context, Inputs};
use schedule::{SunTimes, Timeline};

#[uniffi::remote(Enum)]
pub enum NotificationPermission {
    NotRequested,
    Denied,
    Granted,
}

#[uniffi::remote(Enum)]
pub enum Schedule {
    Off,
    SunsetToSunrise,
    Custom { from: TimeOfDay, to: TimeOfDay },
}

#[uniffi::remote(Record)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

// A menu item carries the very command the CLI would send (ADR-0002).
uniffi::custom_type!(SignedDuration, i64, {
    remote,
    lower: |duration| duration.as_secs(),
    try_lift: |seconds| Ok(SignedDuration::from_secs(seconds)),
});

#[uniffi::remote(Enum)]
pub enum CheckKind {
    NightShift,
    Accessibility,
    Notifications,
    OpenAtLogin,
    ConfigFile,
    AdHocRules,
}

#[uniffi::remote(Enum)]
pub enum CheckState {
    Ok,
    Off,
    Drifted,
    Broken,
}

#[uniffi::remote(Enum)]
pub enum ExportFormat {
    Nix,
    Toml,
}

#[uniffi::remote(Enum)]
pub enum OnOff {
    On,
    Off,
}

#[uniffi::remote(Enum)]
pub enum RuleIdentity {
    App(String),
    Domain(String),
}

#[uniffi::remote(Enum)]
pub enum Command {
    On,
    Off,
    Toggle,
    Disable(SignedDuration),
    Resume,
    AddRule(RuleIdentity),
    RemoveRule(RuleIdentity),
    Snooze(RuleIdentity),
    Unsnooze(RuleIdentity),
    Websites(OnOff),
    OpenAtLogin(OnOff),
    Strength(u8),
    PreviewStrength(u8),
    SetSchedule(Schedule),
}

/// What the menu shows. The core decides it; the shell only draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Menu {
    pub icon: MenuIcon,
    /// What Night Shift is doing, e.g. "Night Shift: Forced Off".
    pub status: String,
    /// Why, in a few words, e.g. "While Figma is in front".
    pub reason: Option<String>,
    /// The problem line, while any problem exists.
    pub problem_line: Option<ProblemLine>,
    pub entries: Vec<MenuEntry>,
}

/// What the settings window shows, pulled like the menu: the shell only draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Settings {
    /// "Check websites", locked while the config sets `websites`.
    pub website_checking: SettingsSwitch,
    /// "Open at login", in self-managed installs only.
    pub open_at_login: Option<SettingsSwitch>,
    /// Shown inline, each under the setting it belongs to.
    pub checks: Vec<SettingsCheck>,
    /// Where the config is read from, whether or not it exists.
    pub config_path: String,
    /// Every rule: the config's, then the ad-hoc ones.
    pub rules: Vec<SettingsRule>,
}

/// What onboarding asks, pulled like the menu: the shell only draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OnboardingSteps {
    pub night_shift: OnboardingNightShift,
    /// Website checking as the config sets it, which answers its step for good.
    pub websites_from_config: Option<bool>,
    /// Whether Nightcap opens at login now, or None where it never opens itself.
    pub opens_at_login: Option<bool>,
}

/// Onboarding's Night Shift steps, which set the strength and the schedule (ADR-0020).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OnboardingNightShift {
    /// How warm, unless the config sets it or the strength can't be read.
    pub ask_strength: bool,
    /// When it comes on, unless the config sets it or Night Shift is unavailable.
    pub ask_schedule: bool,
    /// On a schedule already, so onboarding offers to skip to Nightcap's part.
    pub already_set_up: bool,
    /// The Mac's values, which the steps start from.
    pub strength: Option<u8>,
    pub schedule: Option<Schedule>,
}

/// A rule as the Rules tab lists it: its label and the one thing to do about it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SettingsRule {
    pub identity: RuleIdentity,
    /// "From your config", "Snoozed until 07:00" or "Ad-hoc".
    pub label: String,
    pub snoozed: bool,
    pub action: RuleAction,
}

/// Choosing it sends `command` back as `Event::CommandChosen`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleAction {
    pub title: String,
    pub command: Command,
}

/// The domain a domain rule would have for what the user typed, or None when it isn't one.
#[uniffi::export]
pub fn normalize_domain(text: String) -> Option<String> {
    domain::normalize(text.trim())
}

/// A check as Settings shows it, with what can be done about it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SettingsCheck {
    pub kind: CheckKind,
    pub state: CheckState,
    pub text: String,
    /// Puts back what the user set up, or fixes what's broken.
    pub restore: Option<FixButton>,
    /// Accepts the change as the user's choice.
    pub accept: Option<FixButton>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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
    /// The core's, like the rest: sent back as `Event::FixChosen`.
    StopCheckingWebsites,
    KeepNotificationsOff,
    OpenAtLogin,
    DontOpenAtLogin,
    Reload,
}

/// Flipping it sends `command` back as `Event::CommandChosen`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SettingsSwitch {
    pub on: bool,
    /// Set by the config, so Settings can't change it.
    pub locked: bool,
    pub command: Command,
}

/// The menu's line about problems, under the status line. Choosing it opens Settings, and
/// while it's set the shell marks the icon too.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProblemLine {
    pub title: String,
    pub subtitle: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MenuIcon {
    On,
    Off,
    ForcedOff,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MenuEntry {
    /// The strength's slider, 0 to 100, under the status line. Moving it sends
    /// `PreviewStrength`, and letting go sends `Strength`.
    Strength {
        value: u8,
    },
    Item {
        item: MenuItem,
    },
    Submenu {
        title: String,
        items: Vec<MenuItem>,
    },
    /// Titles the items under it.
    Header {
        title: String,
    },
    Separator,
}

/// Choosing it sends `command` back as `Event::CommandChosen`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MenuItem {
    pub title: String,
    /// A line under the title, e.g. where a rule comes from or what choosing it does.
    pub subtitle: Option<String>,
    /// Drawn one level in, under the item it belongs to.
    pub indented: bool,
    pub checked: bool,
    pub enabled: bool,
    pub command: Command,
}

/// Lets the core's own threads ask the shell for a `tick()` on the main thread (ADR-0002).
#[uniffi::export(foreign)]
pub trait Waker: Send + Sync {
    fn wake(&self);
}

#[derive(Debug, uniffi::Enum)]
pub enum Event {
    Launched,
    /// An app came to the front; some processes have no bundle ID. `is_browser` says whether
    /// it's in the shell's browser list, so its host can be watched.
    AppActivated {
        bundle_id: Option<String>,
        /// The app's name as the user sees it, for the menu.
        name: Option<String>,
        is_browser: bool,
    },
    /// The watched browser's host, read under `generation`; None when the read failed or timed
    /// out, which leaves only app rules.
    HostChanged {
        generation: u64,
        host: Option<String>,
    },
    /// The shell read the login item's state, which the user can change in System Settings.
    LoginItemChanged {
        item: LoginItem,
    },
    PermissionsChanged {
        accessibility: bool,
        notifications: NotificationPermission,
    },
    /// Night Shift as CoreBrightness reports it. Custom hours must be valid times of day.
    StatusChanged {
        enabled: bool,
        schedule: Schedule,
    },
    /// The strength, 0 to 100, as the shell read it. CoreBrightness announces no strength
    /// change, so the shell reads it whenever something is about to show it.
    StrengthChanged {
        value: u8,
    },
    /// Sunrises and sunsets from CoreBrightness, from the day before to the day after. The
    /// shell reports them at launch, on waking and before each `TimerFired`, so they never run
    /// out.
    SunTimesChanged {
        sunrises: Vec<SystemTime>,
        sunsets: Vec<SystemTime>,
    },
    /// The system time zone changed, to this IANA time zone ID.
    TimeZoneChanged {
        iana_id: String,
    },
    /// The user went through onboarding, choosing whether to check websites.
    OnboardingFinished {
        websites: bool,
    },
    /// The user chose one of the core's fixes in Settings.
    FixChosen {
        fix: Fix,
    },
    /// The user chose a menu item or flipped a switch in Settings, which carries the command to
    /// run.
    CommandChosen {
        command: Command,
    },
    /// The wakeup the core last asked for came due.
    TimerFired,
    /// The Mac woke from sleep, so schedule transitions may have passed unseen.
    Woke,
    /// This user's login session became active or inactive, as with fast user switching.
    SessionChanged {
        active: bool,
    },
    Terminating,
}

/// Nightcap's login item, as macOS reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LoginItem {
    Enabled,
    /// Registered, but waiting for the user's approval in System Settings.
    RequiresApproval,
    NotRegistered,
}

#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum Effect {
    /// Registers Nightcap to open at login, as the user chose (ADR-0015).
    RegisterLoginItem,
    UnregisterLoginItem,
    /// Starts Sparkle's daily checks, in self-managed releases only (ADR-0015).
    StartUpdater,
    /// Starts Sparkle's own check, as if the user had asked for one.
    CheckForUpdates,
    /// Opens onboarding, skipping the steps of versions up to `completed`.
    ShowOnboarding {
        completed: u32,
    },
    /// Asks for Accessibility, which website checking needs: the macOS prompt, or the pane in
    /// System Settings once Nightcap has an entry there (ADR-0016).
    AskForAccessibility,
    /// Watches this browser's host, replacing any earlier watch; reports carry `generation`.
    WatchBrowser {
        bundle_id: String,
        generation: u64,
    },
    StopWatching,
    SetNightShift {
        enabled: bool,
    },
    /// Writes Night Shift's schedule. macOS applies it at once, so it can switch Night Shift
    /// on or off.
    SetSchedule {
        schedule: Schedule,
    },
    /// Writes the strength, 0 to 100. Uncommitted, it's a preview: macOS shows it, even with
    /// Night Shift off, but doesn't save it.
    SetStrength {
        value: u8,
        commit: bool,
    },
    /// Replaces any earlier wakeup; the shell answers with `TimerFired`.
    ScheduleWakeup {
        at: SystemTime,
    },
    CancelWakeup,
    Notify {
        title: String,
        body: String,
    },
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{bundle_id} isn't a Nightcap bundle ID")]
    UnknownBundleId { bundle_id: String },
    #[error("another Nightcap is already running at {path}")]
    AlreadyRunning { path: String },
    #[error("{iana_id} isn't a time zone this Mac knows")]
    UnknownTimeZone { iana_id: String },
    #[error("{message}")]
    Io { message: String },
}

/// The current onboarding version: welcome, notifications, and website checking.
const ONBOARDING: u32 = 1;

/// The menu's timed disables, in minutes, shortest first.
const DISABLE_CHOICES: [(i64, &str); 3] = [(30, "30 Minutes"), (60, "1 Hour"), (120, "2 Hours")];

/// How far from a computed schedule transition a status callback can arrive and still be macOS
/// performing it: CoreBrightness's clock and the core's can differ slightly (ADR-0005).
const TRANSITION_TOLERANCE: SignedDuration = SignedDuration::from_mins(2);

/// How long after waking macOS may take to perform a schedule transition it missed during sleep.
/// A guess until measured.
const POST_WAKE_GRACE: SignedDuration = SignedDuration::from_mins(5);

/// Where the core reads the time, so that tests control it completely.
trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

#[derive(uniffi::Object)]
pub struct Core {
    instance: Instance,
    install: Install,
    socket: PathBuf,
    store: state::Store,
    config_file: PathBuf,
    ad_hoc_store: adhoc::Store,
    clock: Arc<dyn Clock>,
    requests: Mutex<mpsc::Receiver<ipc::Pending>>,
    state: Mutex<State>,
}

/// Where a core lives and what kind of install it serves.
struct Setup {
    instance: Instance,
    install: Install,
    socket: PathBuf,
    state_file: PathBuf,
    config_file: PathBuf,
    rules_file: PathBuf,
    clock: Arc<dyn Clock>,
    zone: TimeZone,
}

/// What the core believes about the system, as last reported by the shell, plus what it
/// keeps across restarts.
struct State {
    night_shift: NightShift,
    strength: Option<u8>,
    manual_state: Option<OnOff>,
    timeline: Timeline,
    context: Context,
    /// The config rules and the ad-hoc ones not in the config, as the rule engine sees them.
    rules: Vec<Rule>,
    config_rules: Vec<Rule>,
    config_error: Option<String>,
    /// Whether the config file existed when it was last loaded.
    config_found: bool,
    ad_hoc: Vec<RuleIdentity>,
    /// Why `rules.toml` can't be used; while set, ad-hoc rules are off and the file is left be.
    ad_hoc_error: Option<String>,
    /// Website checking as the user chose it; `websites` in the config wins (ADR-0016).
    websites: bool,
    onboarding: u32,
    config_websites: Option<bool>,
    config_night_shift: NightShiftConfig,
    applied_from_config: NightShiftConfig,
    /// Nightcap switched the schedule off while Night Shift was on, and macOS switches it off
    /// with the schedule: the edit's callback turns it back on, as choosing "By hand" means.
    keep_on_with_schedule_off: bool,
    /// The frontmost app's name, for the menu.
    app_name: Option<String>,
    /// Whether the frontmost app is in the shell's browser list.
    is_browser: bool,
    watch: Option<Watch>,
    /// The generation of the last watch started.
    generation: u64,
    /// Rules muted until the next schedule transition.
    snoozes: Vec<RuleIdentity>,
    /// When the timed disable ends, if one is running.
    timed_disable: Option<Timestamp>,
    /// The write the shell was last asked for, until a status callback reports the result.
    pending_write: Option<bool>,
    unreported: Option<Unreported>,
    /// Set on the way out: forces no longer apply.
    released: bool,
    session: Session,
    /// Where the last run left Night Shift, which the first status callback will show.
    left_by_last_run: Option<OnOff>,
    /// Every schedule transition up to here has been applied.
    applied_until: Timestamp,
    /// The wakeup the shell was last asked for.
    wakeup: Option<Timestamp>,
    /// None until the shell first reports it.
    accessibility: Option<bool>,
    notifications: NotificationPermission,
    /// See `state::Persisted::notifications_wanted`.
    notifications_wanted: bool,
    /// See `state::Persisted::announced`.
    announced: Vec<CheckKind>,
    /// None until the shell first reports it.
    login_item: Option<LoginItem>,
    /// See `state::Persisted::open_at_login`.
    open_at_login: bool,
    /// A self-managed release, which opens itself at login if the user wants (ADR-0015).
    opens_itself_at_login: bool,
    /// What was last written to `state.json`.
    saved: state::Persisted,
}

#[uniffi::export]
impl Core {
    /// Starts the core for the app with this bundle ID, which picks the instance: its
    /// subsystem for logging and its socket. Fails if that instance is already running.
    /// `bundle_writable` says whether the user can replace the app bundle, which makes it a
    /// self-managed install. `time_zone` is the system's IANA time zone ID.
    #[uniffi::constructor]
    pub fn new(
        bundle_id: String,
        bundle_writable: bool,
        time_zone: String,
        waker: Arc<dyn Waker>,
    ) -> Result<Arc<Self>, CoreError> {
        let instance =
            Instance::from_bundle_id(&bundle_id).ok_or_else(|| CoreError::UnknownBundleId {
                bundle_id: bundle_id.clone(),
            })?;
        logging::init(&bundle_id);
        let socket = instance.socket_path().map_err(|error| CoreError::Io {
            message: error.to_string(),
        })?;
        let missing = |what: &str| CoreError::Io {
            message: format!("can't find the XDG {what} directory"),
        };
        let xdg = etcetera::base_strategy::Xdg::new().map_err(|_| missing("base"))?;
        let state_folder = xdg.state_dir().ok_or_else(|| missing("state"))?;
        let zone = TimeZone::get(&time_zone)
            .map_err(|_| CoreError::UnknownTimeZone { iana_id: time_zone })?;
        let install = if bundle_writable {
            Install::SelfManaged
        } else {
            Install::Managed
        };
        let core = Self::start(
            Setup {
                instance,
                install,
                socket,
                state_file: state_folder
                    .join(instance.xdg_folder_name())
                    .join("state.json"),
                config_file: xdg
                    .config_dir()
                    .join(instance.xdg_folder_name())
                    .join("config.toml"),
                rules_file: xdg
                    .data_dir()
                    .join(instance.xdg_folder_name())
                    .join("rules.toml"),
                clock: Arc::new(SystemClock),
                zone,
            },
            waker,
        )?;
        tracing::info!(
            "core {} listening, {install:?} install",
            env!("CARGO_PKG_VERSION")
        );
        Ok(Arc::new(core))
    }

    pub fn handle(&self, event: Event) -> Vec<Effect> {
        self.catch_up();
        // The user is looking at whatever their own choice in Nightcap's UI got wrong.
        let chosen_in_nightcap = matches!(
            event,
            Event::CommandChosen { .. }
                | Event::FixChosen { .. }
                | Event::OnboardingFinished { .. }
        );
        let mut effects = self.react(event);
        self.rewatch(&mut effects);
        self.enforce(&mut effects);
        self.reschedule(&mut effects);
        self.announce(!chosen_in_nightcap, &mut effects);
        self.save();
        effects
    }

    /// The ad-hoc rules, written for the config (ADR-0009).
    pub fn export(&self, format: ExportFormat) -> String {
        adhoc::export(&self.state().ad_hoc, format)
    }

    pub fn onboarding(&self) -> OnboardingSteps {
        let state = self.state();
        OnboardingSteps {
            night_shift: state.onboarding_night_shift(),
            websites_from_config: state.config_websites,
            opens_at_login: state
                .login_item
                .filter(|_| self.updates_itself())
                .map(|item| item != LoginItem::NotRegistered),
        }
    }

    pub fn settings(&self) -> Settings {
        let state = self.state();
        let on = state.website_checking();
        Settings {
            website_checking: SettingsSwitch {
                on,
                locked: state.config_websites.is_some(),
                command: Command::Websites(on_off(!on)),
            },
            // Registered but waiting for approval counts as on; its check explains the rest.
            config_path: self.config_file.display().to_string(),
            rules: state
                .rules
                .iter()
                .map(|rule| state.settings_rule(rule))
                .collect(),
            open_at_login: state
                .login_item
                .filter(|_| self.updates_itself())
                .map(|item| {
                    let on = item != LoginItem::NotRegistered;
                    SettingsSwitch {
                        on,
                        locked: false,
                        command: Command::OpenAtLogin(on_off(!on)),
                    }
                }),
            checks: state
                .checks()
                .into_iter()
                .map(|check| {
                    let (restore, accept) = state.fixes(&check);
                    SettingsCheck {
                        kind: check.kind,
                        state: check.state,
                        text: check.text,
                        restore,
                        accept,
                    }
                })
                .collect(),
        }
    }

    pub fn menu(&self) -> Menu {
        let state = self.state();
        let decision = state.decide();
        if state.night_shift == NightShift::Unavailable {
            return Menu {
                icon: MenuIcon::Unavailable,
                status: "Night Shift: Unavailable".to_owned(),
                reason: None,
                problem_line: None,
                entries: Vec::new(),
            };
        }
        let on = wanted(&decision).unwrap_or(state.night_shift == NightShift::On);
        let item = |title: &str, command| MenuItem {
            title: title.to_owned(),
            subtitle: None,
            indented: false,
            checked: false,
            enabled: true,
            command,
        };
        let mut entries = vec![MenuEntry::Item {
            item: if on {
                item("Turn Night Shift Off", Command::Off)
            } else {
                MenuItem {
                    subtitle: state.what_turning_on_does(&decision),
                    ..item("Turn Night Shift On", Command::On)
                }
            },
        }];
        if let Some(value) = state.strength {
            entries.insert(0, MenuEntry::Strength { value });
        }
        // A timed disable matters while Night Shift is on or forced off, or when the schedule
        // turns it on before the longest one would end.
        let longest = SignedDuration::from_mins(DISABLE_CHOICES[DISABLE_CHOICES.len() - 1].0);
        let comes_on_soon = state
            .timeline
            .next_transition(state.applied_until)
            .is_some_and(|transition| {
                transition.duration_since(self.clock.now()) <= longest
                    && state.timeline.scheduled_state(transition) == Some(true)
            });
        if on || decision.verdict == Verdict::ForceOff || comes_on_soon {
            entries.push(MenuEntry::Submenu {
                title: "Turn Off For".to_owned(),
                items: DISABLE_CHOICES
                    .map(|(minutes, title)| {
                        item(title, Command::Disable(SignedDuration::from_mins(minutes)))
                    })
                    .to_vec(),
            });
        }
        if state.timed_disable.is_some()
            || !state.snoozes.is_empty()
            || state.manual_state.is_some()
        {
            entries.push(MenuEntry::Item {
                item: MenuItem {
                    subtitle: state.what_resuming_ends(),
                    ..item("Resume", Command::Resume)
                },
            });
        }
        if let Some(bundle_id) = &state.context.app {
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Header {
                title: "Force Off".to_owned(),
            });
            entries.push(MenuEntry::Item {
                item: state.app_toggle(bundle_id),
            });
            if let Some(item) = state.site_toggle() {
                entries.push(MenuEntry::Item { item });
            }
        }
        Menu {
            icon: match (decision.verdict, on) {
                (Verdict::ForceOff, _) => MenuIcon::ForcedOff,
                (Verdict::HandsOff, true) => MenuIcon::On,
                (Verdict::HandsOff, false) => MenuIcon::Off,
            },
            status: match (decision.verdict, on) {
                (Verdict::ForceOff, _) => "Night Shift: Forced Off",
                (Verdict::HandsOff, true) => "Night Shift: On",
                (Verdict::HandsOff, false) => "Night Shift: Off",
            }
            .to_owned(),
            reason: state.menu_reason(&decision),
            problem_line: state.problem_line(),
            entries,
        }
    }

    pub fn tick(&self) -> Vec<Effect> {
        let requests = self
            .requests
            .lock()
            .expect("no panic happens while it's held");
        self.catch_up();
        let mut effects = Vec::new();
        for pending in requests.try_iter() {
            let response = self.answer(&pending.request, &mut effects);
            pending.reply(response);
        }
        self.rewatch(&mut effects);
        self.enforce(&mut effects);
        self.reschedule(&mut effects);
        // The CLI already printed what went wrong.
        self.announce(false, &mut effects);
        self.save();
        effects
    }
}

impl Core {
    fn react(&self, event: Event) -> Vec<Effect> {
        match event {
            Event::Launched => {
                let mut state = self.state();
                let mut effects = Vec::new();
                if state.onboarding < ONBOARDING {
                    // The config can answer website checking and Night Shift; opening at login
                    // it can't.
                    let night_shift = state.onboarding_night_shift();
                    let night_shift_answered =
                        !night_shift.ask_strength && !night_shift.ask_schedule;
                    if state.config_websites.is_some()
                        && night_shift_answered
                        && !state.opens_itself_at_login
                    {
                        state.onboarding = ONBOARDING;
                    } else {
                        effects.push(Effect::ShowOnboarding {
                            completed: state.onboarding,
                        });
                    }
                }
                if self.updates_itself() {
                    effects.push(Effect::StartUpdater);
                }
                state.apply_config_night_shift(&mut effects);
                effects
            }
            Event::AppActivated {
                bundle_id,
                name,
                is_browser,
            } => {
                tracing::debug!("frontmost app: {bundle_id:?}, browser: {is_browser}");
                let mut state = self.state();
                state.context.app = bundle_id;
                state.app_name = name;
                state.is_browser = is_browser;
                Vec::new()
            }
            // The host itself is never logged (ADR-0012).
            Event::HostChanged { generation, host } => {
                let mut state = self.state();
                match &mut state.watch {
                    Some(watch) if watch.generation == generation => {
                        watch.reported = true;
                        state.context.host = host.as_deref().and_then(domain::normalize);
                        tracing::debug!("host reported for watch {generation}");
                    }
                    _ => tracing::debug!("dropped a host from stale watch {generation}"),
                }
                Vec::new()
            }
            Event::LoginItemChanged { item } => {
                tracing::info!("login item: {item:?}");
                let mut state = self.state();
                state.login_item = Some(item);
                // An enabled item becomes the user's wish, however it got enabled.
                if item == LoginItem::Enabled {
                    state.open_at_login = true;
                }
                Vec::new()
            }
            Event::PermissionsChanged {
                accessibility,
                notifications,
            } => {
                tracing::info!(
                    "permissions: accessibility {accessibility}, notifications {notifications:?}"
                );
                let mut state = self.state();
                state.accessibility = Some(accessibility);
                state.notifications = notifications;
                // Allowed notifications become the user's wish, however they got allowed.
                if notifications == NotificationPermission::Granted {
                    state.notifications_wanted = true;
                }
                Vec::new()
            }
            Event::StatusChanged { enabled, schedule } => {
                let mut state = self.state();
                if state.session == Session::Away {
                    tracing::debug!("ignored a status callback while dormant");
                    return Vec::new();
                }
                state.night_shift = if enabled {
                    NightShift::On
                } else {
                    NightShift::Off
                };
                state.pending_write = None;
                let keep_on = std::mem::take(&mut state.keep_on_with_schedule_off);
                let previous = state.timeline.schedule.replace(schedule);
                if state.session == Session::Back {
                    // The shell's fresh read after returning: nobody could change Night Shift by
                    // hand while the session was away.
                    state.session = Session::Active;
                    Vec::new()
                } else if previous.is_some_and(|previous| previous != schedule) {
                    // macOS applies an edited schedule at once, so the edit is a schedule
                    // transition and whatever it switched isn't an external change.
                    tracing::info!("schedule edited");
                    state.pass_transition();
                    if keep_on && schedule == Schedule::Off && !enabled {
                        state.set_manual_state(OnOff::On);
                    }
                    Vec::new()
                } else {
                    self.reconcile(&mut state, enabled).into_iter().collect()
                }
            }
            Event::StrengthChanged { value } => {
                self.state().strength = Some(value);
                Vec::new()
            }
            Event::OnboardingFinished { websites } => {
                let mut state = self.state();
                state.onboarding = ONBOARDING;
                if state.config_websites.is_none() {
                    tracing::info!("onboarding finished; website checking {websites}");
                    state.websites = websites;
                }
                Vec::new()
            }
            Event::FixChosen { fix } => {
                let mut effects = Vec::new();
                match fix {
                    Fix::StopCheckingWebsites => {
                        if let Err(error) =
                            self.run_command(&Command::Websites(OnOff::Off), &mut effects)
                        {
                            tracing::warn!("a fix was refused: {:?}", error.code);
                        }
                    }
                    Fix::KeepNotificationsOff => self.state().notifications_wanted = false,
                    Fix::OpenAtLogin => effects.push(Effect::RegisterLoginItem),
                    Fix::DontOpenAtLogin => {
                        let mut state = self.state();
                        state.open_at_login = false;
                        // An item waiting for approval would still ask for it.
                        if state.login_item != Some(LoginItem::NotRegistered) {
                            effects.push(Effect::UnregisterLoginItem);
                        }
                    }
                    // An invalid config shows in its check again.
                    Fix::Reload => _ = self.reload(&mut effects),
                    _ => tracing::debug!("{fix:?} is the shell's to carry out"),
                }
                effects
            }
            Event::CommandChosen { command } => {
                let mut effects = Vec::new();
                if let Err(error) = self.run_command(&command, &mut effects) {
                    // The message can name a domain, so only the code is logged (ADR-0012).
                    tracing::warn!("a menu command was refused: {:?}", error.code);
                }
                effects
            }
            Event::TimerFired => {
                // The shell's timer is spent, whether or not its transition came due.
                self.state().wakeup = None;
                Vec::new()
            }
            Event::SunTimesChanged { sunrises, sunsets } => {
                let timestamps = |times: Vec<SystemTime>| -> Vec<Timestamp> {
                    times
                        .into_iter()
                        .map(|time| {
                            Timestamp::try_from(time).expect("sun times are within jiff's range")
                        })
                        .collect()
                };
                self.state().timeline.sun_times =
                    SunTimes::new(&timestamps(sunrises), &timestamps(sunsets));
                Vec::new()
            }
            Event::TimeZoneChanged { iana_id } => {
                match TimeZone::get(&iana_id) {
                    Ok(zone) => {
                        tracing::info!("time zone changed to {iana_id}");
                        self.state().timeline.zone = zone;
                    }
                    Err(error) => {
                        tracing::error!("keeping the old time zone: {iana_id} is unknown: {error}")
                    }
                }
                Vec::new()
            }
            Event::SessionChanged { active } => {
                tracing::info!("session active: {active}");
                self.state().session = if active { Session::Back } else { Session::Away };
                Vec::new()
            }
            Event::Woke => {
                tracing::debug!("woke");
                let now = self.clock.now();
                if let Some(unreported) = &mut self.state().unreported {
                    unreported.until = now + POST_WAKE_GRACE;
                }
                Vec::new()
            }
            Event::Terminating => {
                // Nightcap can't tell a restart from being removed, so it never leaves a force
                // behind (ADR-0011).
                self.state().released = true;
                if let Err(error) = std::fs::remove_file(&self.socket) {
                    tracing::warn!("couldn't remove {}: {error}", self.socket.display());
                }
                Vec::new()
            }
        }
    }

    fn start(setup: Setup, waker: Arc<dyn Waker>) -> Result<Self, CoreError> {
        let requests = ipc::serve(&setup.socket, waker)?;
        let store = state::Store::new(setup.state_file);
        let mut persisted = store.load();
        persisted.expire(setup.clock.now());
        let (config, config_error) = match config::load(&setup.config_file) {
            Ok(config) => (config, None),
            Err(error) => {
                // The error can quote a domain, so it goes to status, not the log (ADR-0012).
                tracing::warn!("running without config rules: the config is invalid; see status");
                let config = config::Config {
                    rules: Vec::new(),
                    websites: None,
                    night_shift: NightShiftConfig::default(),
                    found: true,
                };
                (config, Some(error))
            }
        };
        let ad_hoc_store = adhoc::Store::new(setup.rules_file);
        let (ad_hoc, ad_hoc_error) = match ad_hoc_store.load() {
            Ok(rules) => (rules, None),
            Err(error) => {
                tracing::warn!("ad-hoc rules are off: rules.toml can't be used; see status");
                (Vec::new(), Some(error))
            }
        };
        let core = Self {
            instance: setup.instance,
            install: setup.install,
            socket: setup.socket,
            store,
            config_file: setup.config_file,
            ad_hoc_store,
            clock: setup.clock.clone(),
            requests: Mutex::new(requests),
            state: Mutex::new(State {
                night_shift: NightShift::Unavailable,
                strength: None,
                manual_state: persisted.manual_state,
                timeline: Timeline {
                    schedule: None,
                    sun_times: SunTimes::default(),
                    zone: setup.zone,
                },
                context: Context::default(),
                rules: Vec::new(),
                config_rules: config.rules,
                config_error,
                config_found: config.found,
                ad_hoc,
                ad_hoc_error,
                websites: persisted.websites,
                onboarding: persisted.onboarding,
                config_websites: config.websites,
                config_night_shift: config.night_shift,
                applied_from_config: persisted.applied_from_config,
                keep_on_with_schedule_off: false,
                app_name: None,
                is_browser: false,
                watch: None,
                generation: 0,
                snoozes: persisted.snoozes.clone(),
                timed_disable: persisted.timed_disable,
                pending_write: None,
                unreported: None,
                released: false,
                session: Session::Active,
                left_by_last_run: persisted.night_shift,
                applied_until: setup.clock.now(),
                wakeup: None,
                accessibility: None,
                notifications: NotificationPermission::NotRequested,
                notifications_wanted: persisted.notifications_wanted,
                announced: persisted.announced.clone(),
                login_item: None,
                open_at_login: persisted.open_at_login,
                opens_itself_at_login: setup.install == Install::SelfManaged
                    && setup.instance == Instance::Release,
                saved: persisted,
            }),
        };
        core.graduate();
        Ok(core)
    }

    /// Only a self-managed release opens at login and updates through Sparkle: an installer
    /// owns a managed one, and a dev build would shadow the installed app (ADR-0015).
    fn updates_itself(&self) -> bool {
        self.install == Install::SelfManaged && self.instance == Instance::Release
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("no panic happens while it's held")
    }

    fn answer(&self, request: &Request, effects: &mut Vec<Effect>) -> Response {
        match request {
            Request::Command(command) => match self.run_command(command, effects) {
                Ok(decision) => Response::Ok(Payload::Decision(decision)),
                Err(error) => Response::Error(error),
            },
            Request::Query(Query::Status) => self.status(),
            Request::Query(Query::Rules) => {
                Response::Ok(Payload::Rules(self.state().rules.clone()))
            }
            Request::Query(Query::Export(format)) => Response::Ok(Payload::Export(adhoc::export(
                &self.state().ad_hoc,
                *format,
            ))),
            Request::Update => {
                if self.updates_itself() {
                    tracing::info!("checking for updates");
                    effects.push(Effect::CheckForUpdates);
                    return Response::Ok(Payload::UpdateCheckStarted);
                }
                Response::Error(ErrorReply {
                    code: ErrorCode::NoUpdater,
                    message: match self.instance {
                        Instance::Dev => "Dev builds don't update themselves; rebuild instead",
                        Instance::Release => {
                            "This install is managed by its installer, which keeps it up to date"
                        }
                    }
                    .to_owned(),
                })
            }
            Request::Reload => match self.reload(effects) {
                Ok(()) => self.status(),
                Err(error) => Response::Error(ErrorReply {
                    code: ErrorCode::InvalidConfig,
                    message: error,
                }),
            },
        }
    }

    /// Loads the config again; an invalid one keeps the rules loaded before.
    fn reload(&self, effects: &mut Vec<Effect>) -> Result<(), String> {
        match config::load(&self.config_file) {
            Ok(config) => {
                tracing::info!("reloaded the config: {} rules", config.rules.len());
                let mut state = self.state();
                state.config_rules = config.rules;
                state.config_websites = config.websites;
                state.config_night_shift = config.night_shift;
                state.apply_config_night_shift(effects);
                state.config_error = None;
                state.config_found = config.found;
                drop(state);
                self.graduate();
                Ok(())
            }
            Err(error) => {
                tracing::warn!("keeping the rules loaded before: the config is invalid");
                self.state().config_error = Some(error.clone());
                Err(error)
            }
        }
    }

    /// Runs a command from the CLI or the menu, answering with the resulting decision.
    fn run_command(
        &self,
        command: &Command,
        effects: &mut Vec<Effect>,
    ) -> Result<Decision, ErrorReply> {
        let night_shift = self.state().night_shift;
        let needs_night_shift = matches!(
            command,
            Command::On
                | Command::Off
                | Command::Toggle
                | Command::Disable(_)
                | Command::Resume
                | Command::Strength(_)
                | Command::PreviewStrength(_)
                | Command::SetSchedule(_)
        );
        if needs_night_shift && night_shift == NightShift::Unavailable {
            return Err(refused(
                ErrorCode::NightShiftUnavailable,
                "Night Shift isn't available on this Mac",
            ));
        }
        let mut state = self.state();
        let outcome = match command {
            Command::On => {
                state.set_manual_state(OnOff::On);
                Ok(())
            }
            Command::Off => {
                state.set_manual_state(OnOff::Off);
                Ok(())
            }
            Command::Toggle => {
                state.set_manual_state(on_off(night_shift != NightShift::On));
                Ok(())
            }
            Command::Disable(duration) => match self.clock.now().checked_add(*duration) {
                Ok(until) => {
                    tracing::info!("timed disable until {until}");
                    state.timed_disable = Some(until);
                    Ok(())
                }
                Err(_) => Err(refused(
                    ErrorCode::BadRequest,
                    format!("{duration:#} is too long"),
                )),
            },
            Command::Resume => {
                tracing::info!("resumed");
                state.timed_disable = None;
                state.snoozes.clear();
                state.manual_state = None;
                Ok(())
            }
            Command::AddRule(identity) => self.add_rule(&mut state, identity),
            Command::RemoveRule(identity) => self.remove_rule(&mut state, identity),
            Command::Snooze(identity) => state.snooze(identity, true),
            Command::Unsnooze(identity) => state.snooze(identity, false),
            Command::OpenAtLogin(chosen) => {
                if self.updates_itself() {
                    state.open_at_login = *chosen == OnOff::On;
                    effects.push(match chosen {
                        OnOff::On => Effect::RegisterLoginItem,
                        OnOff::Off => Effect::UnregisterLoginItem,
                    });
                    Ok(())
                } else {
                    Err(refused(
                        ErrorCode::NoLoginItem,
                        "this install doesn't open itself at login: its installer starts it, or it's a dev build",
                    ))
                }
            }
            Command::Strength(value) | Command::PreviewStrength(value) => {
                if state.strength.is_none() {
                    Err(refused(
                        ErrorCode::StrengthUnavailable,
                        "Night Shift's strength can't be read on this Mac",
                    ))
                } else if *value > 100 {
                    Err(refused(
                        ErrorCode::BadRequest,
                        format!("a strength is 0 to 100, not {value}"),
                    ))
                } else {
                    let commit = matches!(command, Command::Strength(_));
                    if commit {
                        tracing::info!("strength set to {value}");
                    } else {
                        tracing::debug!("previewing strength {value}");
                    }
                    effects.push(Effect::SetStrength {
                        value: *value,
                        commit,
                    });
                    Ok(())
                }
            }
            Command::SetSchedule(schedule) => {
                let time_of_day = |time: &TimeOfDay| time.hour < 24 && time.minute < 60;
                if let Schedule::Custom { from, to } = schedule
                    && !(time_of_day(from) && time_of_day(to))
                {
                    Err(refused(
                        ErrorCode::BadRequest,
                        "custom hours are times of day, from 00:00 to 23:59",
                    ))
                } else {
                    tracing::info!("schedule set on command");
                    state.keep_on_with_schedule_off =
                        *schedule == Schedule::Off && night_shift == NightShift::On;
                    effects.push(Effect::SetSchedule {
                        schedule: *schedule,
                    });
                    Ok(())
                }
            }
            Command::Websites(chosen) => {
                let on = *chosen == OnOff::On;
                let turned_on = on && !state.website_checking();
                state.set_websites(on).map(|()| {
                    if turned_on && state.accessibility != Some(true) {
                        effects.push(Effect::AskForAccessibility);
                    }
                })
            }
        };
        outcome.map(|()| state.decide())
    }

    /// Reconciles a status callback with what the core wants (ADR-0005). macOS catching up on a
    /// transition the core already applied, or running just ahead of the next one, is that
    /// transition; a match needs nothing; anything else is an external change, handled like the
    /// equivalent command (ADR-0006).
    fn reconcile(&self, state: &mut State, enabled: bool) -> Option<Effect> {
        if state.left_by_last_run.take() == Some(on_off(enabled)) {
            tracing::info!("Night Shift is where the last run left it");
            return None;
        }
        let now = self.clock.now();
        match state.unreported.take() {
            Some(unreported) if now <= unreported.until && enabled == unreported.scheduled => {
                tracing::info!(
                    "macOS performed the schedule transition at {}, {:#} late",
                    unreported.transition,
                    now.duration_since(unreported.transition)
                );
                return None;
            }
            Some(unreported) if now <= unreported.until => state.unreported = Some(unreported),
            _ => {}
        }
        let wanted = wanted(&state.decide())?;
        if enabled == wanted {
            return None;
        }
        if let Some(transition) = state.timeline.next_transition(state.applied_until)
            && transition.duration_since(now) <= TRANSITION_TOLERANCE
            && state.timeline.scheduled_state(transition) == Some(enabled)
        {
            tracing::info!(
                "schedule transition at {transition}, reported {:#} early",
                transition.duration_since(now)
            );
            state.pass_transition();
            state.applied_until = transition;
            return None;
        }
        tracing::info!(
            "Night Shift turned {} outside Nightcap",
            if enabled { "on" } else { "off" }
        );
        let overridden = state.set_manual_state(on_off(enabled));
        state.collision_notice(&overridden)
    }

    /// Applies every schedule transition that came due since the last event: the manual state
    /// lasts only until the next one.
    fn catch_up(&self) {
        let now = self.clock.now();
        let mut state = self.state();
        if now <= state.applied_until {
            return;
        }
        if state.timed_disable.is_some_and(|until| until <= now) {
            tracing::info!("the timed disable ended");
            state.timed_disable = None;
        }
        if let Some(transition) = state.timeline.next_transition(state.applied_until)
            && transition <= now
        {
            tracing::info!("schedule transition at {transition}");
            state.pass_transition();
            state.unreported = state
                .timeline
                .scheduled_state(now)
                .map(|scheduled| Unreported {
                    transition,
                    scheduled,
                    until: now + TRANSITION_TOLERANCE,
                });
        }
        state.applied_until = now;
    }

    /// Posts one notification when a problem appears, unless `aloud` is false because the user
    /// caused it themselves. A problem that ends and comes back is announced again.
    fn announce(&self, aloud: bool, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        let problems: Vec<_> = state
            .checks()
            .into_iter()
            .filter(|check| matches!(check.state, CheckState::Drifted | CheckState::Broken))
            .map(|check| check.kind)
            .collect();
        state.announced.retain(|kind| problems.contains(kind));
        for kind in problems {
            if state.announced.contains(&kind) {
                continue;
            }
            state.announced.push(kind);
            if let Some((title, body)) = state.notice(kind).filter(|_| aloud) {
                effects.push(Effect::Notify { title, body });
            }
        }
    }

    /// Watches the frontmost browser while website checking is on, unless its own app rule
    /// forces it off (ADR-0016).
    fn rewatch(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        let wanted = state.context.app.clone().filter(|_| {
            state.is_browser && state.website_checking() && !state.front_app_has_rule()
        });
        match (&state.watch, wanted) {
            (Some(watch), Some(bundle_id)) if watch.bundle_id == bundle_id => {}
            (_, Some(bundle_id)) => {
                state.generation += 1;
                let generation = state.generation;
                state.context.host = None;
                state.watch = Some(Watch {
                    bundle_id: bundle_id.clone(),
                    generation,
                    reported: false,
                });
                effects.push(Effect::WatchBrowser {
                    bundle_id,
                    generation,
                });
            }
            (Some(_), None) => {
                state.context.host = None;
                state.watch = None;
                effects.push(Effect::StopWatching);
            }
            (None, None) => {}
        }
    }

    /// Drops the ad-hoc rules the config now has, which have graduated (ADR-0009), and lays out
    /// the rules the engine sees.
    fn graduate(&self) {
        let mut state = self.state();
        let config = state.config_rules.clone();
        let before = state.ad_hoc.len();
        state.ad_hoc.retain(|identity| {
            !config
                .iter()
                .any(|rule| same_rule(&rule.identity, identity))
        });
        let graduated = before - state.ad_hoc.len();
        if graduated > 0 {
            tracing::info!("{graduated} ad-hoc rules graduated into the config");
            if let Err(error) = self.ad_hoc_store.save(&state.ad_hoc) {
                tracing::warn!("couldn't save rules.toml: {error}");
            }
        }
        state.lay_out_rules();
    }

    fn add_rule(&self, state: &mut State, identity: &RuleIdentity) -> Result<(), ErrorReply> {
        let identity = normalize_identity(identity)?;
        if state
            .rules
            .iter()
            .any(|rule| same_rule(&rule.identity, &identity))
        {
            return Ok(());
        }
        state.check_ad_hoc_rules_on()?;
        state.ad_hoc.push(identity.clone());
        if let Err(error) = self.ad_hoc_store.save(&state.ad_hoc) {
            state.ad_hoc.pop();
            return Err(refused(
                ErrorCode::AdHocRulesOff,
                format!("couldn't save rules.toml: {error}"),
            ));
        }
        tracing::info!("added an ad-hoc rule: {}", loggable(&identity));
        state.lay_out_rules();
        Ok(())
    }

    fn remove_rule(&self, state: &mut State, identity: &RuleIdentity) -> Result<(), ErrorReply> {
        let identity = normalize_identity(identity)?;
        let Some(index) = state
            .ad_hoc
            .iter()
            .position(|kept| same_rule(kept, &identity))
        else {
            if state
                .config_rules
                .iter()
                .any(|rule| same_rule(&rule.identity, &identity))
            {
                return Err(refused(
                    ErrorCode::ConfigRule,
                    format!(
                        "{identity} comes from the config; snooze it instead with `nightcap rules snooze {identity}`"
                    ),
                ));
            }
            return Err(refused(
                ErrorCode::NoSuchRule,
                format!("{identity} isn't a rule"),
            ));
        };
        state.check_ad_hoc_rules_on()?;
        let removed = state.ad_hoc.remove(index);
        if let Err(error) = self.ad_hoc_store.save(&state.ad_hoc) {
            state.ad_hoc.insert(index, removed);
            return Err(refused(
                ErrorCode::AdHocRulesOff,
                format!("couldn't save rules.toml: {error}"),
            ));
        }
        tracing::info!("removed an ad-hoc rule: {}", loggable(&identity));
        state
            .snoozes
            .retain(|snoozed| !same_rule(snoozed, &identity));
        state.lay_out_rules();
        Ok(())
    }

    /// Writes `state.json` whenever what it holds changes, so a crash loses nothing (ADR-0011).
    fn save(&self) {
        let mut state = self.state();
        let snapshot = state.snapshot();
        if snapshot == state.saved {
            return;
        }
        match self.store.save(&snapshot) {
            Ok(()) => state.saved = snapshot,
            Err(error) => tracing::warn!("couldn't save the state: {error}"),
        }
    }

    /// Keeps the shell's one wakeup at the next schedule transition or the end of the timed
    /// disable, whichever comes first.
    fn reschedule(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        let next = state
            .timeline
            .next_transition(state.applied_until)
            .into_iter()
            .chain(state.timed_disable)
            .min();
        if next != state.wakeup {
            state.wakeup = next;
            effects.push(match next {
                Some(at) => Effect::ScheduleWakeup { at: at.into() },
                None => Effect::CancelWakeup,
            });
        }
    }

    /// Writes Night Shift when what the core wants differs from what was last observed
    /// (ADR-0005), once until a status callback reports the result.
    fn enforce(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        if state.night_shift == NightShift::Unavailable || state.session != Session::Active {
            return;
        }
        let decision = state.decide();
        // A new watch keeps Night Shift as it is until its first read (ADR-0016), unless it's
        // forced off whatever the host turns out to be.
        let reading = state.watch.as_ref().is_some_and(|watch| !watch.reported);
        if reading && decision.verdict == Verdict::HandsOff {
            return;
        }
        let Some(wanted) = wanted(&decision) else {
            return;
        };
        if wanted != (state.night_shift == NightShift::On) && state.pending_write != Some(wanted) {
            state.pending_write = Some(wanted);
            effects.push(Effect::SetNightShift { enabled: wanted });
        }
    }

    fn status(&self) -> Response {
        let state = self.state();
        Response::Ok(Payload::Status(Box::new(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: self.install,
            night_shift: state.night_shift,
            strength: state.strength,
            config_strength: state.config_night_shift.strength,
            decision: state.decide(),
            manual_state: state.manual_state,
            schedule: state.timeline.schedule,
            config_schedule: state.config_night_shift.schedule,
            scheduled_state: state
                .timeline
                .scheduled_state(state.applied_until)
                .map(on_off),
            next_transition: state.timeline.next_transition(state.applied_until),
            frontmost_app: state.context.app.clone(),
            watching: state.watch.is_some(),
            host: state.context.host.clone(),
            rules: state.rules.clone(),
            config_error: state.config_error.clone(),
            ad_hoc_error: state.ad_hoc_error.clone(),
            website_checking: WebsiteChecking {
                state: on_off(state.website_checking()),
                from_config: state.config_websites.is_some(),
            },
            snoozes: state.snoozes.clone(),
            timed_disable: state.timed_disable,
            permissions: Permissions {
                accessibility: state.accessibility == Some(true),
                notifications: state.notifications,
            },
            checks: state.checks(),
        })))
    }
}

/// A schedule transition the core applied on its own clock that macOS hasn't reported
/// performing yet. Until `until`, a callback matching it is macOS catching up, not an external
/// change.
struct Unreported {
    transition: Timestamp,
    /// Whether the schedule has Night Shift on after the transition.
    scheduled: bool,
    until: Timestamp,
}

/// Whether Night Shift is this user's to change: with fast user switching, another user's
/// session can be in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Session {
    Active,
    /// Another user is in front: status callbacks aren't this user's, and nothing is written.
    Away,
    /// Back in front, waiting for the shell's fresh read of a status that went unwatched.
    Back,
}

/// The browser being watched, and whether its first read under this generation has arrived.
struct Watch {
    bundle_id: String,
    generation: u64,
    reported: bool,
}

/// What turning Night Shift on overrode.
#[derive(Default)]
struct Overridden {
    snoozed: Vec<RuleIdentity>,
    timed_disable: bool,
}

impl State {
    /// Sets the manual state the way `on`, `off` and external changes do (ADR-0006): turning
    /// Night Shift on while it's forced off snoozes the forcing rules and cancels a timed
    /// disable. Short of a force, a state that a switching schedule gives anyway is no manual
    /// state, so turning Night Shift off and back on follows the schedule again.
    fn set_manual_state(&mut self, state: OnOff) -> Overridden {
        let mut overridden = Overridden::default();
        let decision = self.decide();
        if state == OnOff::On && decision.verdict == Verdict::ForceOff {
            for reason in decision.reasons {
                match reason {
                    Reason::Rule(rule) => overridden.snoozed.push(rule.identity),
                    Reason::TimedDisable { .. } => overridden.timed_disable = true,
                    _ => {}
                }
            }
            for identity in &overridden.snoozed {
                tracing::info!(
                    "snoozed {} until the next schedule transition",
                    loggable(identity)
                );
            }
            self.snoozes.extend(overridden.snoozed.iter().cloned());
            if overridden.timed_disable {
                tracing::info!("cancelled the timed disable");
                self.timed_disable = None;
            }
        }
        let overrides = !overridden.snoozed.is_empty() || overridden.timed_disable;
        let scheduled = self.timeline.next_transition(self.applied_until).is_some()
            && self.timeline.scheduled_state(self.applied_until).map(on_off) == Some(state);
        self.manual_state = (overrides || !scheduled).then_some(state);
        overridden
    }

    /// Tells the user what an external change overrode, if it collided with a force (ADR-0006).
    fn collision_notice(&self, overridden: &Overridden) -> Option<Effect> {
        if overridden.snoozed.is_empty() && !overridden.timed_disable {
            return None;
        }
        let until = match self.timeline.next_transition(self.applied_until) {
            Some(transition) => format!(
                "until {}",
                self.timeline.zone.to_datetime(transition).strftime("%H:%M")
            ),
            None => "until you resume".to_owned(),
        };
        let mut sentences = Vec::new();
        if !overridden.snoozed.is_empty() {
            let snoozed: Vec<_> = overridden.snoozed.iter().map(ToString::to_string).collect();
            sentences.push(format!("Snoozed {} {until}.", snoozed.join(", ")));
        }
        if overridden.timed_disable {
            sentences.push("Cancelled the timed disable.".to_owned());
        }
        Some(Effect::Notify {
            title: "Night Shift turned on outside Nightcap".to_owned(),
            body: sentences.join(" "),
        })
    }

    fn snapshot(&self) -> state::Persisted {
        state::Persisted {
            manual_state: self.manual_state,
            snoozes: self.snoozes.clone(),
            timed_disable: self.timed_disable,
            websites: self.websites,
            onboarding: self.onboarding,
            notifications_wanted: self.notifications_wanted,
            open_at_login: self.open_at_login,
            announced: self.announced.clone(),
            night_shift: wanted(&self.decide()).map(on_off),
            until: self.timeline.next_transition(self.applied_until),
            applied_from_config: self.applied_from_config,
        }
    }

    fn onboarding_night_shift(&self) -> OnboardingNightShift {
        let available = self.night_shift != NightShift::Unavailable;
        let ask_strength = self.strength.is_some() && self.config_night_shift.strength.is_none();
        let ask_schedule = available && self.config_night_shift.schedule.is_none();
        OnboardingNightShift {
            ask_strength,
            ask_schedule,
            already_set_up: (ask_strength || ask_schedule)
                && self
                    .timeline
                    .schedule
                    .is_some_and(|schedule| schedule != Schedule::Off),
            strength: self.strength,
            schedule: self.timeline.schedule,
        }
    }

    /// Writes the config's strength and schedule where they changed since last applied, once
    /// Night Shift, and for the strength its value, is known (ADR-0020).
    fn apply_config_night_shift(&mut self, effects: &mut Vec<Effect>) {
        if self.night_shift == NightShift::Unavailable {
            return;
        }
        let wanted = self.config_night_shift;
        if wanted.strength != self.applied_from_config.strength && self.strength.is_some() {
            if let Some(value) = wanted.strength {
                tracing::info!("strength set to {value} from the config");
                effects.push(Effect::SetStrength {
                    value,
                    commit: true,
                });
            }
            self.applied_from_config.strength = wanted.strength;
        }
        if wanted.schedule != self.applied_from_config.schedule {
            if let Some(schedule) = wanted.schedule {
                tracing::info!("schedule set from the config");
                effects.push(Effect::SetSchedule { schedule });
            }
            self.applied_from_config.schedule = wanted.schedule;
        }
    }

    /// The menu's toggle for the frontmost app (ADR-0009).
    fn app_toggle(&self, bundle_id: &str) -> MenuItem {
        let name = self.app_name.as_deref().unwrap_or(bundle_id).to_owned();
        let identity = RuleIdentity::App(bundle_id.to_owned());
        let rule = self
            .rules
            .iter()
            .find(|rule| same_rule(&rule.identity, &identity));
        self.rule_toggle(rule, identity, |_| format!("In {name}"))
    }

    /// The menu's toggle for the front tab's site, while website checking knows its host: it
    /// adds a rule for the registrable domain, or acts on the most specific rule covering the
    /// host (ADR-0009).
    fn site_toggle(&self) -> Option<MenuItem> {
        let host = self
            .context
            .host
            .as_deref()
            .filter(|_| self.website_checking())?;
        let rule = self
            .rules
            .iter()
            .filter_map(|rule| match &rule.identity {
                RuleIdentity::Domain(domain) if domain::covers(domain, host) => {
                    Some((domain.len(), rule))
                }
                _ => None,
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, rule)| rule);
        let identity = RuleIdentity::Domain(domain::registrable(host));
        let toggle = self.rule_toggle(rule, identity, |identity| match identity {
            RuleIdentity::Domain(domain) => format!("On {domain}"),
            RuleIdentity::App(bundle_id) => format!("In {bundle_id}"),
        });
        // The site sits under its browser's own toggle.
        Some(MenuItem {
            indented: true,
            ..toggle
        })
    }

    /// A rule toggle (ADR-0009): with no `rule` it adds `new`; otherwise it removes an ad-hoc
    /// rule, snoozes a config rule, or undoes a snooze. `title` names the identity it acts on.
    fn rule_toggle(
        &self,
        rule: Option<&Rule>,
        new: RuleIdentity,
        title: impl Fn(&RuleIdentity) -> String,
    ) -> MenuItem {
        let toggle = |title: String, subtitle: Option<String>, checked, command| MenuItem {
            title,
            subtitle,
            indented: false,
            checked,
            enabled: true,
            command,
        };
        let Some(rule) = rule else {
            return toggle(title(&new), None, false, Command::AddRule(new));
        };
        let identity = rule.identity.clone();
        let name = title(&identity);
        if self
            .snoozes
            .iter()
            .any(|snoozed| same_rule(snoozed, &identity))
        {
            toggle(
                name,
                Some(format!("Snoozed{}", self.until_snoozes_end())),
                false,
                Command::Unsnooze(identity),
            )
        } else if rule.source == RuleSource::Config {
            toggle(
                name,
                Some("From your config".to_owned()),
                true,
                Command::Snooze(identity),
            )
        } else {
            toggle(name, None, true, Command::RemoveRule(identity))
        }
    }

    /// A rule's row in the Rules tab, acted on as the menu's toggles are (ADR-0009): a config
    /// rule is snoozed and unsnoozed, an ad-hoc one removed.
    fn settings_rule(&self, rule: &Rule) -> SettingsRule {
        let identity = rule.identity.clone();
        let snoozed = self
            .snoozes
            .iter()
            .any(|snoozed| same_rule(snoozed, &identity));
        let (label, title, command) = if snoozed {
            (
                format!("Snoozed{}", self.until_snoozes_end()),
                "Unsnooze",
                Command::Unsnooze(identity.clone()),
            )
        } else if rule.source == RuleSource::Config {
            (
                "From your config".to_owned(),
                "Snooze",
                Command::Snooze(identity.clone()),
            )
        } else {
            (
                "Ad-hoc".to_owned(),
                "Remove",
                Command::RemoveRule(identity.clone()),
            )
        };
        SettingsRule {
            identity,
            label,
            snoozed,
            action: RuleAction {
                title: title.to_owned(),
                command,
            },
        }
    }

    /// " until 07:00" for snoozes made now, which last until the next schedule transition.
    fn until_snoozes_end(&self) -> String {
        self.timeline
            .next_transition(self.applied_until)
            .map_or_else(String::new, |at| format!(" until {}", self.clock_time(at)))
    }

    /// What "Turn Night Shift On" does during a force (ADR-0006); nothing to say otherwise.
    fn what_turning_on_does(&self, decision: &Decision) -> Option<String> {
        if decision.verdict != Verdict::ForceOff {
            return None;
        }
        let rules: Vec<_> = decision
            .reasons
            .iter()
            .filter_map(|reason| match reason {
                Reason::Rule(rule) => Some(self.name_in_front(&rule.identity)),
                _ => None,
            })
            .collect();
        let mut parts = Vec::new();
        if !rules.is_empty() {
            let noun = if rules.len() == 1 { "rule" } else { "rules" };
            parts.push(format!(
                "snoozes the {} {noun}{}",
                rules.join(" and "),
                self.until_snoozes_end()
            ));
        }
        if decision
            .reasons
            .iter()
            .any(|reason| matches!(reason, Reason::TimedDisable { .. }))
        {
            parts.push("ends the timed disable".to_owned());
        }
        capitalized(&listed(&parts))
    }

    /// What "Resume" clears: the timed disable, the snoozes and the manual state.
    fn what_resuming_ends(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.timed_disable.is_some() {
            parts.push("ends the timed disable".to_owned());
        }
        if !self.snoozes.is_empty() {
            let names: Vec<_> = self
                .snoozes
                .iter()
                .map(|identity| self.name_in_front(identity))
                .collect();
            let noun = if names.len() == 1 { "rule" } else { "rules" };
            parts.push(format!("unsnoozes the {} {noun}", names.join(" and ")));
        }
        if self.manual_state.is_some() {
            parts.push("goes back to the schedule".to_owned());
        }
        capitalized(&listed(&parts))
    }

    /// A time of day in the user's time zone, as the menu shows it.
    fn clock_time(&self, at: Timestamp) -> String {
        at.to_zoned(self.timeline.zone.clone())
            .strftime("%H:%M")
            .to_string()
    }

    fn website_checking(&self) -> bool {
        self.config_websites.unwrap_or(self.websites)
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
                    .filter(|_| self.config_websites.is_none()),
            ),
            (CheckKind::Notifications, CheckState::Drifted) => (
                button("Allow Notifications…", Fix::AllowNotifications),
                button("Keep Notifications Off", Fix::KeepNotificationsOff),
            ),
            (CheckKind::Notifications, CheckState::Off) => (
                button("Allow Notifications…", Fix::AllowNotifications),
                None,
            ),
            (CheckKind::OpenAtLogin, CheckState::Drifted) => (
                // Registering again doesn't help an item that's waiting for approval.
                if self.login_item == Some(LoginItem::RequiresApproval) {
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

    /// The menu's problem line, from the checks. Night Shift's own is left to the status line.
    fn problem_line(&self) -> Option<ProblemLine> {
        let problems: Vec<_> = self
            .checks()
            .into_iter()
            .filter(|check| {
                check.kind != CheckKind::NightShift
                    && matches!(check.state, CheckState::Drifted | CheckState::Broken)
            })
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
            subtitle: capitalized(&listed(&names)),
        })
    }

    /// Everything Nightcap depends on, compared with what the user set up.
    fn checks(&self) -> Vec<Check> {
        let check = |kind, state, text: &str| Check {
            kind,
            state,
            text: text.to_owned(),
        };
        let mut checks = vec![if self.night_shift == NightShift::Unavailable {
            check(
                CheckKind::NightShift,
                CheckState::Broken,
                "Night Shift isn’t available on this Mac.",
            )
        } else {
            let time = |at: TimeOfDay| format!("{:02}:{:02}", at.hour, at.minute);
            check(
                CheckKind::NightShift,
                CheckState::Ok,
                &match self.timeline.schedule {
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
        if let Some(granted) = self.accessibility {
            checks.push(match (self.website_checking(), granted) {
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
                    self.notifications == NotificationPermission::Granted,
                    self.notifications_wanted,
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
        if let Some(item) = self.login_item.filter(|_| self.opens_itself_at_login) {
            checks.push(match (item, self.open_at_login) {
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
        checks.push(match (&self.config_error, self.config_found) {
            (Some(error), _) => check(
                CheckKind::ConfigFile,
                CheckState::Broken,
                &format!("The config file is invalid: {error}"),
            ),
            (None, true) => check(
                CheckKind::ConfigFile,
                CheckState::Ok,
                &match self.config_rules.len() {
                    0 => "Valid, with no rules.".to_owned(),
                    1 => "Valid, with 1 rule.".to_owned(),
                    count => format!("Valid, with {count} rules."),
                },
            ),
            (None, false) => check(CheckKind::ConfigFile, CheckState::Off, "No config file."),
        });
        checks.push(match (&self.ad_hoc_error, self.ad_hoc.len()) {
            (Some(error), _) => check(
                CheckKind::AdHocRules,
                CheckState::Broken,
                &format!("Ad-hoc rules are off: {error}"),
            ),
            (None, 0) => check(CheckKind::AdHocRules, CheckState::Ok, "No ad-hoc rules."),
            (None, 1) => check(CheckKind::AdHocRules, CheckState::Ok, "1 ad-hoc rule."),
            (None, count) => check(
                CheckKind::AdHocRules,
                CheckState::Ok,
                &format!("{count} ad-hoc rules."),
            ),
        });
        checks
    }

    /// Whether an unsnoozed app rule covers the frontmost app, which forces Night Shift off
    /// whatever a browser's tab shows, so its host doesn't matter.
    fn front_app_has_rule(&self) -> bool {
        self.context.app.as_ref().is_some_and(|bundle_id| {
            let identity = RuleIdentity::App(bundle_id.clone());
            self.rules
                .iter()
                .any(|rule| same_rule(&rule.identity, &identity))
                && !self
                    .snoozes
                    .iter()
                    .any(|snoozed| same_rule(snoozed, &identity))
        })
    }

    /// The reason under the menu's status line, naming what's in front rather than rules.
    fn menu_reason(&self, decision: &Decision) -> Option<String> {
        let until = |at: &Option<Timestamp>| {
            at.map_or_else(String::new, |at| format!(" until {}", self.clock_time(at)))
        };
        if decision.verdict == Verdict::HandsOff {
            return decision.reasons.iter().find_map(|reason| match reason {
                Reason::ManualState { state, until: end } => {
                    Some(format!("Turned {state} by hand{}", until(end)))
                }
                Reason::Schedule { until: end, .. } => {
                    Some(format!("Following the schedule{}", until(end)))
                }
                _ => None,
            });
        }
        let in_front: Vec<_> = decision
            .reasons
            .iter()
            .filter_map(|reason| match reason {
                Reason::Rule(rule) => Some(self.name_in_front(&rule.identity)),
                _ => None,
            })
            .collect();
        let mut parts = Vec::new();
        if !in_front.is_empty() {
            let verb = if in_front.len() == 1 { "is" } else { "are" };
            parts.push(format!("while {} {verb} in front", in_front.join(" and ")));
        }
        parts.extend(decision.reasons.iter().filter_map(|reason| match reason {
            Reason::TimedDisable { until: end } => Some(format!("until {}", self.clock_time(*end))),
            _ => None,
        }));
        capitalized(&parts.join(", and "))
    }

    /// An app rule's app by its name when it's the frontmost one, and a domain rule's domain.
    fn name_in_front(&self, identity: &RuleIdentity) -> String {
        match identity {
            RuleIdentity::App(bundle_id) => self
                .app_name
                .clone()
                .filter(|_| {
                    self.context
                        .app
                        .as_deref()
                        .is_some_and(|front| front.eq_ignore_ascii_case(bundle_id))
                })
                .unwrap_or_else(|| bundle_id.clone()),
            RuleIdentity::Domain(domain) => domain.clone(),
        }
    }

    /// The notification for a problem that appeared on its own; Night Shift's is a fact about
    /// the Mac, and switched-off notifications can't announce themselves.
    fn notice(&self, kind: CheckKind) -> Option<(String, String)> {
        match kind {
            CheckKind::Accessibility => Some((
                "Website checking needs Accessibility".to_owned(),
                "Open Nightcap’s settings to allow it, or to stop checking websites.".to_owned(),
            )),
            CheckKind::ConfigFile => self
                .config_error
                .clone()
                .map(|error| ("Nightcap’s config is invalid".to_owned(), error)),
            CheckKind::AdHocRules => self
                .ad_hoc_error
                .clone()
                .map(|error| ("Ad-hoc rules are off".to_owned(), error)),
            // Waiting for approval follows the user's own choice, which macOS announces itself.
            CheckKind::OpenAtLogin => {
                (self.login_item == Some(LoginItem::NotRegistered)).then(|| {
                    (
                        "Nightcap no longer opens at login".to_owned(),
                        "Open Nightcap’s settings to turn it back on, or to keep it off."
                            .to_owned(),
                    )
                })
            }
            CheckKind::NightShift | CheckKind::Notifications => None,
        }
    }

    fn set_websites(&mut self, on: bool) -> Result<(), ErrorReply> {
        if self.config_websites.is_some() {
            return Err(refused(
                ErrorCode::SetInConfig,
                "website checking is set by `websites` in the config",
            ));
        }
        tracing::info!("website checking {}", if on { "on" } else { "off" });
        self.websites = on;
        Ok(())
    }

    /// The config rules, then the ad-hoc rules the config doesn't have.
    fn lay_out_rules(&mut self) {
        let ad_hoc = self.ad_hoc.iter().cloned().map(|identity| Rule {
            identity,
            source: RuleSource::AdHoc,
        });
        self.rules = self.config_rules.iter().cloned().chain(ad_hoc).collect();
    }

    fn check_ad_hoc_rules_on(&self) -> Result<(), ErrorReply> {
        match &self.ad_hoc_error {
            None => Ok(()),
            Some(error) => Err(refused(
                ErrorCode::AdHocRulesOff,
                format!("ad-hoc rules are off: {error}"),
            )),
        }
    }

    /// Snoozes or unsnoozes an existing rule, config or ad-hoc, until the next schedule
    /// transition.
    fn snooze(&mut self, identity: &RuleIdentity, snoozed: bool) -> Result<(), ErrorReply> {
        let identity = normalize_identity(identity)?;
        let Some(rule) = self
            .rules
            .iter()
            .find(|rule| same_rule(&rule.identity, &identity))
        else {
            return Err(refused(
                ErrorCode::NoSuchRule,
                format!("{identity} isn't a rule"),
            ));
        };
        let rule = rule.identity.clone();
        self.snoozes.retain(|kept| !same_rule(kept, &rule));
        if snoozed {
            tracing::info!(
                "snoozed {} until the next schedule transition",
                loggable(&rule)
            );
            self.snoozes.push(rule);
        }
        Ok(())
    }

    /// Snoozes and the manual state last only until the next schedule transition (ADR-0006).
    fn pass_transition(&mut self) {
        self.manual_state = None;
        self.snoozes.clear();
    }

    fn decide(&self) -> Decision {
        engine::decide(&self.inputs())
    }

    fn inputs(&self) -> Inputs<'_> {
        Inputs {
            context: &self.context,
            rules: if self.released { &[] } else { &self.rules },
            snoozes: &self.snoozes,
            timed_disable: self.timed_disable.filter(|_| !self.released),
            manual_state: self.manual_state,
            scheduled_state: self
                .timeline
                .scheduled_state(self.applied_until)
                .map(on_off),
            next_transition: self.timeline.next_transition(self.applied_until),
        }
    }
}

/// The Night Shift state a decision asks for, if it asks for one.
fn wanted(decision: &Decision) -> Option<bool> {
    match decision.verdict {
        Verdict::ForceOff => Some(false),
        Verdict::HandsOff => decision.reasons.iter().find_map(|reason| match reason {
            Reason::ManualState { state, .. } | Reason::Schedule { state, .. } => {
                Some(*state == OnOff::On)
            }
            Reason::Rule(_) | Reason::TimedDisable { .. } | Reason::Snooze { .. } => None,
        }),
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

/// `text` with its first letter capitalized, or None when it's empty.
fn capitalized(text: &str) -> Option<String> {
    let mut letters = text.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
}

fn refused(code: ErrorCode, message: impl Into<String>) -> ErrorReply {
    ErrorReply {
        code,
        message: message.into(),
    }
}

/// `identity` as rules keep it: a bundle ID is letters, digits, hyphens and periods, and a
/// domain is bare and normalized (ADR-0008).
fn normalize_identity(identity: &RuleIdentity) -> Result<RuleIdentity, ErrorReply> {
    let normalized = match identity {
        RuleIdentity::App(bundle_id) => (!bundle_id.is_empty()
            && bundle_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'))
        .then(|| identity.clone()),
        RuleIdentity::Domain(domain) => domain::normalize(domain).map(RuleIdentity::Domain),
    };
    normalized.ok_or_else(|| {
        refused(
            ErrorCode::BadIdentity,
            format!(
                "{identity} isn't a rule identity like app:com.figma.Desktop or domain:figma.com"
            ),
        )
    })
}

/// Rules with the same identity are the same rule; bundle IDs compare case-insensitively.
fn same_rule(a: &RuleIdentity, b: &RuleIdentity) -> bool {
    match (a, b) {
        (RuleIdentity::App(a), RuleIdentity::App(b)) => a.eq_ignore_ascii_case(b),
        (RuleIdentity::Domain(a), RuleIdentity::Domain(b)) => a == b,
        _ => false,
    }
}

/// A rule identity as the log may show it: `tracing-oslog` logs everything as public, so a
/// domain rule is named by its kind only (ADR-0012).
fn loggable(identity: &RuleIdentity) -> String {
    match identity {
        RuleIdentity::App(_) => identity.to_string(),
        RuleIdentity::Domain(_) => "a domain rule".to_owned(),
    }
}

fn on_off(on: bool) -> OnOff {
    if on { OnOff::On } else { OnOff::Off }
}

#[cfg(test)]
mod tests;
