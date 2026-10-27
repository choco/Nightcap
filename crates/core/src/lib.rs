#![warn(clippy::expect_used, clippy::indexing_slicing, clippy::unreachable)]

uniffi::setup_scaffolding!();

mod adhoc;
mod config;
mod domain;
mod file;
mod ipc;
mod logging;
mod places;
mod schedule;
mod state;

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::time::SystemTime;

use etcetera::BaseStrategy;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use nightcap_protocol::{
    BundleId, Check, CheckKind, CheckState, Command, Decision, Domain, ErrorCode, ErrorReply,
    ExportFormat, Install, Instance, Layer, Length, NightShift, NotificationPermission, OnOff,
    Payload, Permissions, Place, Query, Reason, Request, Response, Rule, RuleIdentity, RuleSource,
    Schedule, Status, TimeOfDay, WebsiteChecking,
};

use config::NightShiftConfig;
use domain::Host;
use places::{Context, Kept, Look, Places, Refusal, Stack, World, place_identity};
use schedule::{CheckedSchedule, SunTimes, Timeline};

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

// The shell sees bundle IDs and domains as strings. One it sends that isn't valid fails to lift,
// which stops the app, so the shell checks any it didn't get from the core.
uniffi::custom_type!(BundleId, String, {
    remote,
    lower: |bundle_id| bundle_id.into(),
    try_lift: |text| Ok(text.parse()?),
});

uniffi::custom_type!(Domain, String, {
    remote,
    lower: |domain| domain.into(),
    try_lift: |text| Ok(text.parse()?),
});

#[uniffi::remote(Enum)]
pub enum RuleIdentity {
    App(BundleId),
    Domain(Domain),
}

#[uniffi::remote(Enum)]
pub enum Place {
    Everywhere,
    App(BundleId),
    Domain(Domain),
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
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Menu {
    pub icon: MenuIcon,
    /// The menu's first item, after the problem line.
    pub night_shift: NightShiftItem,
    /// The problem line, while any problem exists.
    pub problem_line: Option<ProblemLine>,
    pub entries: Vec<MenuEntry>,
}

/// The menu's Night Shift item: everywhere's switch, with what decides the screen (ADR-0018).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NightShiftItem {
    /// Checked while Night Shift is on everywhere.
    pub on: bool,
    /// What decides the screen, in a few words, e.g. "Off in Figma: your config’s rule".
    pub reason: Option<String>,
    /// None while Night Shift is unavailable, which disables the item.
    pub command: Option<Command>,
}

/// What the settings window shows, pulled like the menu: the shell only draws it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
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
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsLayers {
    pub place: Place,
    /// "Night Shift off until 20:00".
    pub label: String,
    pub action: RuleAction,
}

/// What onboarding asks, pulled like the menu: the shell only draws it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OnboardingSteps {
    pub night_shift: OnboardingNightShift,
    /// Website checking as the config sets it, which answers its step for good.
    pub websites_from_config: Option<bool>,
    /// Whether Nightcap opens at login now, or None where it never opens itself.
    pub opens_at_login: Option<bool>,
}

/// Onboarding's Night Shift steps, which set the strength and the schedule (ADR-0014).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
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
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
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
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RuleAction {
    pub title: String,
    pub command: Command,
}

/// The domain a domain rule would have for what the user typed, or None when it isn't one.
#[uniffi::export]
pub fn normalize_domain(text: String) -> Option<String> {
    text.trim().parse::<Domain>().ok().map(String::from)
}

/// Whether an app rule can name this bundle ID: letters, digits, hyphens and periods. A
/// command naming any other stops the app as it reaches the core.
#[uniffi::export]
pub fn is_bundle_id(text: String) -> bool {
    text.parse::<BundleId>().is_ok()
}

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

/// Flipping it sends `command` back as `Event::CommandChosen`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
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

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MenuEntry {
    /// The strength's slider, 0 to 100, under the Night Shift item. Moving it sends
    /// `PreviewStrength`, and letting go sends `Strength`.
    Strength {
        value: u8,
    },
    /// A place row: a submenu that turns one place the other way (ADR-0018).
    Row {
        row: PlaceRow,
    },
    Separator,
}

/// A place row: everywhere's under the Night Shift item, then the front app's and site's.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
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

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
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
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
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
        /// Whether macOS has sunset to sunrise set aside, to put back once Location Services
        /// is on.
        sun_schedule_set_aside: bool,
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
    /// The tour started (ADR-0016).
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

#[derive(Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Effect {
    /// Registers Nightcap to open at login, as the user chose (ADR-0011).
    RegisterLoginItem,
    UnregisterLoginItem,
    /// Starts Sparkle's daily checks, in self-managed releases only (ADR-0011).
    StartUpdater,
    /// Starts Sparkle's own check, as if the user had asked for one.
    CheckForUpdates,
    /// Opens onboarding, skipping the steps of versions up to `completed`.
    ShowOnboarding {
        completed: u32,
    },
    /// Asks for Accessibility, which website checking needs: the macOS prompt, or the pane in
    /// System Settings once Nightcap has an entry there (ADR-0012).
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
        /// A page to open when the notification is clicked.
        opens: Option<String>,
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
/// performing it: CoreBrightness's clock and the core's can differ slightly (ADR-0004).
const TRANSITION_TOLERANCE: SignedDuration = SignedDuration::from_mins(2);

/// How long after waking macOS may take to perform a schedule transition it missed during sleep.
/// A guess until measured.
const POST_WAKE_GRACE: SignedDuration = SignedDuration::from_mins(5);

/// How long sunset to sunrise can go without sun times before it's a problem. CoreBrightness
/// took 83 seconds to work them out after a login, and two minutes after Location Services came
/// back on (macOS 27).
const SUN_TIMES_PATIENCE: SignedDuration = SignedDuration::from_mins(10);

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

