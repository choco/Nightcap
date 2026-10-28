//! The protocol's types as the shell sees them, through UniFFI: the remote mirrors of its enums
//! and records, the queries' answers among them, and the custom types that cross as plain
//! values, with the checks the shell runs before it sends one.

use std::time::SystemTime;

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    About, AccessibilityDetail, AdHocRulesDetail, BundleId, Check, CheckDetail, CheckKind,
    CheckState, Checks, Command, ConfigFileDetail, ConfigState, Decision, Domain, ErrorCode,
    ErrorReply, ExportFormat, FileError, Front, Inherited, Install, Lasting, Length, LoginItem,
    NightShift, NightShiftDetail, NightShiftState, NotificationPermission, NotificationsDetail,
    OnOff, OpenAtLoginDetail, Permissions, Place, PlaceInFront, Preferences, Reason, Rule,
    RuleIdentity, Schedule, SetBy, TimeOfDay, TourState, WebsiteChecking,
};

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

// A length crosses as whole seconds: the config's lengths, and the commands the app sends with
// them, the very ones the CLI would send (ADR-0002).
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
    Valid { rules: u32 },
    Missing,
    Error(FileError),
}

#[uniffi::remote(Enum)]
pub enum AdHocRulesDetail {
    On { rules: u32 },
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
        line: Option<u32>,
        message: String,
    },
    Invalid {
        path: String,
        line: Option<u32>,
        message: String,
    },
    Newer {
        path: String,
        version: u32,
    },
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
pub enum Lasting {
    For(SignedDuration),
    Until(Timestamp),
    UntilScheduleTurns,
    Indefinite,
}

#[uniffi::remote(Enum)]
pub enum Command {
    On,
    Off,
    Toggle,
    Set {
        place: Place,
        state: OnOff,
        length: Lasting,
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

#[uniffi::remote(Enum)]
pub enum NightShift {
    Unavailable,
    On,
    Off,
}

#[uniffi::remote(Record)]
pub struct NightShiftState {
    pub state: NightShift,
    pub strength: Option<u8>,
    pub schedule: Option<Schedule>,
    pub set_aside: bool,
    pub scheduled: Option<OnOff>,
    pub next_on: Option<Timestamp>,
    pub next_off: Option<Timestamp>,
}

#[uniffi::remote(Record)]
pub struct Front {
    pub app: Option<BundleId>,
    pub watching: bool,
    pub host: Option<String>,
    pub site: Option<Domain>,
    pub decision: Decision,
    pub places: Vec<PlaceInFront>,
    pub everywhere: PlaceInFront,
    pub after_switch: Vec<Rule>,
}

#[uniffi::remote(Record)]
pub struct PlaceInFront {
    pub place: Place,
    pub gives: OnOff,
    pub rules: Vec<Rule>,
    pub base: Option<Reason>,
    pub inherits: Option<Inherited>,
    pub after_until: Option<OnOff>,
    pub after_reset: Option<OnOff>,
}

#[uniffi::remote(Record)]
pub struct Inherited {
    pub from: Place,
    pub says: OnOff,
}

#[uniffi::remote(Record)]
pub struct Decision {
    pub state: Option<OnOff>,
    pub place: Place,
    pub reasons: Vec<Reason>,
}

#[uniffi::remote(Enum)]
pub enum Reason {
    Rule(Rule),
    Tour,
    Schedule {
        state: OnOff,
        until: Option<Timestamp>,
    },
}

#[uniffi::remote(Record)]
pub struct Rule {
    pub place: Place,
    pub says: OnOff,
    pub length: Length,
    pub set_by: SetBy,
}

#[uniffi::remote(Enum)]
pub enum Length {
    Indefinite,
    UntilScheduleTurns { until: Option<Timestamp> },
    Until { until: Timestamp },
}

#[uniffi::remote(Enum)]
pub enum SetBy {
    Config,
    You,
    Tour,
    OutsideChange,
}

#[uniffi::remote(Record)]
pub struct ConfigState {
    pub path: String,
    pub found: bool,
    pub error: Option<FileError>,
    pub strength: Option<u8>,
    pub schedule: Option<Schedule>,
    pub websites: Option<OnOff>,
    pub lengths: Vec<SignedDuration>,
    pub ad_hoc_error: Option<FileError>,
}

#[uniffi::remote(Record)]
pub struct Preferences {
    pub website_checking: WebsiteChecking,
    pub notifications_wanted: bool,
    pub open_at_login: bool,
    pub onboarding_completed: u32,
    pub onboarding_finished: bool,
}

#[uniffi::remote(Record)]
pub struct WebsiteChecking {
    pub state: OnOff,
    pub from_config: bool,
}

#[uniffi::remote(Record)]
pub struct Checks {
    pub checks: Vec<Check>,
    pub permissions: Permissions,
}

#[uniffi::remote(Record)]
pub struct Permissions {
    pub accessibility: bool,
    pub notifications: NotificationPermission,
    pub login_item: Option<LoginItem>,
}

#[uniffi::remote(Enum)]
pub enum LoginItem {
    Enabled,
    RequiresApproval,
    NotRegistered,
}

#[uniffi::remote(Record)]
pub struct TourState {
    pub stands_in: bool,
}

#[uniffi::remote(Record)]
pub struct About {
    pub version: String,
    pub install: Install,
}

#[uniffi::remote(Enum)]
pub enum Install {
    Managed,
    SelfManaged,
}
