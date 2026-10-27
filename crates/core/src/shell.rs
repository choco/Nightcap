//! What crosses between the shell and the core (ADR-0002): the facts the shell reports, the
//! commands only its UI sends, the outcome of each, with the effects it carries out and the
//! events it tells, and the protocol's types as the shell sees them.

use std::time::SystemTime;

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, BundleId, Check, CheckDetail, CheckKind, CheckState,
    Command, ConfigFileDetail, Domain, ErrorCode, ErrorReply, ExportFormat, FileError, Length,
    NightShiftDetail, NotificationPermission, NotificationsDetail, OnOff, OpenAtLoginDetail, Place,
    RuleIdentity, Schedule, TimeOfDay,
};

use crate::{Fix, TourApp, TourSite};

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

#[uniffi::remote(Record)]
pub struct Check {
    pub detail: CheckDetail,
    pub state: CheckState,
}

#[uniffi::remote(Enum)]
pub enum CheckDetail {
    NightShift(NightShiftDetail),
    Accessibility(AccessibilityDetail),
    Notifications(NotificationsDetail),
    OpenAtLogin(OpenAtLoginDetail),
    ConfigFile(ConfigFileDetail),
    AdHocRules(AdHocRulesDetail),
}

#[uniffi::remote(Enum)]
pub enum NightShiftDetail {
    Available { schedule: Option<Schedule> },
    Unavailable,
    SetAside,
    NoSunTimes { waited: SignedDuration },
}

#[uniffi::remote(Enum)]
pub enum AccessibilityDetail {
    Allowed,
    NotAllowed,
    NotNeeded,
}

#[uniffi::remote(Enum)]
pub enum NotificationsDetail {
    Allowed,
    SwitchedOff,
    Off,
}

#[uniffi::remote(Enum)]
pub enum OpenAtLoginDetail {
    Opens,
    AwaitingApproval,
    Removed,
    Off,
}

#[uniffi::remote(Enum)]
pub enum ConfigFileDetail {
    Valid { rules: usize },
    Missing,
    Error(FileError),
}

#[uniffi::remote(Enum)]
pub enum AdHocRulesDetail {
    On { rules: usize },
    Off(FileError),
}

#[uniffi::remote(Enum)]
pub enum FileError {
    Unreadable {
        path: String,
        message: String,
    },
    Malformed {
        path: String,
        line: Option<usize>,
        message: String,
    },
    Invalid {
        path: String,
        line: Option<usize>,
        message: String,
    },
    Newer {
        path: String,
        version: u32,
    },
}

// A check's counts and lines cross as u64, which holds any usize on the 64-bit Macs Nightcap runs
// on (ADR-0013).
uniffi::custom_type!(usize, u64, {
    remote,
    lower: |count| u64::try_from(count).unwrap_or(u64::MAX),
    try_lift: |count| Ok(usize::try_from(count)?),
});

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

#[uniffi::remote(Enum)]
pub enum ErrorCode {
    ProtocolMismatch,
    BadRequest,
    NightShiftUnavailable,
    StrengthUnavailable,
    InvalidConfig,
    NoUpdater,
    ConfigRule,
    NoSuchRule,
    AdHocRulesOff,
    SetInConfig,
    NoLoginItem,
    Internal,
}

#[uniffi::remote(Record)]
pub struct ErrorReply {
    pub code: ErrorCode,
    pub message: String,
}

/// Something macOS observed, which the shell reports.
#[derive(Debug, uniffi::Enum)]
pub enum Fact {
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
    /// The wakeup the core last asked for came due.
    TimerFired,
    /// The Mac woke from sleep, so schedule transitions may have passed unseen.
    Woke,
    /// This user's login session became active or inactive, as with fast user switching.
    SessionChanged {
        active: bool,
    },
}

/// A command only the menu-bar UI sends, which never goes on the wire: the CLI has no tour
/// (ADR-0016), no onboarding and no fixes.
#[derive(Debug, uniffi::Enum)]
pub enum UiCommand {
    /// Starts the tour (ADR-0016), with its sample, the site where it has a site step, and the
    /// app its steps give as an example of a rule that says on.
    StartTour {
        sample: TourApp,
        site: Option<TourSite>,
        example: TourApp,
    },
    /// Ends the tour, as it finishes or the user leaves it.
    EndTour,
    /// Finishes onboarding, with whether the user chose to check websites.
    FinishOnboarding { websites: bool },
    /// Carries out one of the core's fixes, which the user chose in Settings.
    Fix(Fix),
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
}

/// Something that happened, which a client may tell the user, once.
#[derive(Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Event {
    /// Night Shift switched outside Nightcap, taken as flipping what the user sees (ADR-0018):
    /// `place` holds `state` until `until`, or until turned back, snoozing its rule, if one
    /// decided.
    Adopted {
        place: Place,
        snoozed: Option<RuleIdentity>,
        state: OnOff,
        until: Option<Timestamp>,
    },
    /// The first launch of `to` after an update from `from`, which is None for a run from
    /// before versions were kept.
    Updated { from: Option<String>, to: String },
    /// A check became a problem the user didn't cause, to announce once: what it found, and
    /// where that leaves it. Night Shift being unavailable, notifications switched off and a
    /// login item waiting for approval never are.
    ProblemAppeared { check: Check },
    /// The core refused a command from the menu-bar UI; the CLI hears its refusal as the reply.
    Refused { command: Command, why: ErrorReply },
}

/// What one fact or command led to, in the order the core decided it.
#[derive(Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct Outcome {
    /// What macOS must do, in order.
    pub effects: Vec<Effect>,
    /// What happened, for the clients to tell.
    pub events: Vec<Event>,
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
