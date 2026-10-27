//! What crosses between the shell and the core (ADR-0002): the events the shell reports, the
//! effects it carries out, the waker the core's threads ask for a tick with, and the protocol's
//! types as the shell sees them.

use std::time::SystemTime;

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    BundleId, CheckKind, CheckState, Command, Domain, ExportFormat, Length, NotificationPermission,
    OnOff, Place, RuleIdentity, Schedule, TimeOfDay,
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
    /// The tour started (ADR-0016), with its sample, the site where it has a site step, and
    /// the app its steps give as an example of a rule that says on.
    TourStarted {
        sample: TourApp,
        site: Option<TourSite>,
        example: TourApp,
    },
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