impl fmt::Debug for Core {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Core")
            .field("instance", &self.instance)
            .field("install", &self.install)
            .field("socket", &self.socket)
            .finish_non_exhaustive()
    }
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
    facts: Facts,
    places: Places,
    strength: Option<u8>,
    config_error: Option<config::ConfigError>,
    /// Whether the config file existed when it was last loaded.
    config_found: bool,
    /// The menu's preset lengths, from the config or else 30 minutes, 1 hour and 2 hours.
    lengths: Vec<SignedDuration>,
    /// Why `rules.toml` can't be used; while set, ad-hoc rules are off and the file is left be.
    ad_hoc_error: Option<adhoc::AdHocError>,
    onboarding: u32,
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
    /// The write the shell was last asked for, until a status callback reports the result.
    pending_write: Option<bool>,
    unreported: Option<Unreported>,
    session: Session,
    /// Where the last run left Night Shift, which the first status callback will show.
    left_by_last_run: Option<OnOff>,
    /// The wakeup the shell was last asked for.
    wakeup: Option<Timestamp>,
    /// Since when sunset to sunrise has had no transition ahead, for want of sun times.
    sun_times_awaited_since: Option<Timestamp>,
    /// macOS has sunset to sunrise set aside, and the schedule is off.
    schedule_set_aside: bool,
    /// See `state::Persisted::schedule_off_accepted`.
    schedule_off_accepted: bool,
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
    /// A self-managed release, which opens itself at login if the user wants (ADR-0011).
    opens_itself_at_login: bool,
    /// See `state::Persisted::last_version`.
    last_version: Option<String>,
    /// What was last written to `state.json`.
    saved: state::Persisted,
}

/// What the place stack reads but doesn't own, lent to it as a `World` for each call.
struct Facts {
    night_shift: NightShift,
    timeline: Timeline,
    context: Context,
    /// Every schedule transition up to here has been applied.
    applied_until: Timestamp,
    /// Website checking as the user chose it; `websites` in the config wins (ADR-0012).
    websites: bool,
    config_websites: Option<bool>,
}

