//! What the CLI and the server share: the IPC messages and where an instance's socket lives.
//!
//! Every message is one JSON object per line (ADR-0010). A connection opens with a
//! [`HelloMessage`] from the client, answered by a [`HelloReply`]; then the client sends one
//! [`Request`] and receives one [`Response`]. They carry facts, never words: each client words
//! them itself.
//!
//! The commands and the answers cross to the shell as they are, through UniFFI (ADR-0002).

use std::fmt;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;

use idna::AsciiDenyList;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Deserializer, Serialize, de};

uniffi::setup_scaffolding!();

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

/// Sent in every `Hello`; the CLI and the server must speak the same one exactly.
pub const PROTOCOL_VERSION: u32 = 14;

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
    /// The client speaks another protocol: the server's own hello, for the client to say which
    /// Nightcap answered and that it needs a restart.
    Mismatch(Hello),
    /// The hello isn't one: serde's reason.
    Malformed {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    Command(Command),
    /// Answered in the order asked, all from one snapshot.
    Queries(Vec<Query>),
}

/// What the CLI and the app ask Nightcap to do. Each answers with the resulting decision, short of
/// a reload, which answers with every rule, and an update check, which answers that it started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// The switch: Night Shift on everywhere, until turned back (ADR-0018).
    On,
    /// The switch: Night Shift off everywhere, until turned back.
    Off,
    /// The switch, the other way from what everywhere says now.
    Toggle,
    /// Sets a rule of the user's at a place, saying on or off for a length (ADR-0018). An
    /// indefinite one is everywhere's alone: an app's or site's is added with `AddRule`.
    Set {
        place: Place,
        state: OnOff,
        length: Lasting,
    },
    /// Takes off a place's temporary rules, which puts back its indefinite rule, and for
    /// everywhere its indefinite one too, which puts back the schedule.
    Reset(Place),
    /// Adds an ad-hoc rule, replacing the user's own rule for the same app or domain (ADR-0006).
    AddRule { identity: RuleIdentity, says: OnOff },
    /// Removes an ad-hoc rule; a config rule changes only in the config.
    RemoveRule(RuleIdentity),
    /// Turns website checking on or off, unless the config sets it (ADR-0012).
    Websites(OnOff),
    /// Opens Nightcap at login or stops it, in self-managed installs (ADR-0011).
    OpenAtLogin(OnOff),
    /// Accepts notifications switched off in System Settings as the user's choice, until they're
    /// allowed again.
    KeepNotificationsOff,
    /// Accepts the schedule off while macOS has sunset to sunrise set aside, until Location
    /// Services is back on.
    KeepScheduleOff,
    /// Sets Night Shift's strength, 0 to 100, and saves it (ADR-0014).
    Strength(u8),
    /// Shows a strength, 0 to 100, without saving it, while a slider moves; `Strength` ends it.
    PreviewStrength(u8),
    /// Sets Night Shift's schedule, which macOS applies at once (ADR-0014).
    SetSchedule(Schedule),
    /// Reads the config again, which is how a change to it takes effect.
    Reload,
    /// Starts Sparkle's own check, whose window offers to install, in a self-managed release
    /// (ADR-0011).
    CheckForUpdates,
}

/// Where a change applies: everywhere, an app, or a domain and its subdomains (ADR-0018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    Everywhere,
    App(BundleId),
    Domain(Domain),
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

