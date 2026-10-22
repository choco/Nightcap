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
    Instance, Layer, Length, NightShift, NotificationPermission, OnOff, Payload, Permissions,
    Place, PlaceLayers, Query, Reason, Request, Response, Rule, RuleIdentity, RuleSource, Schedule,
    Status, TimeOfDay, WebsiteChecking,
};

use config::NightShiftConfig;
use engine::{Context, Inputs, same_place};
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

uniffi::custom_type!(Timestamp, SystemTime, {
    remote,
    lower: |timestamp| SystemTime::from(timestamp),
    try_lift: |time| Ok(Timestamp::try_from(time)?),
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
pub enum Place {
    Everywhere,
    App(String),
    Domain(String),
}

#[uniffi::remote(Enum)]
pub enum Length {
    For(SignedDuration),
    Until(Timestamp),
    UntilScheduleTurns,
    UntilTurnedBack,
}

#[uniffi::remote(Enum)]
pub enum Command {
    On,
    Off,
    Toggle,
    Set {
        place: Place,
        state: OnOff,
        length: Length,
    },
    Reset(Place),
    AddRule {
        identity: RuleIdentity,
        says: OnOff,
    },
    RemoveRule(RuleIdentity),
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
    /// The menu's first item, after the problem line.
    pub night_shift: NightShiftItem,
    /// The problem line, while any problem exists.
    pub problem_line: Option<ProblemLine>,
    pub entries: Vec<MenuEntry>,
}

/// The menu's Night Shift item: everywhere's switch, with what decides the screen (ADR-0024).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NightShiftItem {
    /// Checked while Night Shift is on everywhere.
    pub on: bool,
    /// What decides the screen, in a few words, e.g. "Off in Figma: your config’s rule".
    pub reason: Option<String>,
    /// None while Night Shift is unavailable, which disables the item.
    pub command: Option<Command>,
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
    /// The apps and sites Night Shift is on or off in for now, with no rule of their own: a
    /// snoozed rule shows on its own row, and everywhere's on the menu's Night Shift item.
    pub for_now: Vec<SettingsLayers>,
}

/// An app's or site's temporary on or off as the Rules tab lists it, with ending it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SettingsLayers {
    pub place: Place,
    /// "Night Shift off until 20:00".
    pub label: String,
    pub action: RuleAction,
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

/// A rule as the Rules tab lists it: what it says, its label and what can be done about it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SettingsRule {
    pub identity: RuleIdentity,
    pub says: OnOff,
    /// "From your config", "Snoozed until 07:00" or "Ad-hoc".
    pub label: String,
    pub snoozed: bool,
    /// None for a config rule that isn't snoozed: only the config changes it.
    pub action: Option<RuleAction>,
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

/// The menu's line about problems, above the Night Shift item. Choosing it opens Settings, and
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
    /// The strength's slider, 0 to 100, under the Night Shift item. Moving it sends
    /// `PreviewStrength`, and letting go sends `Strength`.
    Strength {
        value: u8,
    },
    /// A place row: a submenu that turns one place the other way (ADR-0024).
    Row {
        row: PlaceRow,
    },
    Separator,
}