impl Facts {
    fn world(&self, now: Timestamp) -> World<'_> {
        World {
            context: &self.context,
            timeline: &self.timeline,
            applied_until: self.applied_until,
            now,
            observed: OnOff::from(self.night_shift == NightShift::On),
            website_checking: self.website_checking(),
        }
    }

    fn website_checking(&self) -> bool {
        self.config_websites.unwrap_or(self.websites)
    }
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
        let xdg = etcetera::base_strategy::Xdg::new().map_err(|error| CoreError::Io {
            message: format!("can't find the XDG base directories: {error}"),
        })?;
        let state_folder = xdg.state_dir().ok_or_else(|| CoreError::Io {
            message: "can't find the XDG state directory".to_owned(),
        })?;
        let zone = TimeZone::get(&time_zone).map_err(|error| {
            if jiff::tz::db().is_definitively_empty() {
                CoreError::Io {
                    message: format!("can't read the time zone database: {error}"),
                }
            } else {
                CoreError::UnknownTimeZone {
                    iana_id: time_zone.clone(),
                }
            }
        })?;
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

    /// The ad-hoc rules, written for the config (ADR-0007).
    #[expect(
        clippy::expect_used,
        reason = "TOML refuses only shapes the rule lists never have, and the shell's call can't fail"
    )]
    pub fn export(&self, format: ExportFormat) -> String {
        adhoc::export(&self.state().places.kept().ad_hoc, format).expect("rules always serialize")
    }

    pub fn onboarding(&self) -> OnboardingSteps {
        let state = self.state();
        OnboardingSteps {
            night_shift: state.onboarding_night_shift(),
            websites_from_config: state.facts.config_websites,
            opens_at_login: state
                .login_item
                .filter(|_| self.updates_itself())
                .map(|item| item != LoginItem::NotRegistered),
        }
    }

    pub fn settings(&self) -> Settings {
        let state = self.state();
        let now = self.clock.now();
        let look = state.places.look(&state.facts.world(now));
        let on = state.facts.website_checking();
        Settings {
            website_checking: SettingsSwitch {
                on,
                locked: state.facts.config_websites.is_some(),
                command: Command::Websites(OnOff::from(!on)),
            },
            // Registered but waiting for approval counts as on; its check explains the rest.
            config_path: self.config_file.display().to_string(),
            rules: look
                .rules
                .iter()
                .filter_map(|(rule, snooze)| state.settings_rule(rule, *snooze))
                .collect(),
            for_now: look
                .layers
                .iter()
                .filter(|layers| {
                    layers.place != Place::Everywhere
                        && !look
                            .rules
                            .iter()
                            .any(|(rule, _)| Place::from(rule.identity.clone()) == layers.place)
                })
                .map(|layers| SettingsLayers {
                    place: layers.place.clone(),
                    label: format!(
                        "Night Shift {}",
                        state.layer_text(layers.timer.as_ref(), layers.hold.as_ref())
                    ),
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
                        command: Command::OpenAtLogin(OnOff::from(!on)),
                    }
                }),
            checks: state
                .checks(now)
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
        if state.facts.night_shift == NightShift::Unavailable {
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
        let look = state.places.look(&state.facts.world(now));
        let everywhere = look.everywhere.gives;
        let mut entries = Vec::new();
        if let Some(value) = state.strength {
            entries.push(MenuEntry::Strength { value });
        }
        if let Some(row) = state.everywhere_row(&look, now) {
            entries.push(MenuEntry::Row { row });
        }
        let front: Vec<&Stack> = look.app.iter().chain(&look.site).collect();
        if !front.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        for stack in front {
            entries.push(MenuEntry::Row {
                row: state.place_row(stack, now),
            });
        }
        Menu {
            icon: match look.screen {
                OnOff::Off
                    if look.decision.place != Place::Everywhere && everywhere == OnOff::On =>
                {
                    MenuIcon::ForcedOff
                }
                OnOff::On => MenuIcon::On,
                OnOff::Off => MenuIcon::Off,
            },
            night_shift: NightShiftItem {
                on: everywhere == OnOff::On,
                reason: state.menu_reason(&look.decision),
                command: Some(match everywhere {
                    OnOff::On => Command::Off,
                    OnOff::Off => Command::On,
                }),
            },
            problem_line: state.problem_line(now),
            entries,
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
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
                // Updates install silently, so the first launch of a new version says what's new.
                // A first run has nothing to compare; one from before versions were kept has
                // finished onboarding.
                let version = env!("CARGO_PKG_VERSION");
                let updated = match &state.last_version {
                    Some(last) => last != version,
                    None => state.onboarding > 0,
                };
                if updated {
                    tracing::info!("updated to {version}");
                    effects.push(Effect::Notify {
                        title: format!("Nightcap updated to {version}"),
                        body: "Click to see what’s new.".to_owned(),
                        opens: Some(format!("https://nightcap.choco.me/changelog/#v{version}")),
                    });
                }
                state.last_version = Some(version.to_owned());
                if state.onboarding < ONBOARDING {
                    // The config can answer website checking and Night Shift; opening at login
                    // it can't.
                    let night_shift = state.onboarding_night_shift();
                    let night_shift_answered =
                        !night_shift.ask_strength && !night_shift.ask_schedule;
                    if state.facts.config_websites.is_some()
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
                // An app whose bundle ID no rule could name counts as having none.
                let bundle_id = bundle_id.and_then(|bundle_id| {
                    bundle_id
                        .parse()
                        .inspect_err(|error| tracing::debug!("{error}; it counts as having none"))
                        .ok()
                });
                let mut state = self.state();
                state.facts.context.app = bundle_id;
                state.app_name = name;
                state.is_browser = is_browser;
                Vec::new()
            }
            // The host itself is never logged (ADR-0009).
            Event::HostChanged { generation, host } => {
                let mut state = self.state();
                match &mut state.watch {
                    Some(watch) if watch.generation == generation => {
                        watch.reported = true;
                        state.facts.context.host = host.as_deref().and_then(Host::new);
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
            Event::StatusChanged {
                enabled,
                schedule,
                sun_schedule_set_aside,
            } => {
                let mut guard = self.state();
                let state = &mut *guard;
                if state.session == Session::Away {
                    tracing::debug!("ignored a status callback while dormant");
                    return Vec::new();
                }
                state.facts.night_shift = if enabled {
                    NightShift::On
                } else {
                    NightShift::Off
                };
                state.pending_write = None;
                let keep_on = std::mem::take(&mut state.keep_on_with_schedule_off);
                // A shell bug, logged rather than a panic, which would stop the app (ADR-0002).
                let checked = CheckedSchedule::try_from(schedule)
                    .inspect_err(|error| {
                        tracing::error!("the shell reported a schedule it can't have: {error}");
                    })
                    .ok();
                let previous = std::mem::replace(&mut state.facts.timeline.schedule, checked);
                state.schedule_set_aside = sun_schedule_set_aside && schedule == Schedule::Off;
                if !state.schedule_set_aside {
                    state.schedule_off_accepted = false;
                }
                if state.session == Session::Back {
                    // The shell's fresh read after returning: nobody could change Night Shift by
                    // hand while the session was away.
                    state.session = Session::Active;
                    Vec::new()
                } else if previous
                    .zip(checked)
                    .is_some_and(|(previous, checked)| previous != checked)
                {
                    // macOS applies an edited schedule at once, so the edit is a schedule
                    // transition and whatever it switched isn't an external change.
                    tracing::info!("schedule edited");
                    state.places.schedule_edited();
                    if keep_on && schedule == Schedule::Off && !enabled {
                        let now = self.clock.now();
                        state.places.switch(OnOff::On, &state.facts.world(now));
                    }
                    Vec::new()
                } else {
                    self.reconcile(state, enabled).into_iter().collect()
                }
            }
            Event::StrengthChanged { value } => {
                self.state().strength = Some(value);
                Vec::new()
            }
            Event::OnboardingFinished { websites } => {
                let mut state = self.state();
                state.onboarding = ONBOARDING;
                if state.facts.config_websites.is_none() {
                    tracing::info!("onboarding finished; website checking {websites}");
                    state.facts.websites = websites;
                }
                Vec::new()
            }
            Event::TourStarted => {
                let mut state = self.state();
                let stands_in = state.facts.night_shift == NightShift::Off;
                if !state.places.start_tour(stands_in) {
                    tracing::warn!("a tour started while one runs; keeping the first");
                    return Vec::new();
                }
                tracing::info!(
                    "tour started{}",
                    if stands_in {
                        ", standing in for the schedule with Night Shift on"
                    } else {
                        ""
                    }
                );
                Vec::new()
            }
            Event::TourEnded => {
                let now = self.clock.now();
                self.state().places.end_tour(now);
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
                    Fix::KeepScheduleOff => self.state().schedule_off_accepted = true,
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
                    // The message can name a domain, so only the code is logged (ADR-0009).
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
                let timestamps = |times: Vec<SystemTime>| {
                    times
                        .into_iter()
                        .map(Timestamp::try_from)
                        .collect::<Result<Vec<_>, _>>()
                };
                let sun_times = match (timestamps(sunrises), timestamps(sunsets)) {
                    (Ok(sunrises), Ok(sunsets)) => SunTimes::new(&sunrises, &sunsets),
                    // A shell bug, logged rather than a panic, which would stop the app
                    // (ADR-0002).
                    (Err(error), _) | (_, Err(error)) => {
                        tracing::error!("the shell reported a sun time out of range: {error}");
                        SunTimes::default()
                    }
                };
                self.state().facts.timeline.sun_times = sun_times;
                Vec::new()
            }
            Event::TimeZoneChanged { iana_id } => {
                match TimeZone::get(&iana_id) {
                    Ok(zone) => {
                        tracing::info!("time zone changed to {iana_id}");
                        self.state().facts.timeline.zone = zone;
                    }
                    Err(error) => {
                        tracing::error!("keeping the old time zone: {iana_id} is unknown: {error}");
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
                // behind (ADR-0008), nor Night Shift on for a tour (ADR-0016).
                let now = self.clock.now();
                self.state().places.release(now);
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
                tracing::warn!(
                    "running without config rules: the config {}; see status",
                    error.kind()
                );
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
                tracing::warn!(
                    "ad-hoc rules are off: rules.toml {}; see status",
                    error.kind()
                );
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
                facts: Facts {
                    night_shift: NightShift::Unavailable,
                    timeline: Timeline {
                        schedule: None,
                        sun_times: SunTimes::default(),
                        zone: setup.zone,
                    },
                    context: Context::default(),
                    applied_until: setup.clock.now(),
                    websites: persisted.websites,
                    config_websites: config.websites,
                },
                places: Places::new(Kept {
                    layers: persisted.layers.clone(),
                    ad_hoc,
                }),
                strength: None,
                config_error,
                config_found: config.found,
                lengths: config.lengths,
                ad_hoc_error,
                onboarding: persisted.onboarding,
                config_night_shift: config.night_shift,
                applied_from_config: persisted.applied_from_config,
                keep_on_with_schedule_off: false,
                app_name: None,
                is_browser: false,
                watch: None,
                generation: 0,
                pending_write: None,
                unreported: None,
                session: Session::Active,
                left_by_last_run: persisted.night_shift,
                wakeup: None,
                sun_times_awaited_since: None,
                schedule_set_aside: false,
                schedule_off_accepted: persisted.schedule_off_accepted,
                accessibility: None,
                notifications: NotificationPermission::NotRequested,
                notifications_wanted: persisted.notifications_wanted,
                announced: persisted.announced.clone(),
                login_item: None,
                open_at_login: persisted.open_at_login,
                opens_itself_at_login: setup.install == Install::SelfManaged
                    && setup.instance == Instance::Release,
                last_version: persisted.last_version.clone(),
                saved: persisted,
            }),
        };
        core.graduate(&mut core.state(), config.rules);
        Ok(core)
    }

    /// Only a self-managed release opens at login and updates through Sparkle: an installer
    /// owns a managed one, and a dev build would shadow the installed app (ADR-0011).
    fn updates_itself(&self) -> bool {
        self.install == Install::SelfManaged && self.instance == Instance::Release
    }

    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
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
                let state = self.state();
                let look = state.places.look(&state.facts.world(self.clock.now()));
                Response::Ok(Payload::Rules(
                    look.rules
                        .into_iter()
                        .map(|(rule, _)| rule.clone())
                        .collect(),
                ))
            }
            Request::Query(Query::Export(format)) => {
                match adhoc::export(&self.state().places.kept().ad_hoc, *format) {
                    Ok(text) => Response::Ok(Payload::Export(text)),
                    Err(error) => Response::Error(refused(
                        ErrorCode::Internal,
                        format!("couldn't write the rules as TOML: {error}"),
                    )),
                }
            }
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
                Err(error) => Response::Error(error),
            },
        }
    }

    /// Loads the config again; an invalid one keeps the rules loaded before, and its check says
    /// why.
    fn reload(&self, effects: &mut Vec<Effect>) -> Result<(), ErrorReply> {
        match config::load(&self.config_file) {
            Ok(config) => {
                tracing::info!("reloaded the config: {} rules", config.rules.len());
                let mut state = self.state();
                state.facts.config_websites = config.websites;
                state.config_night_shift = config.night_shift;
                state.lengths = config.lengths;
                state.apply_config_night_shift(effects);
                state.config_error = None;
                state.config_found = config.found;
                self.graduate(&mut state, config.rules);
                Ok(())
            }
            Err(error) => {
                tracing::warn!(
                    "keeping the rules loaded before: the config {}",
                    error.kind()
                );
                let refusal = refused(ErrorCode::InvalidConfig, error.to_string());
                self.state().config_error = Some(error);
                Err(refusal)
            }
        }
    }

    /// Runs a command from the CLI or the menu, answering with the resulting decision.
    fn run_command(
        &self,
        command: &Command,
        effects: &mut Vec<Effect>,
    ) -> Result<Decision, ErrorReply> {
        let mut guard = self.state();
        let state = &mut *guard;
        let night_shift = state.facts.night_shift;
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
        let now = self.clock.now();
        let outcome = match command {
            Command::On => {
                state.places.switch(OnOff::On, &state.facts.world(now));
                Ok(())
            }
            Command::Off => {
                state.places.switch(OnOff::Off, &state.facts.world(now));
                Ok(())
            }
            Command::Toggle => {
                let world = state.facts.world(now);
                let to = state.places.look(&world).everywhere.gives.other();
                state.places.switch(to, &world);
                Ok(())
            }
            Command::Set {
                place,
                state: on,
                length,
            } => state
                .places
                .set(place, *on, *length, &state.facts.world(now))
                .map_err(ErrorReply::from),
            Command::Reset(place) => {
                state.places.reset(place);
                Ok(())
            }
            Command::AddRule { identity, says } => self.add_rule(state, identity, *says),
            Command::RemoveRule(identity) => self.remove_rule(state, identity),
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
            Command::SetSchedule(schedule) => match CheckedSchedule::try_from(*schedule) {
                Err(error) => Err(refused(ErrorCode::BadRequest, error.to_string())),
                Ok(_) => {
                    tracing::info!("schedule set on command");
                    state.keep_on_with_schedule_off =
                        *schedule == Schedule::Off && night_shift == NightShift::On;
                    effects.push(Effect::SetSchedule {
                        schedule: *schedule,
                    });
                    Ok(())
                }
            },
            Command::Websites(chosen) => {
                let on = *chosen == OnOff::On;
                let turned_on = on && !state.facts.website_checking();
                state.set_websites(on).map(|()| {
                    if turned_on && state.accessibility != Some(true) {
                        effects.push(Effect::AskForAccessibility);
                    }
                })
            }
        };
        outcome.map(|()| state.places.look(&state.facts.world(now)).decision)
    }

    /// Reconciles a status callback with what the core wants (ADR-0004). macOS catching up on a
    /// transition the core already applied, or running just ahead of the next one, is that
    /// transition; a match needs nothing; anything else is an external change, handled like the
    /// equivalent command (ADR-0018).
    fn reconcile(&self, state: &mut State, enabled: bool) -> Option<Effect> {
        if state.left_by_last_run.take() == Some(OnOff::from(enabled)) {
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
        let wanted = wanted(&state.places.look(&state.facts.world(now)).decision)?;
        if enabled == wanted {
            return None;
        }
        if let Some(transition) = state
            .facts
            .timeline
            .next_transition(state.facts.applied_until)
            && transition.duration_since(now) <= TRANSITION_TOLERANCE
            && state.facts.timeline.scheduled_state(transition) == Some(enabled)
        {
            tracing::info!(
                "schedule transition at {transition}, reported {:#} early",
                transition.duration_since(now)
            );
            state.places.expire(transition);
            state.facts.applied_until = transition;
            return None;
        }
        let observed = OnOff::from(enabled);
        tracing::info!("Night Shift turned {observed} outside Nightcap");
        let snooze = state.places.adopt(observed, &state.facts.world(now))?;
        Some(Effect::Notify {
            title: format!("Night Shift turned {} outside Nightcap", snooze.state),
            body: format!(
                "Snoozed the {} rule {}.",
                state.name_in_front(&snooze.rule),
                state.until_text(snooze.until, snooze.state)
            ),
            opens: None,
        })
    }

    /// Applies every schedule transition that came due since the last event, and ends the
    /// timers and holds that ran out.
    fn catch_up(&self) {
        let now = self.clock.now();
        let mut state = self.state();
        if now <= state.facts.applied_until {
            return;
        }
        state.places.expire(now);
        if let Some(transition) = state
            .facts
            .timeline
            .next_transition(state.facts.applied_until)
            && transition <= now
        {
            tracing::info!("schedule transition at {transition}");
            state.unreported =
                state
                    .facts
                    .timeline
                    .scheduled_state(now)
                    .map(|scheduled| Unreported {
                        transition,
                        scheduled,
                        until: now + TRANSITION_TOLERANCE,
                    });
        }
        state.facts.applied_until = now;
    }

    /// Posts one notification when a problem appears, unless `aloud` is false because the user
    /// caused it themselves. A problem that ends and comes back is announced again.
    fn announce(&self, aloud: bool, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        let problems: Vec<_> = state
            .checks(self.clock.now())
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
                effects.push(Effect::Notify {
                    title,
                    body,
                    opens: None,
                });
            }
        }
    }

    /// Watches the frontmost browser while website checking is on, unless its own place says
    /// off and no site could say on (ADR-0012, ADR-0018).
    fn rewatch(&self, effects: &mut Vec<Effect>) {
        let mut guard = self.state();
        let state = &mut *guard;
        let holds_off = state
            .places
            .look(&state.facts.world(self.clock.now()))
            .browser_holds_off;
        let wanted = state
            .facts
            .context
            .app
            .clone()
            .filter(|_| state.is_browser && state.facts.website_checking() && !holds_off);
        match (&state.watch, wanted) {
            (Some(watch), Some(bundle_id)) if watch.bundle_id == bundle_id => {}
            (_, Some(bundle_id)) => {
                state.generation += 1;
                let generation = state.generation;
                state.facts.context.host = None;
                state.watch = Some(Watch {
                    bundle_id: bundle_id.clone(),
                    generation,
                    reported: false,
                });
                effects.push(Effect::WatchBrowser {
                    bundle_id: bundle_id.into(),
                    generation,
                });
            }
            (Some(_), None) => {
                state.facts.context.host = None;
                state.watch = None;
                effects.push(Effect::StopWatching);
            }
            (None, None) => {}
        }
    }

    /// Takes the config's rules, dropping the ad-hoc rules it now has, which have graduated
    /// (ADR-0007), from `rules.toml` too. The config wins over them whether or not the file can
    /// be written, and the next load drops them again.
    fn graduate(&self, state: &mut State, rules: Vec<Rule>) {
        let before = state.places.kept().ad_hoc.len();
        state.places.set_config(rules);
        let graduated = before - state.places.kept().ad_hoc.len();
        if graduated > 0 {
            tracing::info!("{graduated} ad-hoc rules graduated into the config");
            if let Err(error) = self.ad_hoc_store.save(&state.places.kept().ad_hoc) {
                tracing::warn!("couldn't save rules.toml: {error}");
            }
        }
    }

    fn add_rule(
        &self,
        state: &mut State,
        identity: &RuleIdentity,
        says: OnOff,
    ) -> Result<(), ErrorReply> {
        let before = state.places.clone();
        state.places.add_rule(identity, says)?;
        self.keep_ad_hoc(state, before)
    }

    fn remove_rule(&self, state: &mut State, identity: &RuleIdentity) -> Result<(), ErrorReply> {
        let before = state.places.clone();
        state.places.remove_rule(identity)?;
        self.keep_ad_hoc(state, before)
    }

    /// Writes `rules.toml` when a change reached the ad-hoc rules it keeps, and puts the places
    /// back as they were `before` when it can't.
    fn keep_ad_hoc(&self, state: &mut State, before: Places) -> Result<(), ErrorReply> {
        if state.places.kept().ad_hoc == before.kept().ad_hoc {
            return Ok(());
        }
        let saved = state.check_ad_hoc_rules_on().and_then(|()| {
            self.ad_hoc_store
                .save(&state.places.kept().ad_hoc)
                .map_err(|error| {
                    refused(
                        ErrorCode::AdHocRulesOff,
                        format!("couldn't save rules.toml: {error}"),
                    )
                })
        });
        if saved.is_err() {
            state.places = before;
        }
        saved
    }

    /// Writes `state.json` whenever what it holds changes, so a crash loses nothing (ADR-0008).
    fn save(&self) {
        let mut state = self.state();
        let snapshot = state.snapshot(self.clock.now());
        if snapshot == state.saved {
            return;
        }
        match self.store.save(&snapshot) {
            Ok(()) => state.saved = snapshot,
            Err(error) => tracing::warn!("couldn't save the state: {error}"),
        }
    }

    /// Keeps the shell's one wakeup at the next schedule transition or the end of a timer or
    /// hold, whichever comes first. Sunset to sunrise with no transition ahead lacks the sun
    /// times, which CoreBrightness can take a minute or two after login to work out, so the
    /// wakeup comes at the next whole minute instead: the shell reads them again before each.
    fn reschedule(&self, effects: &mut Vec<Effect>) {
        let now = self.clock.now();
        let mut state = self.state();
        let transition = state
            .facts
            .timeline
            .next_transition(state.facts.applied_until);
        let awaits_sun_times = transition.is_none()
            && state.facts.timeline.schedule == Some(CheckedSchedule::SunsetToSunrise);
        state.sun_times_awaited_since =
            awaits_sun_times.then(|| state.sun_times_awaited_since.unwrap_or(now));
        #[expect(
            clippy::expect_used,
            reason = "rounding fails only beside jiff's limits, millennia from any clock"
        )]
        let retry = awaits_sun_times.then(|| {
            now.round(
                jiff::TimestampRound::new()
                    .smallest(jiff::Unit::Minute)
                    .mode(jiff::RoundMode::Trunc),
            )
            .expect("a minute rounds any time")
                + SignedDuration::from_mins(1)
        });
        let look = state.places.look(&state.facts.world(now));
        let ends = look
            .layers
            .iter()
            .flat_map(|layers| [&layers.hold, &layers.timer])
            .flatten()
            .filter_map(|layer| layer.until);
        let next = transition.or(retry).into_iter().chain(ends).min();
        if next != state.wakeup {
            state.wakeup = next;
            effects.push(match next {
                Some(at) => Effect::ScheduleWakeup { at: at.into() },
                None => Effect::CancelWakeup,
            });
        }
    }

    /// Writes Night Shift when what the core wants differs from what was last observed
    /// (ADR-0004), once until a status callback reports the result.
    fn enforce(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        if state.facts.night_shift == NightShift::Unavailable || state.session != Session::Active {
            return;
        }
        // A new watch keeps Night Shift as it is until its first read (ADR-0012).
        if state.watch.as_ref().is_some_and(|watch| !watch.reported) {
            return;
        }
        let look = state.places.look(&state.facts.world(self.clock.now()));
        let Some(wanted) = wanted(&look.decision) else {
            return;
        };
        if wanted != (state.facts.night_shift == NightShift::On)
            && state.pending_write != Some(wanted)
        {
            state.pending_write = Some(wanted);
            effects.push(Effect::SetNightShift { enabled: wanted });
        }
    }

    fn status(&self) -> Response {
        let state = self.state();
        let facts = &state.facts;
        let look = state.places.look(&facts.world(self.clock.now()));
        Response::Ok(Payload::Status(Box::new(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: self.install,
            night_shift: facts.night_shift,
            strength: state.strength,
            config_strength: state.config_night_shift.strength,
            decision: look.decision,
            schedule: facts.timeline.schedule.map(Schedule::from),
            config_schedule: state.config_night_shift.schedule,
            scheduled_state: facts
                .timeline
                .scheduled_state(facts.applied_until)
                .map(OnOff::from),
            next_transition: facts.timeline.next_transition(facts.applied_until),
            frontmost_app: facts.context.app.clone().map(String::from),
            watching: state.watch.is_some(),
            host: facts
                .context
                .host
                .as_ref()
                .map(|host| host.as_str().to_owned()),
            rules: look
                .rules
                .into_iter()
                .map(|(rule, _)| rule.clone())
                .collect(),
            config_error: state.config_error.as_ref().map(ToString::to_string),
            ad_hoc_error: state.ad_hoc_error.as_ref().map(ToString::to_string),
            website_checking: WebsiteChecking {
                state: OnOff::from(facts.website_checking()),
                from_config: facts.config_websites.is_some(),
            },
            layers: look.layers.to_vec(),
            permissions: Permissions {
                accessibility: state.accessibility == Some(true),
                notifications: state.notifications,
            },
            checks: state.checks(self.clock.now()),
        })))
    }
}