#[uniffi::export]
impl Place {
    /// The identity an app's or site's indefinite rule here has; everywhere has none.
    pub fn identity(&self) -> Option<RuleIdentity> {
        match self {
            Self::Everywhere => None,
            Self::App(bundle_id) => Some(RuleIdentity::App(bundle_id.clone())),
            Self::Domain(domain) => Some(RuleIdentity::Domain(domain.clone())),
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

/// How long a rule that `Set` asks for lasts (ADR-0018).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Lasting {
    /// This long from now.
    For(SignedDuration),
    /// Until this moment.
    Until(Timestamp),
    /// Until the schedule next turns Night Shift the other way: when it ends for an on, when it
    /// starts for an off.
    UntilScheduleTurns,
    /// Until it's taken off.
    Indefinite,
}

/// What a client can ask, answered from the core's last snapshot without changing anything
/// (ADR-0010): facts, never words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Query {
    NightShift,
    Front,
    /// Every rule at every place, each with its snooze: the apps' and sites' indefinite rules
    /// first, then each place's others together, shortest first.
    Rules,
    Config,
    Preferences,
    Checks,
    Trial,
    About,
    /// The ad-hoc rules that aren't in the config yet, ready to paste into it.
    Export(ExportFormat),
}

/// What a query answers, named as the query is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    NightShift(NightShiftState),
    Front(Box<Front>),
    Rules(Vec<ListedRule>),
    Config(Box<ConfigState>),
    Preferences(Preferences),
    Checks(Checks),
    /// None while no trial runs.
    Trial(Option<TrialState>),
    About(About),
    Export(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Nix,
    Toml,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Ok(Payload),
    Refused(Refusal),
    /// The request isn't one this Nightcap reads: serde's reason.
    Malformed {
        message: String,
    },
}

/// A command's answer, or why it was refused.
impl From<Result<Payload, Refusal>> for Response {
    fn from(answer: Result<Payload, Refusal>) -> Self {
        match answer {
            Ok(payload) => Self::Ok(payload),
            Err(refusal) => Self::Refused(refusal),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Payload {
    /// The queries' answers, in the order asked.
    Answers(Vec<Answer>),
    /// Every rule, after a reload, as the rules query lists them.
    Rules(Vec<ListedRule>),
    /// What a command leads to.
    Decision(Decision),
    /// What adding a rule did, and the decision it leads to.
    RuleAdded { rule: RuleAdded, decision: Decision },
    /// Sparkle is checking; it shows what it finds in its own window.
    UpdateCheckStarted,
}

/// Night Shift as the shell last reported it, and its schedule (ADR-0014).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct NightShiftState {
    pub state: NightShift,
    /// How warm Night Shift tints the display, 0 to 100; None until the shell first reads it.
    pub strength: Option<u8>,
    /// None until the shell first reports it.
    pub schedule: Option<Schedule>,
    /// macOS has sunset to sunrise set aside, and the schedule off, while Location Services is
    /// off.
    pub set_aside: bool,
    /// What the schedule says now; None while the schedule, or the sun times it needs, aren't
    /// known.
    pub scheduled: Option<OnOff>,
    /// When the schedule next turns Night Shift on, at sunset or as custom hours start, which
    /// ends a rule until the schedule turns that says off (ADR-0018). None while no such turn is
    /// coming, or while the sun times it needs aren't known.
    pub next_on: Option<Timestamp>,
    /// When the schedule next turns Night Shift off, at sunrise or as custom hours end, which
    /// ends a rule until the schedule turns that says on.
    pub next_off: Option<Timestamp>,
}

impl NightShiftState {
    /// The schedule's next transition, whichever way it turns Night Shift.
    pub fn next_transition(&self) -> Option<Timestamp> {
        self.next_on.into_iter().chain(self.next_off).min()
    }
}

/// What's in front, and what decides Night Shift there (ADR-0018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Front {
    /// The frontmost app's bundle ID, when it has one a rule could name.
    pub app: Option<BundleId>,
    /// Whether website checking reads the front browser's host (ADR-0012).
    pub watching: bool,
    /// The front tab's host, once read.
    pub host: Option<String>,
    /// The site in front while website checking knows the host: the most specific domain with
    /// a rule covering it, else the tab domain (ADR-0017).
    pub site: Option<Domain>,
    /// What's in effect: the deciding place, what it says, and by which rule, else the stand-in
    /// or the schedule.
    pub decision: Decision,
    /// The site in front and the front app, where there are, most specific first, each as it
    /// stands.
    pub places: Vec<PlaceInFront>,
    /// Everywhere, the last place in front, as it stands under them.
    pub everywhere: PlaceInFront,
}

/// A place in front as it stands (ADR-0018): what it gives, its rules and what they sit on, and
/// what it inherits from the places in front after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct PlaceInFront {
    pub place: Place,
    /// Its own say, else the one it inherits, else Night Shift as last reported.
    pub gives: OnOff,
    /// Its rules over its base, shortest first: its temporary rules, and for everywhere its
    /// indefinite one too.
    pub rules: Vec<Rule>,
    /// What its rules sit on: an app's or site's indefinite rule, or for everywhere the stand-in,
    /// else the schedule once it's known.
    pub base: Option<Reason>,
    /// The first less specific place in front that says something, and what it says.
    pub inherits: Option<Inherited>,
}

/// What a place in front inherits: a less specific place's say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Inherited {
    pub from: Place,
    pub says: OnOff,
}

/// The config as Nightcap last read it (ADR-0007). Whether it can be used is its check's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct ConfigState {
    /// Where it's read from, whether or not it exists.
    pub path: String,
    /// The strength it sets, if it sets one (ADR-0014).
    pub strength: Option<u8>,
    /// The schedule it sets, if it sets one (ADR-0014).
    pub schedule: Option<Schedule>,
    /// Website checking, if it sets it, which the app and the CLI then can't change
    /// (ADR-0012).
    pub websites: Option<OnOff>,
    /// The lengths a change can be picked for: its own, else 30 minutes, 1 hour and 2 hours.
    pub lengths: Vec<SignedDuration>,
}

/// The user's choices about how Nightcap runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Preferences {
    pub website_checking: WebsiteChecking,
    /// Whether the user wants Nightcap's notifications, which the notifications check compares
    /// against.
    pub notifications_wanted: bool,
    /// Whether the user wants Nightcap to open at login (ADR-0011).
    pub open_at_login: bool,
}

/// The checks, and what macOS last reported behind them: the grants and the login item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Checks {
    pub checks: Vec<Check>,
    pub permissions: Permissions,
}

/// A trial that runs (ADR-0016).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct TrialState {
    /// It has Night Shift on, standing in for the schedule, as the app asked when it began it.
    pub stands_in: bool,
}

/// The Nightcap that answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct About {
    pub version: String,
    pub install: Install,
}

/// What the core makes of everything it knows: what Night Shift should be, the place that
/// decides it, and what that place rests on, its shortest rule first (ADR-0018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Decision {
    /// None while everywhere decides and the schedule isn't known yet.
    pub state: Option<OnOff>,
    pub place: Place,
    pub reasons: Vec<Reason>,
}

/// What adding a rule did at its app or site (ADR-0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAdded {
    /// A rule where there was none, the user's, or during a trial the trial's (ADR-0016).
    New { set_by: SetBy },
    /// The rule there said the other way, and now says this one: the user's own, or a trial's.
    Changed { set_by: SetBy },
    /// The rule there already says so: the config's, the user's or a trial's.
    AlreadySays { set_by: SetBy },
}

/// One thing a decision rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Rule(Rule),
    /// Night Shift on for a trial, which stands in for the schedule while it runs (ADR-0016).
    StandIn,
    /// The schedule, until its next transition, if any.
    Schedule {
        state: OnOff,
        until: Option<Timestamp>,
    },
}

/// A change to Night Shift: a place, what it says there, how long it lasts and who set it
/// (ADR-0018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Rule {
    pub place: Place,
    /// Night Shift off, or on, while the place is in front (ADR-0006).
    pub says: OnOff,
    pub length: Length,
    pub set_by: SetBy,
}

/// A rule as the rules query lists it, written as the rule with its snooze beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct ListedRule {
    #[serde(flatten)]
    pub rule: Rule,
    /// The rule that snoozes it: its place's shortest, where that's shorter and says the other
    /// way. It waits underneath, and comes back when its snooze ends (ADR-0018).
    pub snooze: Option<Rule>,
}

