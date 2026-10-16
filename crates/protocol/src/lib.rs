//! What the CLI and the daemon share: the IPC messages and where an instance's socket lives.
//!
//! Every message is one JSON object per line (ADR-0013). A connection opens with a
//! [`HelloMessage`] from the client, answered by a [`HelloReply`]; then the client sends one
//! [`Request`] and receives one [`Response`].

use std::fmt;
use std::path::PathBuf;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

/// Sent in every `Hello`; the CLI and the daemon must match exactly.
pub const PROTOCOL_VERSION: u32 = 1;

/// The `sockaddr_un` path limit on macOS, including the terminating NUL.
const SOCKET_PATH_LIMIT: usize = 104;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instance {
    Release,
    Dev,
}

impl Instance {
    pub fn from_bundle_id(bundle_id: &str) -> Option<Self> {
        match bundle_id {
            "io.github.choco.nightcap" => Some(Self::Release),
            "io.github.choco.nightcap.dev" => Some(Self::Dev),
            _ => None,
        }
    }

    /// The folder under `~/Library/Application Support` that holds the socket.
    pub fn support_folder_name(self) -> &'static str {
        match self {
            Self::Release => "Nightcap",
            Self::Dev => "Nightcap Dev",
        }
    }

    /// The folder under each XDG base directory that holds this instance's files.
    pub fn xdg_folder_name(self) -> &'static str {
        match self {
            Self::Release => "nightcap",
            Self::Dev => "nightcap-dev",
        }
    }

    pub fn socket_path(self) -> Result<PathBuf, PathError> {
        let home = std::env::home_dir().ok_or(PathError::NoHome)?;
        let path = home
            .join("Library/Application Support")
            .join(self.support_folder_name())
            .join("nightcap.sock");
        if path.as_os_str().len() >= SOCKET_PATH_LIMIT {
            return Err(PathError::SocketPathTooLong(path));
        }
        Ok(path)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("can't find the home directory")]
    NoHome,
    #[error("socket path {0} is longer than macOS allows")]
    SocketPathTooLong(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    pub version: String,
}

impl Hello {
    pub fn current() -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

/// The client's first line: `{"hello": {...}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloMessage {
    pub hello: Hello,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelloReply {
    Hello(Hello),
    Error(ErrorReply),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    Command(Command),
    Query(Query),
    /// Reads the config again; answers with the status.
    Reload,
}

/// What the CLI asks Nightcap to do; each one answers with the resulting decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    On,
    Off,
    Toggle,
    /// Forces Night Shift off for this long: a timed disable.
    Disable(SignedDuration),
    /// Clears the timed disable, every snooze and the manual state.
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Query {
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Ok(Payload),
    Error(ErrorReply),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Payload {
    Status(Status),
    Decision(Decision),
}

/// What the core makes of everything it knows: whether Night Shift is forced off or left alone,
/// and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub verdict: Verdict,
    pub reasons: Vec<Reason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    ForceOff,
    HandsOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// A rule matching the context forces Night Shift off.
    Rule(Rule),
    /// A timed disable forces Night Shift off.
    TimedDisable { until: Timestamp },
    /// A rule matching the context is snoozed until the next schedule transition, if any.
    Snooze {
        identity: RuleIdentity,
        until: Option<Timestamp>,
    },
    /// Hands off, following the manual state until the next schedule transition, if any.
    ManualState {
        state: OnOff,
        until: Option<Timestamp>,
    },
    /// Hands off, following the schedule until its next transition, if any.
    Schedule {
        state: OnOff,
        until: Option<Timestamp>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub identity: RuleIdentity,
    pub source: RuleSource,
}

/// A rule's kind plus its target, written `app:com.figma.Desktop`. Bundle IDs compare
/// case-insensitively.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum RuleIdentity {
    App(String),
}

impl fmt::Display for RuleIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::App(bundle_id) => write!(formatter, "app:{bundle_id}"),
        }
    }
}

impl From<RuleIdentity> for String {
    fn from(identity: RuleIdentity) -> Self {
        identity.to_string()
    }
}

impl TryFrom<String> for RuleIdentity {
    type Error = String;