impl From<Refusal> for ErrorReply {
    fn from(refusal: Refusal) -> Self {
        let code = match refusal {
            Refusal::TooShort
            | Refusal::TooLong(_)
            | Refusal::Passed
            | Refusal::NoSchedule
            | Refusal::ForGoodIsARule => ErrorCode::BadRequest,
            Refusal::ConfigSays { .. } | Refusal::ConfigRule(_) => ErrorCode::ConfigRule,
            Refusal::NoSuchRule(_) => ErrorCode::NoSuchRule,
        };
        refused(code, refusal.to_string())
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
    bundle_id: BundleId,
    generation: u64,
    reported: bool,
}

impl State {
    /// Mid-tour, the state the tour's end would leave, with Night Shift as the tour holds it: a
    /// run after a crash finds it there, as the last run left it, and puts it back (ADR-0016).
    fn snapshot(&self, now: Timestamp) -> state::Persisted {
        state::Persisted {
            layers: self.places.kept().layers.clone(),
            websites: self.facts.websites,
            onboarding: self.onboarding,
            notifications_wanted: self.notifications_wanted,
            open_at_login: self.open_at_login,
            announced: self.announced.clone(),
            night_shift: wanted(&self.places.look(&self.facts.world(now)).decision)
                .map(OnOff::from),
            until: self
                .facts
                .timeline
                .next_transition(self.facts.applied_until),
            applied_from_config: self.applied_from_config,
            schedule_off_accepted: self.schedule_off_accepted,
            last_version: self.last_version.clone(),
        }
    }