/// A place row: everywhere's under the Night Shift item, then the front app's and site's.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlaceRow {
    pub title: String,
    /// A site's domain, on a line of its own under the title, which says "This Site" so that a
    /// long domain doesn't widen the menu.
    pub site: Option<String>,
    /// What the place says, or for everywhere's row the place that keeps its own say.
    pub subtitle: Option<String>,
    pub image: RowImage,
    /// Drawn one level in, under the row it belongs to: a site under its browser.
    pub indented: bool,
    /// The submenu; empty while there's nothing to offer, which greys the row.
    pub items: Vec<RowItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RowImage {
    /// Everywhere: the timer, as the row under the switch.
    Everywhere,
    /// The app's own icon.
    App {
        bundle_id: String,
    },
    Site,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum RowItem {
    Choice {
        item: MenuItem,
    },
    /// Custom…: the shell asks for an end time in a window titled `title`, starting at
    /// `start`, and sends `Set` for `place` and `state` until the time picked.
    PickTime {
        title: String,
        place: Place,
        state: OnOff,
        start: Timestamp,
    },
    Separator,
}

/// Choosing it sends `command` back as `Event::CommandChosen`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MenuItem {
    pub title: String,
    /// A line under the title, e.g. when a length ends.
    pub subtitle: Option<String>,
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
    /// The tour started (ADR-0022).
    TourStarted,
    /// The tour ended, or the user ended it.
    TourEnded,
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
    timeline: Timeline,
    context: Context,
    /// The config rules and the ad-hoc ones not in the config, as the rule engine sees them.
    rules: Vec<Rule>,
    config_rules: Vec<Rule>,
    config_error: Option<String>,
    /// Whether the config file existed when it was last loaded.
    config_found: bool,
    /// The menu's preset lengths, from the config or else 30 minutes, 1 hour and 2 hours.
    lengths: Vec<SignedDuration>,
    ad_hoc: Vec<(RuleIdentity, OnOff)>,
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
    /// Every place with a hold or a timer (ADR-0024).
    layers: Vec<PlaceLayers>,
    /// The write the shell was last asked for, until a status callback reports the result.
    pending_write: Option<bool>,
    unreported: Option<Unreported>,
    /// Set on the way out to the layers that still apply: only everywhere's hold.
    released: Option<Vec<PlaceLayers>>,
    session: Session,
    /// Where the last run left Night Shift, which the first status callback will show.
    left_by_last_run: Option<OnOff>,
    tour: Option<Tour>,
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
                .filter(|rule| rule.source != RuleSource::Tour)
                .map(|rule| state.settings_rule(rule))
                .collect(),
            for_now: state
                .layers
                .iter()
                .filter(|layers| {
                    layers.place != Place::Everywhere && state.rule_at(&layers.place).is_none()
                })
                .map(|layers| SettingsLayers {
                    place: layers.place.clone(),
                    label: format!("Night Shift {}", state.layer_text(layers)),
                    action: RuleAction {
                        title: "End".to_owned(),
                        command: Command::Reset(layers.place.clone()),
                    },
                })
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
        if state.night_shift == NightShift::Unavailable {
            return Menu {
                icon: MenuIcon::Unavailable,
                night_shift: NightShiftItem {
                    on: false,
                    reason: Some("Unavailable".to_owned()),
                    command: None,
                },
                problem_line: None,
                entries: Vec::new(),
            };
        }
        let now = self.clock.now();
        let decision = state.decide();
        let observed = on_off(state.night_shift == NightShift::On);
        let screen = decision.state.unwrap_or(observed);
        let everywhere = state.says(&Place::Everywhere).unwrap_or(observed);
        let mut entries = Vec::new();
        if let Some(value) = state.strength {
            entries.push(MenuEntry::Strength { value });
        }
        if let Some(row) = state.everywhere_row(&decision, now) {
            entries.push(MenuEntry::Row { row });
        }
        let front: Vec<Place> = state
            .context
            .app
            .clone()
            .map(Place::App)
            .into_iter()
            .chain(state.site_in_front())
            .collect();
        if !front.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        for place in &front {
            entries.push(MenuEntry::Row {
                row: state.place_row(place, now),
            });
        }
        Menu {
            icon: match screen {
                OnOff::Off if decision.place != Place::Everywhere && everywhere == OnOff::On => {
                    MenuIcon::ForcedOff
                }
                OnOff::On => MenuIcon::On,
                OnOff::Off => MenuIcon::Off,
            },
            night_shift: NightShiftItem {
                on: everywhere == OnOff::On,
                reason: state.menu_reason(&decision),
                command: Some(match everywhere {
                    OnOff::On => Command::Off,
                    OnOff::Off => Command::On,
                }),
            },
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
                    state.schedule_edited();
                    if keep_on && schedule == Schedule::Off && !enabled {
                        state.switch(OnOff::On);
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
            Event::TourStarted => {
                let mut state = self.state();
                if state.tour.is_some() {
                    tracing::warn!("a tour started while one runs; keeping the first");
                    return Vec::new();
                }
                let stands_in = state.night_shift == NightShift::Off;
                tracing::info!(
                    "tour started{}",
                    if stands_in {
                        ", standing in for the schedule with Night Shift on"
                    } else {
                        ""
                    }
                );
                let (hold, timer) = if stands_in {
                    let set_aside = state.layers_at(&Place::Everywhere).cloned();
                    state.clear_layers(&Place::Everywhere);
                    set_aside.map_or((None, None), |layers| (layers.hold, layers.timer))
                } else {
                    (None, None)
                };
                state.tour = Some(Tour {
                    rules: Vec::new(),
                    stands_in,
                    hold,
                    timer,
                });
                state.lay_out_rules();
                Vec::new()
            }
            Event::TourEnded => {
                let now = self.clock.now();
                self.state().end_tour(now);
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
                // behind (ADR-0011), nor Night Shift on for a tour (ADR-0022).
                let now = self.clock.now();
                let mut state = self.state();
                state.end_tour(now);
                state.released = Some(
                    state
                        .layers_at(&Place::Everywhere)
                        .and_then(|layers| layers.hold.clone())
                        .map(|hold| PlaceLayers {
                            place: Place::Everywhere,
                            hold: Some(hold),
                            timer: None,
                        })
                        .into_iter()
                        .collect(),
                );
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
                    lengths: config::default_lengths(),
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
                lengths: config.lengths,
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
                layers: persisted.layers.clone(),
                pending_write: None,
                unreported: None,
                released: None,
                session: Session::Active,
                left_by_last_run: persisted.night_shift,
                tour: None,
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
                state.lengths = config.lengths;
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
                | Command::Set { .. }
                | Command::Reset(_)
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
                state.switch(OnOff::On);
                Ok(())
            }
            Command::Off => {
                state.switch(OnOff::Off);
                Ok(())
            }
            Command::Toggle => {
                let everywhere = state
                    .says(&Place::Everywhere)
                    .unwrap_or(on_off(night_shift == NightShift::On));
                state.switch(everywhere.other());
                Ok(())
            }
            Command::Set {
                place,
                state: on,
                length,
            } => state.set(place, *on, *length, self.clock.now()),
            Command::Reset(place) => state.reset(place),
            Command::AddRule { identity, says } => self.add_rule(&mut state, identity, *says),
            Command::RemoveRule(identity) => self.remove_rule(&mut state, identity),
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
            state.expire(transition);
            state.applied_until = transition;
            return None;
        }
        tracing::info!(
            "Night Shift turned {} outside Nightcap",
            if enabled { "on" } else { "off" }
        );
        state.adopt(on_off(enabled))
    }

    /// Applies every schedule transition that came due since the last event, and ends the
    /// timers and holds that ran out.
    fn catch_up(&self) {
        let now = self.clock.now();
        let mut state = self.state();
        if now <= state.applied_until {
            return;
        }
        state.expire(now);
        if let Some(transition) = state.timeline.next_transition(state.applied_until)
            && transition <= now
        {
            tracing::info!("schedule transition at {transition}");
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

    /// Watches the frontmost browser while website checking is on, unless its own place says
    /// off and no site could say on (ADR-0016, ADR-0024).
    fn rewatch(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        let wanted =
            state.context.app.clone().filter(|_| {
                state.is_browser && state.website_checking() && !state.browser_holds_off()
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
        state.ad_hoc.retain(|(identity, _)| {
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

    /// Makes the user's rule for an app or domain say `says`, replacing one of theirs that said
    /// the other and the place's snoozes; a config rule changes only in the config (ADR-0025).
    fn add_rule(
        &self,
        state: &mut State,
        identity: &RuleIdentity,
        says: OnOff,
    ) -> Result<(), ErrorReply> {
        let identity = normalize_identity(identity)?;
        if let Some(rule) = state
            .rules
            .iter()
            .find(|rule| same_rule(&rule.identity, &identity))
        {
            if rule.says == says {
                return Ok(());
            }
            if rule.source == RuleSource::Config {
                return Err(refused(
                    ErrorCode::ConfigRule,
                    format!(
                        "{identity} comes from the config, which says {}; change it there",
                        rule.says
                    ),
                ));
            }
        }
        let kept = state
            .ad_hoc
            .iter()
            .position(|(kept, _)| same_rule(kept, &identity));
        // A rule added during the tour is the tour's, which nothing keeps past it (ADR-0022).
        if kept.is_none()
            && let Some(tour) = &mut state.tour
        {
            tracing::info!("tour rule {}: {says}", loggable(&identity));
            tour.rules.retain(|(rule, _)| !same_rule(rule, &identity));
            tour.rules.push((identity.clone(), says));
        } else {
            state.check_ad_hoc_rules_on()?;
            let before = state.ad_hoc.clone();
            match kept {
                Some(index) => state.ad_hoc[index].1 = says,
                None => state.ad_hoc.push((identity.clone(), says)),
            }
            if let Err(error) = self.ad_hoc_store.save(&state.ad_hoc) {
                state.ad_hoc = before;
                return Err(refused(
                    ErrorCode::AdHocRulesOff,
                    format!("couldn't save rules.toml: {error}"),
                ));
            }
            tracing::info!("ad-hoc rule {}: {says}", loggable(&identity));
        }
        state.clear_layers(&Place::from(identity));
        state.lay_out_rules();
        Ok(())
    }

    fn remove_rule(&self, state: &mut State, identity: &RuleIdentity) -> Result<(), ErrorReply> {
        let identity = normalize_identity(identity)?;
        let place = Place::from(identity.clone());
        if let Some(tour) = &mut state.tour
            && let Some(index) = tour
                .rules
                .iter()
                .position(|(rule, _)| same_rule(rule, &identity))
        {
            tour.rules.remove(index);
            state.clear_layers(&place);
            state.lay_out_rules();
            return Ok(());
        }
        let Some(index) = state
            .ad_hoc
            .iter()
            .position(|(kept, _)| same_rule(kept, &identity))
        else {
            if state
                .config_rules
                .iter()
                .any(|rule| same_rule(&rule.identity, &identity))
            {
                return Err(refused(
                    ErrorCode::ConfigRule,
                    format!("{identity} comes from the config; change it there"),
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
        state.clear_layers(&place);
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

    /// Keeps the shell's one wakeup at the next schedule transition or the end of a timer or
    /// hold, whichever comes first.
    fn reschedule(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        let ends = state
            .layers
            .iter()
            .flat_map(|layers| [&layers.hold, &layers.timer])
            .flatten()
            .filter_map(|layer| layer.until);
        let next = state
            .timeline
            .next_transition(state.applied_until)
            .into_iter()
            .chain(ends)
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
        // A new watch keeps Night Shift as it is until its first read (ADR-0016).
        if state.watch.as_ref().is_some_and(|watch| !watch.reported) {
            return;
        }
        let Some(wanted) = wanted(&state.decide()) else {
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
            layers: state.layers.clone(),
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

/// A tour in progress, and what it set aside (ADR-0022).
struct Tour {
    /// The rules the user added during the tour, which it holds instead of ad-hoc ones.
    rules: Vec<(RuleIdentity, OnOff)>,
    /// Whether the tour has Night Shift on for itself, standing in for the schedule: it was off
    /// as the tour started.
    stands_in: bool,
    /// Everywhere's hold and timer from before the tour stood in, which its end puts back.
    hold: Option<Layer>,
    timer: Option<Layer>,
}

impl State {
    /// The switch (ADR-0024): takes off everywhere's layers that say otherwise, top first, and
    /// if the schedule still says otherwise, holds `state` until the user turns it back.
    fn switch(&mut self, state: OnOff) {
        while self.says(&Place::Everywhere) != Some(state) && self.peel(&Place::Everywhere, state) {
        }
        if self.says(&Place::Everywhere) != Some(state) {
            tracing::info!("switched {state} everywhere until turned back");
            let layers = self.layers_mut(&Place::Everywhere);
            layers.timer = None;
            layers.hold = Some(Layer {
                state,
                until: None,
                with: None,
            });
        }
    }

    /// Takes off a place's top layer if it says other than `state`, with the hold an external
    /// change put beside it, and says whether it did.
    fn peel(&mut self, place: &Place, state: OnOff) -> bool {
        let Some(layers) = self.layers_at_mut(place) else {
            return false;
        };
        let taken = match &layers.timer {
            Some(timer) if timer.state == state => return false,
            Some(_) => layers.timer.take(),
            None => layers.hold.take_if(|hold| hold.state != state),
        };
        let Some(taken) = taken else {
            return false;
        };
        if let Some(with) = &taken.with {
            self.drop_hold_with(with, place);
        }
        self.tidy();
        true
    }

    /// Takes off `place`'s hold if an external change put it there beside `partner`'s.
    fn drop_hold_with(&mut self, place: &Place, partner: &Place) {
        if let Some(layers) = self.layers_at_mut(place)
            && layers.hold.as_ref().is_some_and(|hold| {
                hold.with
                    .as_ref()
                    .is_some_and(|with| same_place(with, partner))
            })
        {
            layers.hold = None;
        }
    }

    /// Puts Night Shift on or off at a place for a length (ADR-0024): a timer for a time or
    /// until a time, which gives back what's under it, and a hold otherwise, replacing the
    /// place's hold and ending its timer.
    fn set(
        &mut self,
        place: &Place,
        state: OnOff,
        length: Length,
        now: Timestamp,
    ) -> Result<(), ErrorReply> {
        let place = normalize_place(place)?;
        let layer = |until| Layer {
            state,
            until,
            with: None,
        };
        match length {
            Length::For(duration) => {
                if duration <= SignedDuration::ZERO {
                    return Err(refused(
                        ErrorCode::BadRequest,
                        "a length is longer than zero",
                    ));
                }
                let until = now.checked_add(duration).map_err(|_| {
                    refused(ErrorCode::BadRequest, format!("{duration:#} is too long"))
                })?;
                self.layers_mut(&place).timer = Some(layer(Some(until)));
            }
            Length::Until(until) => {
                if until <= now {
                    return Err(refused(ErrorCode::BadRequest, "that time has passed"));
                }
                self.layers_mut(&place).timer = Some(layer(Some(until)));
            }
            Length::UntilScheduleTurns => {
                let until = self.next_turn(state).ok_or_else(|| {
                    refused(ErrorCode::BadRequest, "there's no schedule to wait for")
                })?;
                let layers = self.layers_mut(&place);
                layers.timer = None;
                layers.hold = Some(layer(Some(until)));
            }
            Length::UntilTurnedBack => {
                if place != Place::Everywhere {
                    return Err(refused(
                        ErrorCode::BadRequest,
                        "only everywhere holds until turned back: for good in an app or on a site is a rule",
                    ));
                }
                let layers = self.layers_mut(&place);
                layers.timer = None;
                layers.hold = Some(layer(None));
            }
        }
        tracing::info!("{} {state} {length:?}", loggable_place(&place));
        Ok(())
    }

    /// Takes off a place's hold and timer, which puts its rule back, or for everywhere the
    /// schedule.
    fn reset(&mut self, place: &Place) -> Result<(), ErrorReply> {
        let place = normalize_place(place)?;
        self.clear_layers(&place);
        tracing::info!("reset {}", loggable_place(&place));
        Ok(())
    }

    fn clear_layers(&mut self, place: &Place) {
        let Some(index) = self
            .layers
            .iter()
            .position(|layers| same_place(&layers.place, place))
        else {
            return;
        };
        let removed = self.layers.remove(index);
        if let Some(with) = removed.hold.and_then(|hold| hold.with) {
            self.drop_hold_with(&with, place);
            self.tidy();
        }
    }

    /// When the schedule next turns Night Shift away from `state`: sunrise ends an on, sunset
    /// an off. None without a schedule.
    fn next_turn(&self, state: OnOff) -> Option<Timestamp> {
        let first = self.timeline.next_transition(self.applied_until)?;
        if self.timeline.scheduled_state(first).map(on_off) != Some(state) {
            return Some(first);
        }
        self.timeline.next_transition(first)
    }

    /// Takes an external change as flipping what the user sees (ADR-0024): takes off what the
    /// deciding place says, and holds `state` over a rule or the schedule until the schedule
    /// next turns it back, everywhere too where a rule decides and everywhere says otherwise.
    /// Tells the user when that snoozes a rule.
    fn adopt(&mut self, state: OnOff) -> Option<Effect> {
        loop {
            let decision = self.decide();
            if decision.state == Some(state) || !self.peel(&decision.place, state) {
                break;
            }
        }
        let decision = self.decide();
        if decision.state == Some(state) {
            return None;
        }
        let until = self.next_turn(state);
        let deciding = decision.place;
        let mut places = vec![deciding.clone()];
        if deciding != Place::Everywhere {
            while self.says(&Place::Everywhere) != Some(state)
                && self.peel(&Place::Everywhere, state)
            {}
            if self.says(&Place::Everywhere) != Some(state) {
                places.push(Place::Everywhere);
            }
        }
        for (index, place) in places.iter().enumerate() {
            let with = (places.len() > 1).then(|| places[1 - index].clone());
            let layers = self.layers_mut(place);
            layers.timer = None;
            layers.hold = Some(Layer { state, until, with });
        }
        let rule = decision.reasons.iter().find_map(|reason| match reason {
            Reason::Rule(rule) => Some(rule),
            _ => None,
        })?;
        tracing::info!(
            "snoozed {} after an external change",
            loggable(&rule.identity)
        );
        Some(Effect::Notify {
            title: format!("Night Shift turned {state} outside Nightcap"),
            body: format!(
                "Snoozed the {} rule {}.",
                self.name_in_front(&rule.identity),
                self.until_text(until, state)
            ),
        })
    }

    /// Drops every timer and hold that ended by `at`, and what the tour set aside likewise.
    fn expire(&mut self, at: Timestamp) {
        let ended = |layer: &Option<Layer>| {
            layer
                .as_ref()
                .is_some_and(|layer| layer.until.is_some_and(|until| until <= at))
        };
        for layers in &mut self.layers {
            if ended(&layers.timer) {
                tracing::info!("the timer {} ended", loggable_place(&layers.place));
                layers.timer = None;
            }
            if ended(&layers.hold) {
                layers.hold = None;
            }
        }
        if let Some(tour) = &mut self.tour {
            if ended(&tour.timer) {
                tour.timer = None;
            }
            if ended(&tour.hold) {
                tour.hold = None;
            }
        }
        self.tidy();
    }

    /// An edited schedule ends every hold waiting on it (ADR-0024).
    fn schedule_edited(&mut self) {
        let waiting = |hold: &Option<Layer>| hold.as_ref().is_some_and(|hold| hold.until.is_some());
        for layers in &mut self.layers {
            if waiting(&layers.hold) {
                layers.hold = None;
            }
        }
        if let Some(tour) = &mut self.tour
            && waiting(&tour.hold)
        {
            tour.hold = None;
        }
        self.tidy();
    }

    /// Every place's layers once the tour, if one runs, has ended: the tour rules' go, and
    /// everywhere has what the user chose while the tour stood in, or else what it set aside.
    fn after_tour(&self) -> Vec<PlaceLayers> {
        let Some(tour) = &self.tour else {
            return self.layers.clone();
        };
        let mut layers: Vec<_> = self
            .layers
            .iter()
            .filter(|layers| {
                !tour
                    .rules
                    .iter()
                    .any(|(rule, _)| same_place(&Place::from(rule.clone()), &layers.place))
            })
            .cloned()
            .collect();
        if tour.stands_in && (tour.hold.is_some() || tour.timer.is_some()) {
            let index = match layers
                .iter()
                .position(|layers| layers.place == Place::Everywhere)
            {
                Some(index) => index,
                None => {
                    layers.push(PlaceLayers {
                        place: Place::Everywhere,
                        hold: None,
                        timer: None,
                    });
                    layers.len() - 1
                }
            };
            let everywhere = &mut layers[index];
            everywhere.hold = everywhere.hold.take().or_else(|| tour.hold.clone());
            everywhere.timer = everywhere.timer.take().or_else(|| tour.timer.clone());
        }
        layers
    }

    /// Puts back what the tour set aside, short of what ran out meanwhile, and drops the rules
    /// added during it with their layers.
    fn end_tour(&mut self, now: Timestamp) {
        let layers = self.after_tour();
        if self.tour.take().is_none() {
            return;
        }
        tracing::info!("tour ended");
        self.layers = layers;
        self.expire(now);
        self.lay_out_rules();
    }

    /// Mid-tour, the state the tour's end would leave, with Night Shift as the tour holds it: a
    /// run after a crash finds it there, as the last run left it, and puts it back (ADR-0022).
    fn snapshot(&self) -> state::Persisted {
        state::Persisted {
            layers: self.after_tour(),
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

    /// The row under the Night Shift item (ADR-0024): everywhere, whatever is in front, with
    /// everywhere's lengths but the switch's own, and none that would come to the same as
    /// clicking the switch. None when it would offer nothing.
    fn everywhere_row(&self, decision: &Decision, now: Timestamp) -> Option<PlaceRow> {
        let place = Place::Everywhere;
        let state = self.says(&place)?.other();
        let mut items = Vec::new();
        let switched = self.after_switch(state);
        if self.layers_at(&place).is_some()
            && self.has_schedule()
            && self.base_says() == Some(state)
            && (switched.hold.is_some() || switched.timer.is_some())
        {
            items.push(choice(
                "Back to the Schedule",
                Command::Reset(place.clone()),
            ));
        }
        let mut lengths = Vec::new();
        if self.value_below(&place, true) != Some(state) {
            lengths.extend(self.timers(&place, state, now, "Turn {} Everywhere"));
        }
        if self.has_schedule()
            && self.base_says() != Some(state)
            && let Some(until) = self.next_turn(state)
        {
            lengths.push(self.until_turn(&place, state, until));
        }
        if !items.is_empty() && !lengths.is_empty() {
            items.push(RowItem::Separator);
        }
        items.extend(lengths);
        if items.is_empty() {
            return None;
        }
        Some(PlaceRow {
            title: format!(
                "Turn {}",
                capitalized(&state.to_string()).expect("on or off")
            ),
            site: None,
            // The app or site in front keeps the other way, which the row can't change there.
            subtitle: (decision.place != Place::Everywhere && decision.state != Some(state))
                .then(|| format!("Everywhere but {}", self.here(&decision.place))),
            image: RowImage::Everywhere,
            indented: false,
            items,
        })
    }

    /// The row for an app or site in front (ADR-0024): that place alone, with undoing first:
    /// putting its rule back where that goes the right way, and removing a rule of the user's;
    /// then its lengths, then for good, which is a rule.
    fn place_row(&self, place: &Place, now: Timestamp) -> PlaceRow {
        let state = self.value_at(place).unwrap_or(OnOff::Off).other();
        let rule = self.rule_at(place);
        let layered = self.layers_at(place).is_some();
        let inherited = self.inherited(place);
        let mut items = Vec::new();
        let below = rule.map(|rule| rule.says).or(inherited);
        if layered && below == Some(state) {
            let title = if rule.is_some() {
                "Put the Rule Back"
            } else {
                "Like Everywhere Else"
            };
            items.push(choice(title, Command::Reset(place.clone())));
        }
        // Removing a rule is the user's to do whenever they like, even where nothing on screen
        // would change.
        if let Some(rule) = rule
            && rule.source != RuleSource::Config
        {
            items.push(choice(
                "Remove Your Rule",
                Command::RemoveRule(rule.identity.clone()),
            ));
        }
        let mut lengths = Vec::new();
        if self.value_below(place, true) != Some(state) {
            let title = format!("Turn {{}} {}", self.here(place));
            lengths.extend(self.timers(place, state, now, &title));
        }
        if below != Some(state)
            && let Some(until) = self.next_turn(state)
        {
            lengths.push(self.until_turn(place, state, until));
        }
        let always = match rule {
            Some(rule) if rule.says == state => None,
            Some(rule) if rule.source == RuleSource::Config => Some(MenuItem {
                title: "Always".to_owned(),
                subtitle: Some("Your config decides".to_owned()),
                enabled: false,
                command: Command::AddRule {
                    identity: rule.identity.clone(),
                    says: state,
                },
            }),
            _ => place_identity(place).map(|identity| MenuItem {
                title: "Always".to_owned(),
                subtitle: None,
                enabled: true,
                command: Command::AddRule {
                    identity,
                    says: state,
                },
            }),
        };
        if !items.is_empty() && !lengths.is_empty() {
            items.push(RowItem::Separator);
        }
        items.extend(lengths);
        if let Some(item) = always {
            if !items.is_empty() {
                items.push(RowItem::Separator);
            }
            items.push(RowItem::Choice { item });
        }
        let offers = items.iter().any(|item| match item {
            RowItem::Choice { item } => item.enabled,
            RowItem::PickTime { .. } => true,
            RowItem::Separator => false,
        });
        let here = match place {
            Place::Domain(_) => "at This Site".to_owned(),
            _ => self.here(place),
        };
        PlaceRow {
            title: if offers {
                format!(
                    "Turn {} {here}",
                    capitalized(&state.to_string()).expect("on or off")
                )
            } else {
                capitalized(&here).expect("a place has a name")
            },
            site: match place {
                Place::Domain(domain) => Some(domain.clone()),
                _ => None,
            },
            subtitle: self.stack(place),
            image: match place {
                Place::App(bundle_id) => RowImage::App {
                    bundle_id: bundle_id.clone(),
                },
                Place::Domain(_) | Place::Everywhere => RowImage::Site,
            },
            indented: matches!(place, Place::Domain(_)),
            items: if offers { items } else { Vec::new() },
        }
    }

    /// The menu's lengths for a place, then Custom…, whose window `title` names, with `{}` for
    /// on or off.
    fn timers(&self, place: &Place, state: OnOff, now: Timestamp, title: &str) -> Vec<RowItem> {
        let mut items: Vec<_> = self
            .lengths
            .iter()
            .map(|duration| {
                choice(
                    &format!("For {}", length_title(*duration)),
                    Command::Set {
                        place: place.clone(),
                        state,
                        length: Length::For(*duration),
                    },
                )
            })
            .collect();
        let start = (now + SignedDuration::from_hours(1))
            .round(
                jiff::TimestampRound::new()
                    .smallest(jiff::Unit::Minute)
                    .increment(15)
                    .mode(jiff::RoundMode::Ceil),
            )
            .expect("a quarter hour rounds any time");
        items.push(RowItem::PickTime {
            title: title.replace("{}", &capitalized(&state.to_string()).expect("on or off")),
            place: place.clone(),
            state,
            start,
        });
        items
    }

    fn until_turn(&self, place: &Place, state: OnOff, until: Timestamp) -> RowItem {
        // The time alone could be any clock's; the line under it says it's the schedule's.
        let moment = match (&self.timeline.schedule, state) {
            (Some(Schedule::SunsetToSunrise), OnOff::On) => "At sunrise",
            (Some(Schedule::SunsetToSunrise), OnOff::Off) => "At sunset",
            (_, OnOff::On) => "When the schedule ends",
            (_, OnOff::Off) => "When the schedule starts",
        };
        RowItem::Choice {
            item: MenuItem {
                title: format!("Until {}", self.clock_time(until)),
                subtitle: Some(moment.to_owned()),
                enabled: true,
                command: Command::Set {
                    place: place.clone(),
                    state,
                    length: Length::UntilScheduleTurns,
                },
            },
        }
    }

    /// A place's timer and hold, top down: "on until 23:30, then off until 07:00".
    fn layer_text(&self, layers: &PlaceLayers) -> String {
        [&layers.timer, &layers.hold]
            .into_iter()
            .flatten()
            .map(|layer| {
                format!(
                    "{} {}",
                    layer.state,
                    self.until_text(layer.until, layer.state)
                )
            })
            .collect::<Vec<_>>()
            .join(", then ")
    }

    /// What a place holds, top down, for its row: "On until 07:00, then your config’s rule".
    fn stack(&self, place: &Place) -> Option<String> {
        let rule = self.rule_at(place);
        let mut parts: Vec<String> = self
            .layers_at(place)
            .map(|layers| self.layer_text(layers))
            .into_iter()
            .collect();
        match rule {
            Some(rule) if parts.is_empty() => {
                parts.push(format!("always {}: {}", rule.says, whose(rule)))
            }
            Some(rule) => parts.push(whose(rule).to_owned()),
            None => {}
        }
        capitalized(&parts.join(", then "))
    }

    /// The reason under the menu's Night Shift item: what decides the screen.
    fn menu_reason(&self, decision: &Decision) -> Option<String> {
        let top = decision.reasons.first()?;
        let at = match &decision.place {
            Place::Everywhere => String::new(),
            place => format!(" {}", self.here(place)),
        };
        let text = match top {
            Reason::Timer(layer) | Reason::Hold(layer) => format!(
                "{}{at} {}",
                layer.state,
                self.until_text(layer.until, layer.state)
            ),
            Reason::Rule(rule) => format!("{}{at}: {}", rule.says, whose(rule)),
            Reason::Tour => "on for the tour".to_owned(),
            Reason::Schedule { until, .. } => match until {
                Some(until) => format!("following the schedule until {}", self.clock_time(*until)),
                None => "following the schedule".to_owned(),
            },
        };
        capitalized(&text)
    }

    /// Everywhere's hold and timer after the switch to `state`, as `switch` would leave them.
    fn after_switch(&self, state: OnOff) -> PlaceLayers {
        let mut layers = self
            .layers_at(&Place::Everywhere)
            .cloned()
            .unwrap_or(PlaceLayers {
                place: Place::Everywhere,
                hold: None,
                timer: None,
            });
        loop {
            match (&layers.timer, &layers.hold) {
                (Some(timer), _) if timer.state != state => layers.timer = None,
                (None, Some(hold)) if hold.state != state => layers.hold = None,
                _ => break,
            }
        }
        let says = layers
            .timer
            .as_ref()
            .or(layers.hold.as_ref())
            .map(|layer| layer.state)
            .or(self.base_says());
        if says != Some(state) {
            layers.timer = None;
            layers.hold = Some(Layer {
                state,
                until: None,
                with: None,
            });
        }
        layers
    }

    /// A rule's row in the Rules tab: a snooze can be put back, an ad-hoc rule removed, and a
    /// config rule changed only in the config (ADR-0025).
    fn settings_rule(&self, rule: &Rule) -> SettingsRule {
        let identity = rule.identity.clone();
        let place = Place::from(identity.clone());
        let snooze = self
            .layers_at(&place)
            .and_then(|layers| layers.timer.as_ref().or(layers.hold.as_ref()))
            .filter(|layer| layer.state != rule.says);
        let action = |title: &str, command| {
            Some(RuleAction {
                title: title.to_owned(),
                command,
            })
        };
        let (label, action) = match (snooze, rule.source) {
            (Some(layer), _) => (
                format!("Snoozed {}", self.until_text(layer.until, layer.state)),
                action("Put Back", Command::Reset(place)),
            ),
            (None, RuleSource::Config) => ("From your config".to_owned(), None),
            (None, RuleSource::AdHoc) => (
                "Ad-hoc".to_owned(),
                action("Remove", Command::RemoveRule(identity.clone())),
            ),
            (None, RuleSource::Tour) => (
                "For the tour".to_owned(),
                action("Remove", Command::RemoveRule(identity.clone())),
            ),
        };
        SettingsRule {
            identity,
            says: rule.says,
            label,
            snoozed: snooze.is_some(),
            action,
        }
    }

    /// "until 07:00", or "until you turn it on" for an off with no end.
    fn until_text(&self, until: Option<Timestamp>, state: OnOff) -> String {
        match until {
            Some(until) => format!("until {}", self.clock_time(until)),
            None => format!("until you turn it {}", state.other()),
        }
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

    /// The menu's problem line, from the checks. Night Shift's own is left to its item.
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

    /// The config rules, then the ad-hoc rules the config doesn't have, then the tour's.
    fn lay_out_rules(&mut self) {
        let rule = |source| {
            move |(identity, says): &(RuleIdentity, OnOff)| Rule {
                identity: identity.clone(),
                source,
                says: *says,
            }
        };
        let ad_hoc = self.ad_hoc.iter().map(rule(RuleSource::AdHoc));
        let tour = self
            .tour
            .iter()
            .flat_map(|tour| &tour.rules)
            .map(rule(RuleSource::Tour));
        self.rules = self
            .config_rules
            .iter()
            .cloned()
            .chain(ad_hoc)
            .chain(tour)
            .collect();
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

    fn decide(&self) -> Decision {
        engine::decide(&self.inputs())
    }

    /// What the rule engine sees. On the way out, only everywhere's hold still applies: macOS
    /// keeps Night Shift where the user put it until its next transition, but nothing else of
    /// Nightcap's may outlive it (ADR-0011).
    fn inputs(&self) -> Inputs<'_> {
        let released = self.released.as_ref();
        Inputs {
            context: &self.context,
            rules: if released.is_some() { &[] } else { &self.rules },
            layers: released.map_or(&self.layers, |kept| kept),
            tour_stands_in: released.is_none()
                && self.tour.as_ref().is_some_and(|tour| tour.stands_in),
            scheduled_state: self
                .timeline
                .scheduled_state(self.applied_until)
                .map(on_off),
            next_transition: self.timeline.next_transition(self.applied_until),
        }
    }

    /// What a place itself says, or None when it has no layer.
    fn says(&self, place: &Place) -> Option<OnOff> {
        engine::layers_of(&self.inputs(), place)
            .first()
            .map(engine::says)
    }

    /// What everywhere says under its hold and timer: the tour standing in, or the schedule.
    fn base_says(&self) -> Option<OnOff> {
        if self.tour.as_ref().is_some_and(|tour| tour.stands_in) {
            return Some(OnOff::On);
        }
        self.timeline
            .scheduled_state(self.applied_until)
            .map(on_off)
    }

    fn has_schedule(&self) -> bool {
        self.timeline.next_transition(self.applied_until).is_some()
    }

    /// The places from `place` on, as the engine reads them: the less specific ones in front
    /// after it, or, for a place that isn't in front, everywhere.
    fn places_from(&self, place: &Place) -> Vec<Place> {
        let in_front = engine::in_front(&self.inputs());
        match in_front.iter().position(|front| same_place(front, place)) {
            Some(index) => in_front[index..].to_vec(),
            None => vec![place.clone(), Place::Everywhere],
        }
    }

    /// What a place gives: its own say, or the one it inherits.
    fn value_at(&self, place: &Place) -> Option<OnOff> {
        self.places_from(place)
            .iter()
            .find_map(|place| self.says(place))
    }

    /// What a place would give if it said nothing itself.
    fn inherited(&self, place: &Place) -> Option<OnOff> {
        self.places_from(place)
            .iter()
            .skip(1)
            .find_map(|place| self.says(place))
    }

    /// What a place gives under its timer, if `under_timer`, else what it gives.
    fn value_below(&self, place: &Place, under_timer: bool) -> Option<OnOff> {
        let inputs = self.inputs();
        self.places_from(place)
            .iter()
            .enumerate()
            .find_map(|(index, each)| {
                engine::layers_of(&inputs, each)
                    .iter()
                    .find(|reason| {
                        !(index == 0 && under_timer && matches!(reason, Reason::Timer(_)))
                    })
                    .map(engine::says)
            })
    }

    /// The rule for a place, if one says something there.
    fn rule_at(&self, place: &Place) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|rule| same_place(&Place::from(rule.identity.clone()), place))
    }

    fn layers_at(&self, place: &Place) -> Option<&PlaceLayers> {
        self.layers
            .iter()
            .find(|layers| same_place(&layers.place, place))
    }

    fn layers_at_mut(&mut self, place: &Place) -> Option<&mut PlaceLayers> {
        self.layers
            .iter_mut()
            .find(|layers| same_place(&layers.place, place))
    }

    /// A place's layers, made empty if it had none; `tidy` drops it again if it stays empty.
    fn layers_mut(&mut self, place: &Place) -> &mut PlaceLayers {
        match self
            .layers
            .iter()
            .position(|layers| same_place(&layers.place, place))
        {
            Some(index) => &mut self.layers[index],
            None => {
                self.layers.push(PlaceLayers {
                    place: place.clone(),
                    hold: None,
                    timer: None,
                });
                self.layers.last_mut().expect("just pushed")
            }
        }
    }

    fn tidy(&mut self) {
        self.layers
            .retain(|layers| layers.hold.is_some() || layers.timer.is_some());
    }

    /// The site in front, while website checking knows the host: the most specific domain with
    /// a rule or layers covering it, else the tab domain (ADR-0023).
    fn site_in_front(&self) -> Option<Place> {
        let host = self
            .context
            .host
            .as_deref()
            .filter(|_| self.website_checking())?;
        Some(
            engine::in_front(&self.inputs())
                .into_iter()
                .find(|place| matches!(place, Place::Domain(_)))
                .unwrap_or_else(|| Place::Domain(domain::of_tab(host))),
        )
    }

    /// Whether the front browser's own place says off and no site could say on, so its host
    /// doesn't matter.
    fn browser_holds_off(&self) -> bool {
        let Some(app) = &self.context.app else {
            return false;
        };
        let site_could_say_on =
            self.rules.iter().any(|rule| {
                matches!(rule.identity, RuleIdentity::Domain(_)) && rule.says == OnOff::On
            }) || self.layers.iter().any(|layers| {
                matches!(layers.place, Place::Domain(_))
                    && [&layers.hold, &layers.timer]
                        .into_iter()
                        .flatten()
                        .any(|layer| layer.state == OnOff::On)
            });
        self.says(&Place::App(app.clone())) == Some(OnOff::Off) && !site_could_say_on
    }

    /// "in Figma" for the front app, by its name, "at figma.com" for a site, or "everywhere".
    fn here(&self, place: &Place) -> String {
        match place {
            Place::Everywhere => "everywhere".to_owned(),
            Place::App(bundle_id) => format!(
                "in {}",
                self.name_in_front(&RuleIdentity::App(bundle_id.clone()))
            ),
            Place::Domain(domain) => format!("at {domain}"),
        }
    }
}

/// The Night Shift state a decision asks for, if it asks for one.
fn wanted(decision: &Decision) -> Option<bool> {
    decision.state.map(|state| state == OnOff::On)
}

fn choice(title: &str, command: Command) -> RowItem {
    RowItem::Choice {
        item: MenuItem {
            title: title.to_owned(),
            subtitle: None,
            enabled: true,
            command,
        },
    }
}

/// "30 Minutes", "1 Hour", "1 Hour 30 Minutes".
fn length_title(duration: SignedDuration) -> String {
    let minutes = duration.as_mins();
    let unit =
        |count: i64, name: &str| format!("{count} {name}{}", if count == 1 { "" } else { "s" });
    match (minutes / 60, minutes % 60) {
        (0, minutes) => unit(minutes, "Minute"),
        (hours, 0) => unit(hours, "Hour"),
        (hours, minutes) => format!("{} {}", unit(hours, "Hour"), unit(minutes, "Minute")),
    }
}

fn whose(rule: &Rule) -> &'static str {
    match rule.source {
        RuleSource::Config => "your config’s rule",
        RuleSource::AdHoc => "your rule",
        RuleSource::Tour => "the tour’s rule",
    }
}

/// The identity a rule for this place would have; everywhere has none.
fn place_identity(place: &Place) -> Option<RuleIdentity> {
    match place {
        Place::Everywhere => None,
        Place::App(bundle_id) => Some(RuleIdentity::App(bundle_id.clone())),
        Place::Domain(domain) => Some(RuleIdentity::Domain(domain.clone())),
    }
}

/// `place` as layers keep it, checked as a rule identity is.
fn normalize_place(place: &Place) -> Result<Place, ErrorReply> {
    match place_identity(place) {
        None => Ok(Place::Everywhere),
        Some(identity) => normalize_identity(&identity).map(Place::from),
    }
}

/// A place as the log may show it: a domain is named by its kind only (ADR-0012).
fn loggable_place(place: &Place) -> String {
    match place_identity(place) {
        None => "everywhere".to_owned(),
        Some(identity) => loggable(&identity),
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