    fn try_from(identity: String) -> Result<Self, Self::Error> {
        match identity.split_once(':') {
            Some(("app", bundle_id)) if !bundle_id.is_empty() => {
                Ok(Self::App(bundle_id.to_owned()))
            }
            _ => Err(format!(
                "{identity} isn't a rule identity like app:com.figma.Desktop"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSource {
    Config,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub install: Install,
    pub night_shift: NightShift,
    pub decision: Decision,
    /// The on or off the user set last; it lasts until the next schedule transition.
    pub manual_state: Option<OnOff>,
    /// None until the shell first reports Night Shift's status.
    pub schedule: Option<Schedule>,
    /// None while the schedule, or the sun times it needs, aren't known.
    pub scheduled_state: Option<OnOff>,
    pub next_transition: Option<Timestamp>,
    /// The frontmost app's bundle ID, when it has one.
    pub frontmost_app: Option<String>,
    pub rules: Vec<Rule>,
    /// Why the config couldn't be loaded; the rules are then the ones loaded before, or none.
    pub config_error: Option<String>,
    /// Rules snoozed until the next schedule transition.
    pub snoozes: Vec<RuleIdentity>,
    /// When the timed disable ends, if one is running.
    pub timed_disable: Option<Timestamp>,
    pub permissions: Permissions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnOff {
    On,
    Off,
}

/// Night Shift as last observed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NightShift {
    #[default]
    Unavailable,
    On,
    Off,
}

/// Night Shift's schedule in System Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Schedule {
    Off,
    SunsetToSunrise,
    /// On from `from` until `to`, in local time; `to` falls on the next day when it's earlier.
    Custom {
        from: TimeOfDay,
        to: TimeOfDay,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

/// Who owns updates and starting at login (ADR-0015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Install {
    Managed,
    SelfManaged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    pub accessibility: bool,
    pub notifications: NotificationPermission,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPermission {
    #[default]
    NotRequested,
    Denied,
    Granted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorReply {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ProtocolMismatch,
    BadRequest,
    NightShiftUnavailable,
    InvalidConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json<T: Serialize>(value: &T) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn messages_match_the_documented_wire_format() {
        let hello = Hello {
            protocol: 1,
            version: "1.2.0".to_owned(),
        };
        assert_eq!(
            json(&HelloMessage {
                hello: hello.clone()
            }),
            r#"{"hello":{"protocol":1,"version":"1.2.0"}}"#
        );
        assert_eq!(
            json(&HelloReply::Error(ErrorReply {
                code: ErrorCode::ProtocolMismatch,
                message: "x".to_owned()
            })),
            r#"{"error":{"code":"protocol_mismatch","message":"x"}}"#
        );
        assert_eq!(
            json(&Request::Query(Query::Status)),
            r#"{"query":"status"}"#
        );
        assert_eq!(
            json(&Request::Command(Command::Toggle)),
            r#"{"command":"toggle"}"#
        );
        assert_eq!(
            json(&Response::Ok(Payload::Status(Status {
                version: "1.2.0".to_owned(),
                install: Install::SelfManaged,
                night_shift: NightShift::On,
                decision: Decision {
                    verdict: Verdict::HandsOff,
                    reasons: Vec::new(),
                },
                manual_state: None,
                schedule: Some(Schedule::Custom {
                    from: TimeOfDay {
                        hour: 22,
                        minute: 0
                    },
                    to: TimeOfDay { hour: 7, minute: 0 },
                }),
                scheduled_state: Some(OnOff::Off),
                next_transition: Some("2026-10-16T20:00:00Z".parse().unwrap()),
                frontmost_app: Some("com.apple.Terminal".to_owned()),
                rules: Vec::new(),
                config_error: None,
                snoozes: Vec::new(),
                timed_disable: None,
                permissions: Permissions {
                    accessibility: true,
                    notifications: NotificationPermission::NotRequested,
                },
            }))),
            r#"{"ok":{"status":{"version":"1.2.0","install":"self_managed","night_shift":"on","decision":{"verdict":"hands_off","reasons":[]},"manual_state":null,"schedule":{"custom":{"from":{"hour":22,"minute":0},"to":{"hour":7,"minute":0}}},"scheduled_state":"off","next_transition":"2026-10-16T20:00:00Z","frontmost_app":"com.apple.Terminal","rules":[],"config_error":null,"snoozes":[],"timed_disable":null,"permissions":{"accessibility":true,"notifications":"not_requested"}}}}"#
        );
    }

    #[test]
    fn decisions_match_the_documented_wire_format() {
        assert_eq!(json(&Request::Reload), r#""reload""#);
        assert_eq!(
            json(&Request::Command(Command::Disable(
                jiff::SignedDuration::from_mins(90)
            ))),
            r#"{"command":{"disable":"PT1H30M"}}"#
        );
        assert_eq!(
            json(&Decision {
                verdict: Verdict::ForceOff,
                reasons: vec![Reason::TimedDisable {
                    until: "2026-10-16T05:50:47Z".parse().unwrap(),
                }],
            }),
            r#"{"verdict":"force_off","reasons":[{"timed_disable":{"until":"2026-10-16T05:50:47Z"}}]}"#
        );
        assert_eq!(
            json(&Reason::Snooze {
                identity: RuleIdentity::App("com.figma.Desktop".to_owned()),
                until: None,
            }),
            r#"{"snooze":{"identity":"app:com.figma.Desktop","until":null}}"#
        );
        assert_eq!(
            json(&Payload::Decision(Decision {
                verdict: Verdict::ForceOff,
                reasons: vec![Reason::Rule(Rule {
                    identity: RuleIdentity::App("com.figma.Desktop".to_owned()),
                    source: RuleSource::Config,
                })],
            })),
            r#"{"decision":{"verdict":"force_off","reasons":[{"rule":{"identity":"app:com.figma.Desktop","source":"config"}}]}}"#
        );
        assert_eq!(
            json(&Decision {
                verdict: Verdict::HandsOff,
                reasons: vec![Reason::ManualState {
                    state: OnOff::On,
                    until: Some("2026-10-16T05:50:47Z".parse().unwrap()),
                }],
            }),
            r#"{"verdict":"hands_off","reasons":[{"manual_state":{"state":"on","until":"2026-10-16T05:50:47Z"}}]}"#
        );
    }

    #[test]
    fn rule_identities_read_back_as_written() {
        let identity: RuleIdentity = serde_json::from_str(r#""app:com.figma.Desktop""#).unwrap();

        assert_eq!(identity, RuleIdentity::App("com.figma.Desktop".to_owned()));
        assert!(serde_json::from_str::<RuleIdentity>(r#""figma""#).is_err());
    }

    #[test]
    fn instances_come_from_bundle_ids() {
        assert_eq!(
            Instance::from_bundle_id("io.github.choco.nightcap"),
            Some(Instance::Release)
        );
        assert_eq!(
            Instance::from_bundle_id("io.github.choco.nightcap.dev"),
            Some(Instance::Dev)
        );
        assert_eq!(Instance::from_bundle_id("com.example.other"), None);
    }
}
