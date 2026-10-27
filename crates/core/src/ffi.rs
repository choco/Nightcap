//! The protocol's types as the shell sees them, through UniFFI: the remote mirrors of its enums
//! and records, and the custom types that cross as plain values, with the checks the shell runs
//! before it sends one.

use std::time::SystemTime;

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    AccessibilityDetail, AdHocRulesDetail, BundleId, Check, CheckDetail, CheckKind, CheckState,
    Command, ConfigFileDetail, Domain, ErrorCode, ErrorReply, ExportFormat, FileError, Lasting,
    NightShiftDetail, NotificationPermission, NotificationsDetail, OnOff, OpenAtLoginDetail, Place,
    RuleIdentity, Schedule, TimeOfDay,
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