/// How long a rule lasts (ADR-0018). At one place, the shortest decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Length {
    /// Until it's taken off.
    Indefinite,
    /// Until the schedule next turns Night Shift the other way: when it ends for an on, when it
    /// starts for an off. None while the schedule has no turn to wait for, as with no schedule.
    UntilScheduleTurns { until: Option<Timestamp> },
    /// Until a time.
    Until { until: Timestamp },
}

#[uniffi::export]
impl Length {
    /// When a rule of this length ends, if it has an end.
    pub fn end(&self) -> Option<Timestamp> {
        match self {
            Self::Indefinite => None,
            Self::UntilScheduleTurns { until } => *until,
            Self::Until { until } => Some(*until),
        }
    }
}

/// Who set a rule (ADR-0018).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum SetBy {
    /// The config, whose rules are indefinite, at an app or a site (ADR-0007).
    Config,
    /// The user, from the app or the CLI.
    You,
    /// The user during a trial, kept only while it runs (ADR-0016).
    Trial,
    /// An outside change, which Nightcap adopted.
    OutsideChange,
}

/// The app or the site an indefinite rule is at, written `app:com.figma.Desktop` or
/// `domain:figma.com`: each has one at most (ADR-0006).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, uniffi::Enum)]
#[serde(into = "String")]
pub enum RuleIdentity {
    App(BundleId),
    Domain(Domain),
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

impl FromStr for RuleIdentity {
    type Err = IdentityError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.split_once(':') {
            Some(("app", bundle_id)) => Ok(Self::App(bundle_id.parse()?)),
            Some(("domain", domain)) => Ok(Self::Domain(domain.parse()?)),
            _ => Err(IdentityError::RuleIdentity(text.to_owned())),
        }
    }
}

impl<'de> Deserialize<'de> for RuleIdentity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        parsed(deserializer)
    }
}

/// An app's bundle ID, such as `com.figma.Desktop`: letters, digits, hyphens and periods. It
/// keeps its spelling, and compares and hashes in any case.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct BundleId(String);

impl BundleId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq for BundleId {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for BundleId {}

impl Hash for BundleId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for byte in self.0.bytes() {
            state.write_u8(byte.to_ascii_lowercase());
        }
        // Ends as `str`'s hash does, so no bundle ID's hash is a prefix of a longer one's.
        state.write_u8(0xff);
    }
}

impl fmt::Display for BundleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<BundleId> for String {
    fn from(bundle_id: BundleId) -> Self {
        bundle_id.0
    }
}

impl FromStr for BundleId {
    type Err = IdentityError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let valid = !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(IdentityError::BundleId(text.to_owned()))
        }
    }
}

impl<'de> Deserialize<'de> for BundleId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        parsed(deserializer)
    }
}

/// A domain in the one form rules compare: lowercase ASCII, IDNs as punycode, no trailing dot
/// (ADR-0006). It's made only by normalizing what was written.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Domain(String);

impl Domain {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The domain one label up, as figma.com is to www.figma.com, or None for a top-level one.
    pub fn parent(&self) -> Option<Self> {
        self.0
            .split_once('.')
            .map(|(_, parent)| Self(parent.to_owned()))
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<Domain> for String {
    fn from(domain: Domain) -> Self {
        domain.0
    }
}

/// `text` as a bare domain; anything else is an error: a URL, a host with a port, a wildcard,
/// an empty label.
impl FromStr for Domain {
    type Err = IdentityError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bare = text.strip_suffix('.').unwrap_or(text);
        idna::domain_to_ascii_cow(bare.as_bytes(), AsciiDenyList::URL)
            .ok()
            .filter(|ascii| !ascii.contains('*') && ascii.split('.').all(|label| !label.is_empty()))
            .map(|ascii| Self(ascii.into_owned()))
            .ok_or_else(|| IdentityError::Domain(text.to_owned()))
    }
}

impl<'de> Deserialize<'de> for Domain {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        parsed(deserializer)
    }
}

// The shell sees bundle IDs and domains as strings. One it sends that isn't valid fails to lift,
// which stops the app, so the shell checks any it didn't get from the core.
uniffi::custom_type!(BundleId, String, {
    lower: |bundle_id| bundle_id.into(),
    try_lift: |text| Ok(text.parse()?),
});

uniffi::custom_type!(Domain, String, {
    lower: |domain| domain.into(),
    try_lift: |text| Ok(text.parse()?),
});

/// Why text doesn't name an app, a domain or a rule. A domain's message quotes it, so the core
/// never logs one (ADR-0009).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    #[error(
        "{0:?} isn't a bundle ID like com.figma.Desktop: only letters, digits, hyphens and periods"
    )]
    BundleId(String),
    #[error("{0:?} isn't a domain like figma.com: no scheme, path, port or wildcard")]
    Domain(String),
    #[error("{0} isn't a rule identity like app:com.figma.Desktop or domain:figma.com")]
    RuleIdentity(String),
}

/// Reads a value written as a string through its `FromStr`.
fn parsed<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    String::deserialize(deserializer)?
        .parse()
        .map_err(de::Error::custom)
}

/// One thing Nightcap depends on, compared with what the user set up: what it found, which each
/// client words itself, and where that leaves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Check {
    /// Written as the check's `kind` and, beside it, its `detail`.
    #[serde(flatten)]
    pub detail: CheckDetail,
    pub state: CheckState,
}

/// What a check found, by its kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum CheckDetail {
    NightShift(NightShiftDetail),
    Accessibility(AccessibilityDetail),
    Notifications(NotificationsDetail),
    OpenAtLogin(OpenAtLoginDetail),
    ConfigFile(ConfigFileDetail),
    AdHocRules(AdHocRulesDetail),
}

