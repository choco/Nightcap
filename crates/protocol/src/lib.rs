//! What the CLI and the daemon share: the IPC messages and where an instance's socket lives.
//!
//! Every message is one JSON object per line (ADR-0013). A connection opens with a
//! [`HelloMessage`] from the client, answered by a [`HelloReply`]; then the client sends one
//! [`Request`] and receives one [`Response`].

use std::fmt;
use std::path::PathBuf;

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

/// Sent in every `Hello`; the CLI and the daemon must match exactly.
pub const PROTOCOL_VERSION: u32 = 2;

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
    /// Starts Sparkle's own check, whose window offers to install (ADR-0015).
    Update,
}

/// What the CLI and the menu ask Nightcap to do; each one answers with the resulting decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// The switch: Night Shift on everywhere, until turned back (ADR-0024).
    On,
    /// The switch: Night Shift off everywhere, until turned back.
    Off,
    /// The switch, the other way from what everywhere says now.
    Toggle,
    /// Puts Night Shift on or off at a place for a length: a timer for a time or until a
    /// time, a hold otherwise. For good is everywhere's alone; an app's or site's is a rule.
    Set {
        place: Place,
        state: OnOff,
        length: Length,
    },
    /// Takes off a place's hold and timer, which puts its rule back, or for everywhere the
    /// schedule.
    Reset(Place),
    /// Adds an ad-hoc rule, replacing the user's own rule for the same app or domain (ADR-0025).
    AddRule { identity: RuleIdentity, says: OnOff },
    /// Removes an ad-hoc rule; a config rule changes only in the config.
    RemoveRule(RuleIdentity),
    /// Turns website checking on or off, unless the config sets it (ADR-0016).
    Websites(OnOff),
    /// Opens Nightcap at login or stops it, in self-managed installs (ADR-0015).
    OpenAtLogin(OnOff),
    /// Sets Night Shift's strength, 0 to 100, and saves it (ADR-0020).
    Strength(u8),
    /// Shows a strength, 0 to 100, without saving it, while a slider moves; `Strength` ends it.
    PreviewStrength(u8),
    /// Sets Night Shift's schedule, which macOS applies at once (ADR-0020).
    SetSchedule(Schedule),
}

/// Where a change applies: everywhere, an app, or a domain and its subdomains (ADR-0024).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    Everywhere,
    App(String),
    Domain(String),
}

/// `everywhere`, `app:com.figma.Desktop` or `domain:figma.com`.
impl fmt::Display for Place {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Everywhere => formatter.write_str("everywhere"),
            Self::App(bundle_id) => write!(formatter, "app:{bundle_id}"),
            Self::Domain(domain) => write!(formatter, "domain:{domain}"),
        }
    }
}

impl From<RuleIdentity> for Place {
    fn from(identity: RuleIdentity) -> Self {
        match identity {
            RuleIdentity::App(bundle_id) => Self::App(bundle_id),
            RuleIdentity::Domain(domain) => Self::Domain(domain),
        }
    }
}

/// How long a change lasts (ADR-0024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Length {
    /// A timer for this long.
    For(SignedDuration),
    /// A timer until this moment.
    Until(Timestamp),
    /// A hold until the schedule next turns Night Shift the other way: sunrise for an on,
    /// sunset for an off.
    UntilScheduleTurns,
    /// A hold with no end, everywhere's alone.
    UntilTurnedBack,
}

/// A hold or a timer: on or off at a place until a time, or a hold with no end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    pub state: OnOff,
    pub until: Option<Timestamp>,
    /// The place an external change held the same way at once; turning either back takes off
    /// both (ADR-0024).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with: Option<Place>,
}

/// A place's hold and timer, the timer over the hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceLayers {
    pub place: Place,
    pub hold: Option<Layer>,
    pub timer: Option<Layer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Query {
    Status,
    Rules,
    /// The ad-hoc rules that aren't in the config yet, ready to paste into it.
    Export(ExportFormat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Nix,
    Toml,
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
    Status(Box<Status>),
    Decision(Decision),
    /// Sparkle is checking; it shows what it finds in its own window.
    UpdateCheckStarted,
    Rules(Vec<Rule>),
    Export(String),
}

/// What the core makes of everything it knows: what Night Shift should be, the place that
/// decides it, and the layers that place rests on, top first (ADR-0024).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// None while everywhere decides and the schedule isn't known yet.
    pub state: Option<OnOff>,
    pub place: Place,
    pub reasons: Vec<Reason>,
}