    fn onboarding_night_shift(&self) -> OnboardingNightShift {
        let available = self.facts.night_shift != NightShift::Unavailable;
        let ask_strength = self.strength.is_some() && self.config_night_shift.strength.is_none();
        let ask_schedule = available && self.config_night_shift.schedule.is_none();
        OnboardingNightShift {
            ask_strength,
            ask_schedule,
            already_set_up: (ask_strength || ask_schedule)
                && self
                    .facts
                    .timeline
                    .schedule
                    .is_some_and(|schedule| schedule != CheckedSchedule::Off),
            strength: self.strength,
            schedule: self.facts.timeline.schedule.map(Schedule::from),
        }
    }

    /// Writes the config's strength and schedule where they changed since last applied, once
    /// Night Shift, and for the strength its value, is known (ADR-0014).
    fn apply_config_night_shift(&mut self, effects: &mut Vec<Effect>) {
        if self.facts.night_shift == NightShift::Unavailable {
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

    /// The row under the Night Shift item (ADR-0018): everywhere, whatever is in front, with
    /// everywhere's lengths but the switch's own, and none that would come to the same as
    /// clicking the switch. None when it would offer nothing.
    fn everywhere_row(&self, look: &Look<'_>, now: Timestamp) -> Option<PlaceRow> {
        let everywhere = &look.everywhere;
        let place = &everywhere.place;
        let state = everywhere.gives.other();
        let mut items = Vec::new();
        // While the tour stands in, what's under everywhere's layers is the tour's on, which the
        // switch gives back; the schedule isn't there to go back to.
        let schedule_says_so = matches!(
            everywhere.base,
            Some(Reason::Schedule { state: scheduled, until: Some(_) }) if scheduled == state
        );
        if schedule_says_so
            && (everywhere.timer.is_some() || everywhere.hold.is_some())
            && look.after_switch.is_some()
        {
            items.push(choice(
                "Back to the Schedule",
                Command::Reset(place.clone()),
            ));
        }
        let mut lengths = Vec::new();
        if everywhere.after_timer != Some(state) {
            lengths.extend(self.timers(place, state, now, "Turn {} Everywhere"));
        }
        if everywhere.after_reset != Some(state)
            && let Some(until) = self
                .facts
                .timeline
                .next_turn(self.facts.applied_until, state)
        {
            lengths.push(self.until_turn(place, state, until));
        }
        if !items.is_empty() && !lengths.is_empty() {
            items.push(RowItem::Separator);
        }
        items.extend(lengths);
        if items.is_empty() {
            return None;
        }
        let decision = &look.decision;
        Some(PlaceRow {
            title: format!("Turn {}", capitalized(&state.to_string())),
            site: None,
            // The app or site in front keeps the other way, which the row can't change there.
            subtitle: (decision.place != Place::Everywhere && decision.state != Some(state))
                .then(|| format!("Everywhere but {}", self.here(&decision.place))),
            image: RowImage::Everywhere,
            indented: false,
            items,
        })
    }

    /// The row for an app or site in front (ADR-0018): that place alone, with undoing first:
    /// putting its rule back where that goes the right way, and removing a rule of the user's;
    /// then its lengths, then for good, which is a rule.
    fn place_row(&self, stack: &Stack, now: Timestamp) -> PlaceRow {
        let place = &stack.place;
        let state = stack.gives.other();
        let rule = rule_of(stack);
        let mut items = Vec::new();
        let below = stack.after_reset;
        if (stack.timer.is_some() || stack.hold.is_some()) && below == Some(state) {
            // A site follows its browser where the browser has its own say.
            let title = match (rule, &stack.inherits) {
                (Some(_), _) => "Put the Rule Back".to_owned(),
                (None, Some((Place::App(bundle_id), _))) => format!(
                    "Like the Rest of {}",
                    self.name_in_front(&RuleIdentity::App(bundle_id.clone()))
                ),
                (None, _) => "Like Everywhere Else".to_owned(),
            };
            items.push(choice(&title, Command::Reset(place.clone())));
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
        if stack.after_timer != Some(state) {
            let title = format!("Turn {{}} {}", self.here(place));
            lengths.extend(self.timers(place, state, now, &title));
        }
        if below != Some(state)
            && let Some(until) = self
                .facts
                .timeline
                .next_turn(self.facts.applied_until, state)
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
                format!("Turn {} {here}", capitalized(&state.to_string()))
            } else {
                capitalized(&here)
            },
            site: match place {
                Place::Domain(domain) => Some(domain.to_string()),
                _ => None,
            },
            subtitle: self.stack_text(stack),
            image: match place {
                Place::App(bundle_id) => RowImage::App {
                    bundle_id: bundle_id.to_string(),
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
        #[expect(
            clippy::expect_used,
            reason = "rounding fails only beside jiff's limits, millennia from any clock"
        )]
        let start = (now + SignedDuration::from_hours(1))
            .round(
                jiff::TimestampRound::new()
                    .smallest(jiff::Unit::Minute)
                    .increment(15)
                    .mode(jiff::RoundMode::Ceil),
            )
            .expect("a quarter hour rounds any time");
        items.push(RowItem::PickTime {
            title: title.replace("{}", &capitalized(&state.to_string())),
            place: place.clone(),
            state,
            start,
        });
        items
    }

    fn until_turn(&self, place: &Place, state: OnOff, until: Timestamp) -> RowItem {
        // The time alone could be any clock's; the line under it says it's the schedule's.
        let moment = match (&self.facts.timeline.schedule, state) {
            (Some(CheckedSchedule::SunsetToSunrise), OnOff::On) => "At sunrise",
            (Some(CheckedSchedule::SunsetToSunrise), OnOff::Off) => "At sunset",
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
    fn layer_text(&self, timer: Option<&Layer>, hold: Option<&Layer>) -> String {
        [timer, hold]
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
    fn stack_text(&self, stack: &Stack) -> Option<String> {
        let mut parts: Vec<String> = (stack.timer.is_some() || stack.hold.is_some())
            .then(|| self.layer_text(stack.timer.as_ref(), stack.hold.as_ref()))
            .into_iter()
            .collect();
        match rule_of(stack) {
            Some(rule) if parts.is_empty() => {
                parts.push(format!("always {}: {}", rule.says, whose(rule)));
            }
            Some(rule) => parts.push(whose(rule).to_owned()),
            None => {}
        }
        (!parts.is_empty()).then(|| capitalized(&parts.join(", then ")))
    }

    /// The reason under the menu's Night Shift item: what decides the screen.
    fn menu_reason(&self, decision: &Decision) -> Option<String> {
        let Some(top) = decision.reasons.first() else {
            // Without the sun times, only macOS knows where the schedule stands, and applies it.
            return (self.facts.timeline.schedule == Some(CheckedSchedule::SunsetToSunrise))
                .then(|| "Following the schedule".to_owned());
        };
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
            Reason::Schedule { until, .. } => match (until, self.facts.timeline.schedule) {
                (Some(until), _) => {
                    format!("following the schedule until {}", self.clock_time(*until))
                }
                (None, Some(CheckedSchedule::Off)) => "no schedule".to_owned(),
                (None, _) => "following the schedule".to_owned(),
            },
        };
        Some(capitalized(&text))
    }

    /// A rule's row in the Rules tab: a snooze can be put back, an ad-hoc rule removed, and a
    /// config rule changed only in the config (ADR-0006). The tour's rules have none
    /// (ADR-0016).
    fn settings_rule(&self, rule: &Rule, snooze: Option<&Layer>) -> Option<SettingsRule> {
        let identity = rule.identity.clone();
        let place = Place::from(identity.clone());
        let action = |title: &str, command| {
            Some(RuleAction {
                title: title.to_owned(),
                command,
            })
        };
        let (label, action) = match (snooze, rule.source) {
            (_, RuleSource::Tour) => return None,
            (Some(layer), _) => (
                format!("Snoozed {}", self.until_text(layer.until, layer.state)),
                action("Put Back", Command::Reset(place)),
            ),
            (None, RuleSource::Config) => ("From your config".to_owned(), None),
            (None, RuleSource::AdHoc) => (
                "Ad-hoc".to_owned(),
                action("Remove", Command::RemoveRule(identity.clone())),
            ),
        };
        Some(SettingsRule {
            identity,
            says: rule.says,
            label,
            snoozed: snooze.is_some(),
            action,
        })
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
        at.to_zoned(self.facts.timeline.zone.clone())
            .strftime("%H:%M")
            .to_string()
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

    /// The menu's problem line, from the checks.
    fn problem_line(&self, now: Timestamp) -> Option<ProblemLine> {
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
    fn checks(&self, now: Timestamp) -> Vec<Check> {
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
        } else if self.schedule_set_aside && !self.schedule_off_accepted {
            check(
                CheckKind::NightShift,
                CheckState::Drifted,
                "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on.",
            )
        } else if self
            .sun_times_awaited_since
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
        if let Some(granted) = self.accessibility {
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
                &match rules_from(RuleSource::Config) {
                    0 => "Valid, with no rules.".to_owned(),
                    1 => "Valid, with 1 rule.".to_owned(),
                    count => format!("Valid, with {count} rules."),
                },
            ),
            (None, false) => check(CheckKind::ConfigFile, CheckState::Off, "No config file."),
        });
        checks.push(match (&self.ad_hoc_error, rules_from(RuleSource::AdHoc)) {
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
                .filter(|_| self.facts.context.app.as_ref() == Some(bundle_id))
                .unwrap_or_else(|| bundle_id.to_string()),
            RuleIdentity::Domain(domain) => domain.to_string(),
        }
    }

    /// The notification for a problem that appeared on its own; Night Shift being unavailable is
    /// a fact about the Mac, and switched-off notifications can't announce themselves.
    fn notice(&self, kind: CheckKind) -> Option<(String, String)> {
        match kind {
            CheckKind::NightShift if self.facts.night_shift == NightShift::Unavailable => None,
            CheckKind::NightShift if self.schedule_set_aside => Some((
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
                .config_error
                .as_ref()
                .map(|error| ("Nightcap’s config is invalid".to_owned(), error.to_string())),
            CheckKind::AdHocRules => self
                .ad_hoc_error
                .as_ref()
                .map(|error| ("Ad-hoc rules are off".to_owned(), error.to_string())),
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
            CheckKind::Notifications => None,
        }
    }

    fn set_websites(&mut self, on: bool) -> Result<(), ErrorReply> {
        if self.facts.config_websites.is_some() {
            return Err(refused(
                ErrorCode::SetInConfig,
                "website checking is set by `websites` in the config",
            ));
        }
        tracing::info!("website checking {}", OnOff::from(on));
        self.facts.websites = on;
        Ok(())
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

/// The rule at a place's base, which for everywhere is none.
fn rule_of(stack: &Stack) -> Option<&Rule> {
    match &stack.base {
        Some(Reason::Rule(rule)) => Some(rule),
        _ => None,
    }
}

fn whose(rule: &Rule) -> &'static str {
    match rule.source {
        RuleSource::Config => "your config’s rule",
        RuleSource::AdHoc => "your rule",
        RuleSource::Tour => "the tour’s rule",
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

fn refused(code: ErrorCode, message: impl Into<String>) -> ErrorReply {
    ErrorReply {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests;