#[uniffi::export]
impl CheckDetail {
    pub fn kind(&self) -> CheckKind {
        match self {
            Self::NightShift(_) => CheckKind::NightShift,
            Self::Accessibility(_) => CheckKind::Accessibility,
            Self::Notifications(_) => CheckKind::Notifications,
            Self::OpenAtLogin(_) => CheckKind::OpenAtLogin,
            Self::ConfigFile(_) => CheckKind::ConfigFile,
            Self::AdHocRules(_) => CheckKind::AdHocRules,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum NightShiftDetail {
    /// Night Shift works on this Mac, with this schedule; None until the shell first reports it.
    Available { schedule: Option<Schedule> },
    /// This Mac has no Night Shift.
    Unavailable,
    /// macOS has sunset to sunrise set aside, and the schedule off, while Location Services is
    /// off.
    SetAside,
    /// Sunset to sunrise has gone too long without sun times, so Nightcap can't follow it.
    NoSunTimes,
}

/// Accessibility, which website checking needs to read the front tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum AccessibilityDetail {
    Allowed,
    /// Not allowed, with website checking on.
    NotAllowed,
    /// Not needed, with website checking off.
    NotNeeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum NotificationsDetail {
    Allowed,
    /// Allowed, then switched off in System Settings.
    SwitchedOff,
    /// Off by the user's choice.
    Off,
}

/// Opening at login, in self-managed installs (ADR-0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum OpenAtLoginDetail {
    /// Nightcap opens when the user logs in.
    Opens,
    /// The user wants it, and macOS is waiting for their approval in Login Items.
    AwaitingApproval,
    /// The user wanted it, and it was removed in System Settings.
    Removed,
    /// Off by the user's choice.
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFileDetail {
    /// Valid, with this many rules.
    Valid { rules: u32 },
    /// There's no config file, which is an empty config.
    Missing,
    /// It can't be used; the rules are the ones loaded before, or none.
    Error(FileError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum AdHocRulesDetail {
    /// In use, with this many rules.
    On { rules: u32 },
    /// Off, because `rules.toml` can't be used.
    Off(FileError),
}

/// Why one of Nightcap's files, the config or `rules.toml`, can't be used, with where it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum FileError {
    /// It can't be read: why, by kind.
    Unreadable { path: String, failure: IoFailure },
    /// It isn't valid TOML: where toml found the first error, where toml says.
    Malformed { path: String, at: Option<Position> },
    /// Valid TOML, but a key, an entry or a value in it can't be: which, and its line.
    Invalid {
        path: String,
        line: u32,
        problem: Problem,
    },
    /// It's from a newer Nightcap, which wrote this version of it.
    Newer { path: String, version: i64 },
}

/// What a value in a file is, as TOML writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    String,
    /// A list of strings, or an empty one.
    Strings,
    /// A list with something other than a string in it.
    List,
    Integer,
    Float,
    Boolean,
    /// A date, a time of day, or both.
    Datetime,
    Table,
}

/// Where in a file's text: a line, and a column on it in characters, each counted from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

/// Why macOS couldn't read or write a file or a folder, by kind, which each client words itself;
/// the log has macOS's own reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum IoFailure {
    /// Nothing is there, or at a folder on its path.
    NotFound,
    /// macOS doesn't let Nightcap read or write it.
    PermissionDenied,
    /// A folder is where the file should be.
    IsAFolder,
    /// A file is where a folder on its path should be.
    NotAFolder,
    /// What it holds isn't UTF-8 text.
    NotUtf8,
    /// Any other reason.
    Other,
}

impl From<io::ErrorKind> for IoFailure {
    /// `InvalidData` is how reading text fails on what isn't UTF-8, as `fs::read_to_string`
    /// does.
    fn from(kind: io::ErrorKind) -> Self {
        match kind {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            io::ErrorKind::IsADirectory => Self::IsAFolder,
            io::ErrorKind::NotADirectory => Self::NotAFolder,
            io::ErrorKind::InvalidData => Self::NotUtf8,
            _ => Self::Other,
        }
    }
}

/// What makes a well-formed config or `rules.toml` invalid, with the key, entry or value as the
/// file writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    /// A key its table doesn't take: the keys that lead to the table from the top of the file,
    /// the key, and the keys the table takes.
    UnknownKey {
        table: Vec<String>,
        key: String,
        expected: Vec<String>,
    },
    /// A key whose value isn't of the type Nightcap reads there: the keys that lead to its table,
    /// the key, what its value is, and what it takes.
    WrongType {
        table: Vec<String>,
        key: String,
        found: ValueType,
        expected: ValueType,
    },
    /// A key its table must have, which it lacks: the keys that lead to the table, and the key.
    MissingKey { table: Vec<String>, key: String },
    /// An app's entry that isn't a bundle ID: letters, digits, hyphens and periods.
    NotABundleId { entry: String },
    /// A site's entry that isn't a bare domain: no scheme, path, port or wildcard.
    NotADomain { entry: String },
    /// An app or domain under both `rules.off` and `rules.on`, which has one rule (ADR-0006).
    BothWays { identity: RuleIdentity },
    /// `night_shift.strength` outside 0 to 100 (ADR-0014).
    StrengthOutOfRange { strength: i64 },
    /// A `night_shift.schedule` that's none of "off", "sunset-to-sunrise" or custom hours such
    /// as "22:00-07:00".
    NotASchedule { schedule: String },
    /// `lengths` that aren't each from a minute to a day in whole minutes, or that are none
    /// (ADR-0018).
    NotLengths { lengths: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    NightShift,
    Accessibility,
    Notifications,
    OpenAtLogin,
    ConfigFile,
    AdHocRules,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
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

#[uniffi::export]
impl CheckState {
    /// Whether the check needs the user: drift, or broken.
    pub fn is_problem(self) -> bool {
        matches!(self, Self::Drifted | Self::Broken)
    }
}

/// Whether Nightcap reads the front browser's host, which domain rules need (ADR-0012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct WebsiteChecking {
    pub state: OnOff,
    /// Set by `websites` in the config, which wins over the app and the CLI.
    pub from_config: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum OnOff {
    On,
    Off,
}

#[uniffi::export]
impl OnOff {
    pub fn other(self) -> Self {
        match self {
            Self::On => Self::Off,
            Self::Off => Self::On,
        }
    }
}

impl From<bool> for OnOff {
    fn from(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum NightShift {
    #[default]
    Unavailable,
    On,
    Off,
}

/// Night Shift's schedule in System Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

/// Who owns updates and starting at login (ADR-0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Install {
    /// A release the user can replace, which updates itself and opens itself at login if the
    /// user wants.
    SelfManaged,
    NotSelfManaged(NotSelfManaged),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Permissions {
    pub accessibility: bool,
    pub notifications: NotificationPermission,
    /// Nightcap's login item, which opening at login's check compares with the user's choice
    /// (ADR-0011); None until the shell first reads it.
    pub login_item: Option<LoginItem>,
}

/// Nightcap's login item, as macOS reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum LoginItem {
    Enabled,
    /// Registered, but waiting for the user's approval in System Settings.
    RequiresApproval,
    NotRegistered,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPermission {
    #[default]
    NotRequested,
    Denied,
    Granted,
}

/// Why Nightcap refused a command, which then changes nothing: what each client needs to say
/// why, in its own words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// This Mac has no Night Shift, which the command needs.
    NightShiftUnavailable,
    /// Night Shift works, but its strength can't be read or set on this Mac.
    StrengthUnavailable,
    /// A strength is 0 to 100.
    StrengthOutOfRange { strength: u8 },
    /// One end of custom hours isn't a time of day, from 00:00 to 23:59.
    NotATimeOfDay(TimeOfDay),
    /// A length of zero or less (ADR-0018).
    TooShort,
    /// A length that would end past any time Nightcap can keep.
    TooLong { length: SignedDuration },
    /// Until a time that has passed.
    Passed,
    /// Until the schedule turns, with no schedule to wait for.
    NoSchedule,
    /// Only everywhere's indefinite rule is set: an app's or a site's is added as a rule.
    IndefiniteIsAdded,
    /// The config's rule for the app or domain says the other way, and only the config changes
    /// it (ADR-0006).
    ConfigSays { identity: RuleIdentity, says: OnOff },
    /// The rule is the config's, so only the config removes it.
    ConfigRule { identity: RuleIdentity },
    /// The app or domain has no rule of the user's or a trial's.
    NoSuchRule { identity: RuleIdentity },
    /// `rules.toml` can't be used, so ad-hoc rules can't change (ADR-0007).
    AdHocRulesOff(FileError),
    /// `rules.toml` couldn't be written: why, by kind. The rules stay as they were.
    AdHocRulesNotSaved { path: String, failure: IoFailure },
    /// The config can't be used, so a reload keeps the rules loaded before.
    InvalidConfig(FileError),
    /// `websites` in the config sets website checking (ADR-0012).
    WebsiteCheckingInConfig,
    /// The install doesn't update itself (ADR-0011).
    NoUpdater(NotSelfManaged),
    /// The install doesn't open itself at login (ADR-0011).
    NoLoginItem(NotSelfManaged),
    /// An accept while its check doesn't drift: there's no change to take as the user's choice.
    NothingToAccept { check: CheckKind },
}

/// What runs where it isn't a self-managed release (ADR-0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum NotSelfManaged {
    /// A dev build the user can replace, which would shadow the installed app: it's rebuilt,
    /// and started by hand.
    DevBuild,
    /// A managed install, which the user can't replace: its installer updates and starts it.
    ManagedInstall,
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
            json(&HelloReply::Mismatch(hello)),
            r#"{"mismatch":{"protocol":1,"version":"1.2.0"}}"#
        );
        assert_eq!(
            json(&HelloReply::Malformed {
                message: "x".to_owned()
            }),
            r#"{"malformed":{"message":"x"}}"#
        );
        assert_eq!(
            json(&Request::Command(Command::Toggle)),
            r#"{"command":"toggle"}"#
        );
        assert_eq!(
            json(&Response::Malformed {
                message: "x".to_owned()
            }),
            r#"{"malformed":{"message":"x"}}"#
        );
    }

    /// Writes `value` as the wire does and reads it back.
    fn round_trip<T>(value: T)
    where
        T: Serialize + de::DeserializeOwned + PartialEq + fmt::Debug,
    {
        let line = json(&value);
        assert_eq!(serde_json::from_str::<T>(&line).unwrap(), value, "{line}");
    }

    fn figma_app() -> Place {
        Place::App("com.figma.Desktop".parse().unwrap())
    }

    fn figma_site() -> Place {
        Place::Domain("figma.com".parse().unwrap())
    }

    fn at(time: &str) -> Timestamp {
        time.parse().unwrap()
    }

    /// A file error of every kind, with every problem, value type and I/O failure in them.
    fn file_errors() -> Vec<FileError> {
        let path = || "/Users/me/.config/nightcap/config.toml".to_owned();
        let table = || vec!["rules".to_owned(), "off".to_owned()];
        let value_types = [
            ValueType::String,
            ValueType::Strings,
            ValueType::List,
            ValueType::Integer,
            ValueType::Float,
            ValueType::Boolean,
            ValueType::Datetime,
            ValueType::Table,
        ];
        let problems = value_types
            .map(|found| Problem::WrongType {
                table: table(),
                key: "app".to_owned(),
                found,
                expected: ValueType::Strings,
            })
            .into_iter()
            .chain([
                Problem::UnknownKey {
                    table: table(),
                    key: "domian".to_owned(),
                    expected: vec!["app".to_owned(), "domain".to_owned()],
                },
                Problem::MissingKey {
                    table: Vec::new(),
                    key: "version".to_owned(),
                },
                Problem::NotABundleId {
                    entry: "com.figma.Desktop ".to_owned(),
                },
                Problem::NotADomain {
                    entry: "https://figma.com".to_owned(),
                },
                Problem::BothWays {
                    identity: RuleIdentity::App("com.figma.Desktop".parse().unwrap()),
                },
                Problem::StrengthOutOfRange { strength: 101 },
                Problem::NotASchedule {
                    schedule: "dusk".to_owned(),
                },
                Problem::NotLengths {
                    lengths: vec!["45m".to_owned(), "forever".to_owned()],
                },
            ]);
        let failures = [
            IoFailure::NotFound,
            IoFailure::PermissionDenied,
            IoFailure::IsAFolder,
            IoFailure::NotAFolder,
            IoFailure::NotUtf8,
            IoFailure::Other,
        ];
        failures
            .map(|failure| FileError::Unreadable {
                path: path(),
                failure,
            })
            .into_iter()
            .chain(problems.map(|problem| FileError::Invalid {
                path: path(),
                line: 4,
                problem,
            }))
            .chain([
                FileError::Malformed {
                    path: path(),
                    at: Some(Position {
                        line: 2,
                        column: 12,
                    }),
                },
                FileError::Malformed {
                    path: path(),
                    at: None,
                },
                FileError::Newer {
                    path: path(),
                    version: 3,
                },
            ])
            .collect()
    }

    #[test]
    fn handshakes_read_back_as_written() {
        round_trip(HelloMessage {
            hello: Hello::current(),
        });
        round_trip(HelloReply::Hello(Hello::current()));
        round_trip(HelloReply::Mismatch(Hello::current()));
        round_trip(HelloReply::Malformed {
            message: "x".to_owned(),
        });
    }

    #[test]
    fn requests_read_back_as_written() {
        let figma = RuleIdentity::Domain("figma.com".parse().unwrap());
        let custom = Schedule::Custom {
            from: TimeOfDay {
                hour: 22,
                minute: 0,
            },
            to: TimeOfDay { hour: 7, minute: 0 },
        };
        let lastings = [
            Lasting::For(SignedDuration::from_mins(90)),
            Lasting::Until(at("2026-10-16T20:00:00Z")),
            Lasting::UntilScheduleTurns,
            Lasting::Indefinite,
        ];
        let commands = lastings
            .map(|length| Command::Set {
                place: figma_site(),
                state: OnOff::Off,
                length,
            })
            .into_iter()
            .chain([
                Command::On,
                Command::Off,
                Command::Toggle,
                Command::Reset(figma_app()),
                Command::Reset(Place::Everywhere),
                Command::AddRule {
                    identity: figma.clone(),
                    says: OnOff::On,
                },
                Command::RemoveRule(figma),
                Command::Websites(OnOff::On),
                Command::OpenAtLogin(OnOff::Off),
                Command::KeepNotificationsOff,
                Command::KeepScheduleOff,
                Command::Strength(60),
                Command::PreviewStrength(60),
                Command::SetSchedule(Schedule::Off),
                Command::SetSchedule(Schedule::SunsetToSunrise),
                Command::SetSchedule(custom),
                Command::Reload,
                Command::CheckForUpdates,
            ]);
        for command in commands {
            round_trip(Request::Command(command));
        }
        round_trip(Request::Queries(vec![
            Query::NightShift,
            Query::Front,
            Query::Rules,
            Query::Config,
            Query::Preferences,
            Query::Checks,
            Query::Trial,
            Query::About,
            Query::Export(ExportFormat::Nix),
            Query::Export(ExportFormat::Toml),
        ]));
    }

    #[test]
    fn answers_read_back_as_written() {
        let figma_rule = || Rule {
            place: figma_site(),
            says: OnOff::Off,
            length: Length::Indefinite,
            set_by: SetBy::Config,
        };
        let front = Front {
            app: Some("com.apple.Safari".parse().unwrap()),
            watching: true,
            host: Some("www.figma.com".to_owned()),
            site: Some("figma.com".parse().unwrap()),
            decision: Decision {
                state: Some(OnOff::Off),
                place: figma_site(),
                reasons: vec![Reason::Rule(figma_rule())],
            },
            places: vec![PlaceInFront {
                place: figma_site(),
                gives: OnOff::Off,
                rules: vec![Rule {
                    length: Length::Until {
                        until: at("2026-10-16T19:30:00Z"),
                    },
                    set_by: SetBy::You,
                    ..figma_rule()
                }],
                base: Some(Reason::Rule(figma_rule())),
                inherits: Some(Inherited {
                    from: Place::Everywhere,
                    says: OnOff::On,
                }),
            }],
            everywhere: PlaceInFront {
                place: Place::Everywhere,
                gives: OnOff::On,
                rules: Vec::new(),
                base: Some(Reason::Schedule {
                    state: OnOff::On,
                    until: Some(at("2026-10-17T05:00:00Z")),
                }),
                inherits: None,
            },
        };
        round_trip(Response::Ok(Payload::Answers(vec![
            Answer::NightShift(NightShiftState {
                state: NightShift::On,
                strength: Some(60),
                schedule: Some(Schedule::SunsetToSunrise),
                set_aside: false,
                scheduled: Some(OnOff::Off),
                next_on: Some(at("2026-10-16T20:00:00Z")),
                next_off: None,
            }),
            Answer::Front(Box::new(front)),
            Answer::Rules(vec![ListedRule {
                rule: figma_rule(),
                snooze: None,
            }]),
            Answer::Config(Box::new(ConfigState {
                path: "/Users/me/.config/nightcap/config.toml".to_owned(),
                strength: Some(40),
                schedule: None,
                websites: Some(OnOff::On),
                lengths: vec![SignedDuration::from_mins(30), SignedDuration::from_mins(90)],
            })),
            Answer::Preferences(Preferences {
                website_checking: WebsiteChecking {
                    state: OnOff::On,
                    from_config: true,
                },
                notifications_wanted: true,
                open_at_login: false,
            }),
            Answer::Checks(Checks {
                checks: vec![Check {
                    detail: CheckDetail::Accessibility(AccessibilityDetail::NotAllowed),
                    state: CheckState::Drifted,
                }],
                permissions: Permissions {
                    accessibility: false,
                    notifications: NotificationPermission::Granted,
                    login_item: Some(LoginItem::RequiresApproval),
                },
            }),
            Answer::Trial(Some(TrialState { stands_in: true })),
            Answer::Trial(None),
            Answer::About(About {
                version: "1.2.0".to_owned(),
                install: Install::SelfManaged,
            }),
            Answer::About(About {
                version: "1.2.0".to_owned(),
                install: Install::NotSelfManaged(NotSelfManaged::DevBuild),
            }),
            Answer::Export("{ }".to_owned()),
        ])));
    }

    #[test]
    fn command_answers_read_back_as_written() {
        let figma = || RuleIdentity::App("com.figma.Desktop".parse().unwrap());
        let decision = || Decision {
            state: Some(OnOff::On),
            place: Place::Everywhere,
            reasons: vec![
                Reason::StandIn,
                Reason::Schedule {
                    state: OnOff::Off,
                    until: None,
                },
            ],
        };
        let added = [
            SetBy::Config,
            SetBy::You,
            SetBy::Trial,
            SetBy::OutsideChange,
        ]
        .into_iter()
        .flat_map(|set_by| {
            [
                RuleAdded::New { set_by },
                RuleAdded::Changed { set_by },
                RuleAdded::AlreadySays { set_by },
            ]
        })
        .map(|rule| Payload::RuleAdded {
            rule,
            decision: decision(),
        });
        let payloads = added.chain([
            Payload::Decision(decision()),
            Payload::Rules(Vec::new()),
            Payload::UpdateCheckStarted,
        ]);
        for payload in payloads {
            round_trip(Response::Ok(payload));
        }
        let refusals = file_errors()
            .into_iter()
            .flat_map(|error| {
                [
                    Refusal::AdHocRulesOff(error.clone()),
                    Refusal::InvalidConfig(error),
                ]
            })
            .chain([
                Refusal::NightShiftUnavailable,
                Refusal::StrengthUnavailable,
                Refusal::StrengthOutOfRange { strength: 101 },
                Refusal::NotATimeOfDay(TimeOfDay {
                    hour: 24,
                    minute: 0,
                }),
                Refusal::TooShort,
                Refusal::TooLong {
                    length: SignedDuration::from_hours(200_000),
                },
                Refusal::Passed,
                Refusal::NoSchedule,
                Refusal::IndefiniteIsAdded,
                Refusal::ConfigSays {
                    identity: figma(),
                    says: OnOff::Off,
                },
                Refusal::ConfigRule { identity: figma() },
                Refusal::NoSuchRule {
                    identity: RuleIdentity::Domain("figma.com".parse().unwrap()),
                },
                Refusal::AdHocRulesNotSaved {
                    path: "/Users/me/.local/share/nightcap/rules.toml".to_owned(),
                    failure: IoFailure::PermissionDenied,
                },
                Refusal::WebsiteCheckingInConfig,
                Refusal::NoUpdater(NotSelfManaged::DevBuild),
                Refusal::NoLoginItem(NotSelfManaged::ManagedInstall),
                Refusal::NothingToAccept {
                    check: CheckKind::Notifications,
                },
            ]);
        for refusal in refusals {
            round_trip(Response::Refused(refusal));
        }
        round_trip(Response::Malformed {
            message: "x".to_owned(),
        });
    }

    // serde reads a flattened field back through a buffer of its own, where a type can fail to
    // read what it wrote: a check's detail, adjacently tagged, is one.
    #[test]
    fn checks_read_back_with_every_detail() {
        let details = file_errors()
            .into_iter()
            .flat_map(|error| {
                [
                    CheckDetail::ConfigFile(ConfigFileDetail::Error(error.clone())),
                    CheckDetail::AdHocRules(AdHocRulesDetail::Off(error)),
                ]
            })
            .chain([
                CheckDetail::NightShift(NightShiftDetail::Available {
                    schedule: Some(Schedule::SunsetToSunrise),
                }),
                CheckDetail::NightShift(NightShiftDetail::Available { schedule: None }),
                CheckDetail::NightShift(NightShiftDetail::Unavailable),
                CheckDetail::NightShift(NightShiftDetail::SetAside),
                CheckDetail::NightShift(NightShiftDetail::NoSunTimes),
                CheckDetail::Accessibility(AccessibilityDetail::Allowed),
                CheckDetail::Accessibility(AccessibilityDetail::NotAllowed),
                CheckDetail::Accessibility(AccessibilityDetail::NotNeeded),
                CheckDetail::Notifications(NotificationsDetail::Allowed),
                CheckDetail::Notifications(NotificationsDetail::SwitchedOff),
                CheckDetail::Notifications(NotificationsDetail::Off),
                CheckDetail::OpenAtLogin(OpenAtLoginDetail::Opens),
                CheckDetail::OpenAtLogin(OpenAtLoginDetail::AwaitingApproval),
                CheckDetail::OpenAtLogin(OpenAtLoginDetail::Removed),
                CheckDetail::OpenAtLogin(OpenAtLoginDetail::Off),
                CheckDetail::ConfigFile(ConfigFileDetail::Valid { rules: 2 }),
                CheckDetail::ConfigFile(ConfigFileDetail::Missing),
                CheckDetail::AdHocRules(AdHocRulesDetail::On { rules: 0 }),
            ]);
        for detail in details {
            for state in [
                CheckState::Ok,
                CheckState::Off,
                CheckState::Drifted,
                CheckState::Broken,
            ] {
                round_trip(Check {
                    detail: detail.clone(),
                    state,
                });
            }
        }
    }

    // A listed rule's rule is flattened beside its snooze.
    #[test]
    fn listed_rules_read_back_with_every_length() {
        let lengths = [
            Length::Indefinite,
            Length::UntilScheduleTurns { until: None },
            Length::UntilScheduleTurns {
                until: Some(at("2026-10-16T05:50:47Z")),
            },
            Length::Until {
                until: at("2026-10-16T19:30:00Z"),
            },
        ];
        for place in [Place::Everywhere, figma_app(), figma_site()] {
            for length in lengths {
                for set_by in [
                    SetBy::Config,
                    SetBy::You,
                    SetBy::Trial,
                    SetBy::OutsideChange,
                ] {
                    let rule = Rule {
                        place: place.clone(),
                        says: OnOff::Off,
                        length,
                        set_by,
                    };
                    round_trip(ListedRule {
                        rule: rule.clone(),
                        snooze: None,
                    });
                    round_trip(ListedRule {
                        snooze: Some(Rule {
                            says: OnOff::On,
                            ..rule.clone()
                        }),
                        rule,
                    });
                }
            }
        }
    }

    #[test]
    fn an_io_error_crosses_as_its_kind() {
        for (kind, failure) in [
            (io::ErrorKind::NotFound, IoFailure::NotFound),
            (io::ErrorKind::PermissionDenied, IoFailure::PermissionDenied),
            (io::ErrorKind::IsADirectory, IoFailure::IsAFolder),
            (io::ErrorKind::NotADirectory, IoFailure::NotAFolder),
            (io::ErrorKind::InvalidData, IoFailure::NotUtf8),
            (io::ErrorKind::StorageFull, IoFailure::Other),
        ] {
            assert_eq!(IoFailure::from(kind), failure, "{kind:?}");
        }
    }

    #[test]
    fn drift_and_broken_checks_are_problems() {
        assert!(!CheckState::Ok.is_problem());
        assert!(!CheckState::Off.is_problem());
        assert!(CheckState::Drifted.is_problem());
        assert!(CheckState::Broken.is_problem());
    }

    #[test]
    fn rule_identities_read_back_as_written() {
        let identity: RuleIdentity = serde_json::from_str(r#""app:com.figma.Desktop""#).unwrap();

        assert_eq!(
            identity,
            RuleIdentity::App("com.figma.Desktop".parse().unwrap())
        );
        let identity: RuleIdentity = serde_json::from_str(r#""domain:figma.com""#).unwrap();
        assert_eq!(identity, RuleIdentity::Domain("figma.com".parse().unwrap()));
        assert_eq!(json(&identity), r#""domain:figma.com""#);
        assert!(serde_json::from_str::<RuleIdentity>(r#""figma""#).is_err());
    }

    #[test]
    fn bundle_ids_keep_their_spelling_and_compare_in_any_case() {
        let written: BundleId = "com.figma.Desktop".parse().unwrap();
        let lowercase: BundleId = "com.figma.desktop".parse().unwrap();
        let hash = |bundle_id: &BundleId| {
            let mut hasher = std::hash::DefaultHasher::new();
            bundle_id.hash(&mut hasher);
            hasher.finish()
        };

        assert_eq!(written, lowercase);
        assert_eq!(hash(&written), hash(&lowercase));
        assert_eq!(written.to_string(), "com.figma.Desktop");
        assert_eq!(json(&written), r#""com.figma.Desktop""#);
        assert_eq!(
            Place::App(written.clone()),
            serde_json::from_str(r#"{"app":"com.figma.desktop"}"#).unwrap()
        );
        for bad in [
            "",
            "com.figma.Desktop ",
            "com.figma.Desktop/",
            "com.foo_bar",
        ] {
            assert_eq!(
                bad.parse::<BundleId>(),
                Err(IdentityError::BundleId(bad.to_owned()))
            );
        }
    }

    #[test]
    fn domains_are_made_only_by_normalizing() {
        let domain = |text: &str| text.parse::<Domain>().map(String::from);

        assert_eq!(domain("WWW.Figma.COM."), Ok("www.figma.com".to_owned()));
        assert_eq!(domain("bücher.ch"), Ok("xn--bcher-kva.ch".to_owned()));
        for bad in [
            "",
            "https://figma.com",
            "figma.com:443",
            "*.figma.com",
            "figma..com",
        ] {
            assert_eq!(domain(bad), Err(IdentityError::Domain(bad.to_owned())));
        }
        assert_eq!(
            serde_json::from_str::<Place>(r#"{"domain":"Figma.COM"}"#).unwrap(),
            Place::Domain("figma.com".parse().unwrap())
        );
        assert!(serde_json::from_str::<Place>(r#"{"domain":"figma.com/x"}"#).is_err());
        let www: Domain = "www.figma.com".parse().unwrap();
        assert_eq!(www.parent(), Some("figma.com".parse().unwrap()));
        assert_eq!("com".parse::<Domain>().unwrap().parent(), None);
    }

    #[test]
    fn a_rule_identity_names_what_it_cant_be() {
        assert_eq!(
            "app:com.figma.Desktop ".parse::<RuleIdentity>(),
            Err(IdentityError::BundleId("com.figma.Desktop ".to_owned()))
        );
        assert_eq!(
            "domain:https://figma.com".parse::<RuleIdentity>(),
            Err(IdentityError::Domain("https://figma.com".to_owned()))
        );
        assert_eq!(
            "figma".parse::<RuleIdentity>(),
            Err(IdentityError::RuleIdentity("figma".to_owned()))
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