impl Decision {
    /// What the core decided and why, as it reads after "Night Shift", with times in `zone`.
    pub fn describe(&self, zone: &TimeZone) -> String {
        let until = |end: Option<Timestamp>, state: OnOff| match end {
            Some(end) => format!(" until {}", end.to_zoned(zone.clone()).strftime("%H:%M")),
            None => format!(" until you turn it {}", state.other()),
        };
        let Some(top) = self.reasons.first() else {
            return "left alone".to_owned();
        };
        let at = match &self.place {
            Place::Everywhere => String::new(),
            place => format!(" at {place}"),
        };
        let mut description = match top {
            Reason::Timer(layer) | Reason::Hold(layer) => {
                format!("{}{at}{}", layer.state, until(layer.until, layer.state))
            }
            Reason::Rule(rule) => format!(
                "{} by the {} rule for {}",
                rule.says, rule.source, rule.identity
            ),
            Reason::Tour => "on for the tour".to_owned(),
            Reason::Schedule { state, .. } => format!("{state}, following the schedule"),
        };
        let snoozed = self.reasons.iter().skip(1).find_map(|reason| match reason {
            Reason::Rule(rule) if Some(rule.says) != self.state => Some(rule),
            _ => None,
        });
        if let Some(rule) = snoozed {
            description.push_str(&format!(", its {} rule snoozed", rule.source));
        }
        description
    }
}

/// One layer a decision rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Timer(Layer),
    Hold(Layer),
    Rule(Rule),
    /// Night Shift on for the tour, which stands in for the schedule while it runs (ADR-0022).
    Tour,
    /// The schedule, until its next transition, if any.
    Schedule {
        state: OnOff,
        until: Option<Timestamp>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub identity: RuleIdentity,
    pub source: RuleSource,
    /// Night Shift off, or on, while the rule matches (ADR-0025).
    pub says: OnOff,
}

/// `app:com.figma.Desktop (config rule, off)`.
impl fmt::Display for Rule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ({} rule, {})",
            self.identity, self.source, self.says
        )
    }
}

/// A rule's kind plus its target, written `app:com.figma.Desktop` or `domain:figma.com`.
/// Bundle IDs compare case-insensitively; domains are lowercase ASCII, IDNs as punycode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum RuleIdentity {
    App(String),
    Domain(String),
}

impl fmt::Display for RuleIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::App(bundle_id) => write!(formatter, "app:{bundle_id}"),
            Self::Domain(domain) => write!(formatter, "domain:{domain}"),
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
            Some(("domain", domain)) if !domain.is_empty() => Ok(Self::Domain(domain.to_owned())),
            _ => Err(format!(
                "{identity} isn't a rule identity like app:com.figma.Desktop or domain:figma.com"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSource {
    Config,
    AdHoc,
    /// The tour's sample window, held only while the tour runs (ADR-0022).
    Tour,
}

impl fmt::Display for RuleSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Config => "config",
            Self::AdHoc => "ad-hoc",
            Self::Tour => "tour",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub install: Install,
    pub night_shift: NightShift,
    /// How warm Night Shift tints the display, 0 to 100; None until the shell first reads it.
    pub strength: Option<u8>,
    /// The strength the config sets, if it sets one (ADR-0020).
    pub config_strength: Option<u8>,
    pub decision: Decision,
    /// None until the shell first reports Night Shift's status.
    pub schedule: Option<Schedule>,
    /// The schedule the config sets, if it sets one (ADR-0020).
    pub config_schedule: Option<Schedule>,
    /// None while the schedule, or the sun times it needs, aren't known.
    pub scheduled_state: Option<OnOff>,
    pub next_transition: Option<Timestamp>,
    /// The frontmost app's bundle ID, when it has one.
    pub frontmost_app: Option<String>,
    /// Whether the frontmost browser's host is being watched (ADR-0016).
    pub watching: bool,
    /// The watched browser's host, once known.
    pub host: Option<String>,
    pub rules: Vec<Rule>,
    /// Why the config couldn't be loaded; the rules are then the ones loaded before, or none.
    pub config_error: Option<String>,
    /// Why `rules.toml` can't be used; ad-hoc rules are then off.
    pub ad_hoc_error: Option<String>,
    pub website_checking: WebsiteChecking,
    /// Every place with a hold or a timer.
    pub layers: Vec<PlaceLayers>,
    pub permissions: Permissions,
    pub checks: Vec<Check>,
}

/// One thing Nightcap depends on, compared with what the user set up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    pub state: CheckState,
    /// One line on how it stands, for Settings and `status`.
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    NightShift,
    Accessibility,
    Notifications,
    OpenAtLogin,
    ConfigFile,
    AdHocRules,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Ok,
    /// Not needed, or off by the user's choice.
    Off,
    /// Something the user set up stopped working. Restoring it or accepting the change as their
    /// choice both settle it.
    Drifted,
    /// Broken, so it can only be fixed.
    Broken,
}

/// Whether Nightcap reads the front browser's host, which domain rules need (ADR-0016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebsiteChecking {
    pub state: OnOff,
    /// Set by `websites` in the config, which wins over the menu and the CLI.
    pub from_config: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnOff {
    On,
    Off,
}

impl OnOff {
    pub fn other(self) -> Self {
        match self {
            Self::On => Self::Off,
            Self::Off => Self::On,
        }
    }
}

impl fmt::Display for OnOff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::On => "on",
            Self::Off => "off",
        })
    }
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
    /// Night Shift works, but its strength can't be read or set on this Mac.
    StrengthUnavailable,
    InvalidConfig,
    /// The install doesn't update itself: its installer does, or it's a dev build.
    NoUpdater,
    /// The identity isn't one a rule can have, such as a domain with a path.
    BadIdentity,
    /// The rule comes from the config, so only the config can change it.
    ConfigRule,
    NoSuchRule,
    /// `rules.toml` is invalid or from a newer Nightcap, so ad-hoc rules can't change.
    AdHocRulesOff,
    /// The config sets this, so only the config can change it.
    SetInConfig,
    /// The install doesn't open itself at login: its installer starts it, or it's a dev build.
    NoLoginItem,
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
            json(&Response::Ok(Payload::Status(Box::new(Status {
                version: "1.2.0".to_owned(),
                install: Install::SelfManaged,
                night_shift: NightShift::On,
                strength: Some(60),
                config_strength: None,
                decision: Decision {
                    state: Some(OnOff::Off),
                    place: Place::Everywhere,
                    reasons: vec![Reason::Schedule {
                        state: OnOff::Off,
                        until: Some("2026-10-16T20:00:00Z".parse().unwrap()),
                    }],
                },
                schedule: Some(Schedule::Custom {
                    from: TimeOfDay {
                        hour: 22,
                        minute: 0
                    },
                    to: TimeOfDay { hour: 7, minute: 0 },
                }),
                config_schedule: None,
                scheduled_state: Some(OnOff::Off),
                next_transition: Some("2026-10-16T20:00:00Z".parse().unwrap()),
                frontmost_app: Some("com.apple.Safari".to_owned()),
                watching: true,
                host: Some("www.figma.com".to_owned()),
                rules: Vec::new(),
                config_error: None,
                ad_hoc_error: None,
                website_checking: WebsiteChecking {
                    state: OnOff::On,
                    from_config: false,
                },
                layers: vec![PlaceLayers {
                    place: Place::App("com.figma.Desktop".to_owned()),
                    hold: None,
                    timer: Some(Layer {
                        state: OnOff::On,
                        until: Some("2026-10-16T19:30:00Z".parse().unwrap()),
                        with: None,
                    }),
                }],
                permissions: Permissions {
                    accessibility: true,
                    notifications: NotificationPermission::NotRequested,
                },
                checks: vec![Check {
                    kind: CheckKind::Accessibility,
                    state: CheckState::Drifted,
                    text: "x".to_owned(),
                }],
            })))),
            r#"{"ok":{"status":{"version":"1.2.0","install":"self_managed","night_shift":"on","strength":60,"config_strength":null,"decision":{"state":"off","place":"everywhere","reasons":[{"schedule":{"state":"off","until":"2026-10-16T20:00:00Z"}}]},"schedule":{"custom":{"from":{"hour":22,"minute":0},"to":{"hour":7,"minute":0}}},"config_schedule":null,"scheduled_state":"off","next_transition":"2026-10-16T20:00:00Z","frontmost_app":"com.apple.Safari","watching":true,"host":"www.figma.com","rules":[],"config_error":null,"ad_hoc_error":null,"website_checking":{"state":"on","from_config":false},"layers":[{"place":{"app":"com.figma.Desktop"},"hold":null,"timer":{"state":"on","until":"2026-10-16T19:30:00Z"}}],"permissions":{"accessibility":true,"notifications":"not_requested"},"checks":[{"kind":"accessibility","state":"drifted","text":"x"}]}}}"#
        );
    }

    #[test]
    fn decisions_match_the_documented_wire_format() {
        assert_eq!(json(&Request::Reload), r#""reload""#);
        assert_eq!(json(&Request::Update), r#""update""#);
        assert_eq!(
            json(&Response::Ok(Payload::UpdateCheckStarted)),
            r#"{"ok":"update_check_started"}"#
        );
        assert_eq!(
            json(&Request::Command(Command::Set {
                place: Place::Everywhere,
                state: OnOff::Off,
                length: Length::For(jiff::SignedDuration::from_mins(90)),
            })),
            r#"{"command":{"set":{"place":"everywhere","state":"off","length":{"for":"PT1H30M"}}}}"#
        );
        assert_eq!(
            json(&Request::Command(Command::Set {
                place: Place::Domain("figma.com".to_owned()),
                state: OnOff::On,
                length: Length::UntilScheduleTurns,
            })),
            r#"{"command":{"set":{"place":{"domain":"figma.com"},"state":"on","length":"until_schedule_turns"}}}"#
        );
        assert_eq!(
            json(&Request::Command(Command::Reset(Place::App(
                "com.figma.Desktop".to_owned()
            )))),
            r#"{"command":{"reset":{"app":"com.figma.Desktop"}}}"#
        );
        assert_eq!(
            json(&Request::Command(Command::Strength(60))),
            r#"{"command":{"strength":60}}"#
        );
        assert_eq!(
            json(&Request::Command(Command::PreviewStrength(60))),
            r#"{"command":{"preview_strength":60}}"#
        );
        assert_eq!(
            json(&Request::Command(Command::SetSchedule(
                Schedule::SunsetToSunrise
            ))),
            r#"{"command":{"set_schedule":"sunset_to_sunrise"}}"#
        );
        assert_eq!(
            json(&Payload::Decision(Decision {
                state: Some(OnOff::On),
                place: Place::App("com.figma.Desktop".to_owned()),
                reasons: vec![
                    Reason::Hold(Layer {
                        state: OnOff::On,
                        until: Some("2026-10-16T05:50:47Z".parse().unwrap()),
                        with: Some(Place::Everywhere),
                    }),
                    Reason::Rule(Rule {
                        identity: RuleIdentity::App("com.figma.Desktop".to_owned()),
                        source: RuleSource::Config,
                        says: OnOff::Off,
                    }),
                ],
            })),
            r#"{"decision":{"state":"on","place":{"app":"com.figma.Desktop"},"reasons":[{"hold":{"state":"on","until":"2026-10-16T05:50:47Z","with":"everywhere"}},{"rule":{"identity":"app:com.figma.Desktop","source":"config","says":"off"}}]}}"#
        );
    }

    #[test]
    fn rule_messages_match_the_documented_wire_format() {
        let figma = RuleIdentity::Domain("figma.com".to_owned());
        assert_eq!(
            json(&Request::Command(Command::AddRule {
                identity: figma.clone(),
                says: OnOff::On,
            })),
            r#"{"command":{"add_rule":{"identity":"domain:figma.com","says":"on"}}}"#
        );
        assert_eq!(
            json(&Request::Command(Command::RemoveRule(figma.clone()))),
            r#"{"command":{"remove_rule":"domain:figma.com"}}"#
        );
        assert_eq!(json(&Request::Query(Query::Rules)), r#"{"query":"rules"}"#);
        assert_eq!(
            json(&Request::Command(Command::Websites(OnOff::On))),
            r#"{"command":{"websites":"on"}}"#
        );
        assert_eq!(
            json(&Request::Query(Query::Export(ExportFormat::Nix))),
            r#"{"query":{"export":"nix"}}"#
        );
        assert_eq!(
            json(&Response::Ok(Payload::Rules(vec![Rule {
                identity: figma,
                source: RuleSource::AdHoc,
                says: OnOff::Off,
            }]))),
            r#"{"ok":{"rules":[{"identity":"domain:figma.com","source":"ad_hoc","says":"off"}]}}"#
        );
        assert_eq!(
            json(&Response::Ok(Payload::Export("{ }".to_owned()))),
            r#"{"ok":{"export":"{ }"}}"#
        );
    }

    #[test]
    fn rule_identities_read_back_as_written() {
        let identity: RuleIdentity = serde_json::from_str(r#""app:com.figma.Desktop""#).unwrap();

        assert_eq!(identity, RuleIdentity::App("com.figma.Desktop".to_owned()));
        let identity: RuleIdentity = serde_json::from_str(r#""domain:figma.com""#).unwrap();
        assert_eq!(identity, RuleIdentity::Domain("figma.com".to_owned()));
        assert_eq!(json(&identity), r#""domain:figma.com""#);
        assert!(serde_json::from_str::<RuleIdentity>(r#""figma""#).is_err());
    }

    #[test]
    fn decisions_describe_themselves_after_night_shift() {
        let zone = jiff::tz::TimeZone::get("Europe/Zurich").unwrap();
        let at = |instant: &str| instant.parse::<Timestamp>().unwrap();
        let figma_app = || Place::App("com.figma.Desktop".to_owned());
        let figma = Rule {
            identity: RuleIdentity::App("com.figma.Desktop".to_owned()),
            source: RuleSource::Config,
            says: OnOff::Off,
        };
        let layer = |state, until| Layer {
            state,
            until,
            with: None,
        };
        let described = |state, place, reasons| {
            Decision {
                state: Some(state),
                place,
                reasons,
            }
            .describe(&zone)
        };

        assert_eq!(
            described(
                OnOff::Off,
                Place::Everywhere,
                vec![
                    Reason::Timer(layer(OnOff::Off, Some(at("2026-10-16T22:00:00Z")))),
                    Reason::Schedule {
                        state: OnOff::On,
                        until: None
                    },
                ]
            ),
            "off until 00:00"
        );
        assert_eq!(
            described(
                OnOff::Off,
                Place::Everywhere,
                vec![Reason::Hold(layer(OnOff::Off, None))]
            ),
            "off until you turn it on"
        );
        assert_eq!(
            described(
                OnOff::On,
                figma_app(),
                vec![
                    Reason::Hold(layer(OnOff::On, Some(at("2026-10-17T05:00:00Z")))),
                    Reason::Rule(figma.clone()),
                ]
            ),
            "on at app:com.figma.Desktop until 07:00, its config rule snoozed"
        );
        assert_eq!(
            described(OnOff::Off, figma_app(), vec![Reason::Rule(figma)]),
            "off by the config rule for app:com.figma.Desktop"
        );
        assert_eq!(
            described(
                OnOff::On,
                Place::Everywhere,
                vec![Reason::Schedule {
                    state: OnOff::On,
                    until: None
                }]
            ),
            "on, following the schedule"
        );
        assert_eq!(
            Decision {
                state: None,
                place: Place::Everywhere,
                reasons: Vec::new(),
            }
            .describe(&zone),
            "left alone"
        );
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
