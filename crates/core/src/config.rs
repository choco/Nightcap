//! `config.toml`: the config rules, declared by the user or their installer (ADR-0007,
//! ADR-0006). It is strict, so a typo is an error rather than a rule that silently never matches.

use std::path::PathBuf;
use std::{fmt, io};

use jiff::SignedDuration;
use nightcap_protocol::{
    IdentityError, OnOff, Rule, RuleIdentity, RuleSource, Schedule, TimeOfDay,
};
use serde::{Deserialize, Serialize};

use crate::file::{self, Files};
use crate::{capitalized, schedule};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    /// Website checking, which Settings and the CLI then can't change (ADR-0012).
    websites: Option<bool>,
    #[serde(default)]
    night_shift: NightShiftFile,
    #[serde(default)]
    rules: Rules,
    #[serde(default)]
    menu: MenuFile,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MenuFile {
    /// The menu's preset lengths, such as "30m", "1h" or "1h30m" (ADR-0018).
    lengths: Option<Vec<String>>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NightShiftFile {
    strength: Option<u8>,
    /// "off", "sunset-to-sunrise", or custom hours such as "22:00-07:00".
    schedule: Option<String>,
}

/// Night Shift's strength and schedule as the config sets them, each optional. Nightcap
/// applies a value when it changes, and leaves alone a change made since (ADR-0014).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NightShiftConfig {
    pub strength: Option<u8>,
    pub schedule: Option<Schedule>,
}

pub(crate) struct Config {
    pub rules: Vec<Rule>,
    pub websites: Option<bool>,
    pub night_shift: NightShiftConfig,
    pub lengths: Vec<SignedDuration>,
    /// Whether the file exists: a missing one is an empty config.
    pub found: bool,
}

/// Rules by what they say: `[rules.off]` and `[rules.on]` (ADR-0006).
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rules {
    #[serde(default)]
    off: RuleLists,
    #[serde(default)]
    on: RuleLists,
    /// Lists straight under `[rules]`, which name no say; read only to point at the tables.
    app: Option<serde::de::IgnoredAny>,
    domain: Option<serde::de::IgnoredAny>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleLists {
    #[serde(default)]
    app: Vec<String>,
    #[serde(default)]
    domain: Vec<String>,
}

/// The menu's preset lengths when the config sets none.
pub(crate) fn default_lengths() -> Vec<SignedDuration> {
    vec![
        SignedDuration::from_mins(30),
        SignedDuration::from_hours(1),
        SignedDuration::from_hours(2),
    ]
}

/// Why the config can't be used. What it says can quote the file, and so a domain, so the log
/// gets only its `kind` (ADR-0009).
#[derive(Debug, thiserror::Error)]
pub(crate) enum ConfigError {
    #[error("couldn't read {}: {error}", .path.display())]
    Unreadable { path: PathBuf, error: io::Error },
    #[error("{}: {error}", .path.display())]
    Malformed { path: PathBuf, error: Malformed },
    #[error("{}: {problem}", .path.display())]
    Invalid { path: PathBuf, problem: Problem },
}

impl ConfigError {
    /// What went wrong, as the log may say it: never what the file holds.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Unreadable { .. } => "can't be read",
            Self::Malformed { .. } => "is malformed",
            Self::Invalid { .. } => "is invalid",
        }
    }

    /// Where and what, in a sentence without the path, which Settings shows above it: "Line 4:
    /// unknown field `domian`, expected `app` or `domain`." A problem keeps its first letter: it
    /// starts with the key or entry it names, as the file writes it.
    pub(crate) fn sentence(&self) -> String {
        match self {
            Self::Unreadable { error, .. } => format!("Couldn't read it: {error}."),
            Self::Malformed { error, .. } => format!("{}.", capitalized(&error.to_string())),
            Self::Invalid { problem, .. } => format!("{problem}."),
        }
    }
}

/// Why toml can't read a file as what it holds, in one line: the line, where toml knows it,
/// then what.
#[derive(Debug, thiserror::Error)]
pub(crate) struct Malformed {
    line: Option<usize>,
    message: String,
}

impl Malformed {
    /// `error`, which toml found in `contents`.
    pub(crate) fn new(error: &toml::de::Error, contents: &[u8]) -> Self {
        let line = error.span().map(|span| {
            1 + contents
                .iter()
                .take(span.start)
                .filter(|&&byte| byte == b'\n')
                .count()
        });
        Self {
            line,
            message: error.message().to_owned(),
        }
    }
}

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// What makes a well-formed config invalid.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Problem {
    #[error("rules are listed by what they say, under [rules.off] and [rules.on]")]
    UnsaidRules,
    #[error(transparent)]
    Entry(#[from] IdentityError),
    #[error("{0} is in both rules.off and rules.on: an app or domain has one rule")]
    BothWays(RuleIdentity),
    #[error("night_shift.strength is 0 to 100, not {0}")]
    Strength(u8),
    #[error(
        "night_shift.schedule is \"off\", \"sunset-to-sunrise\" or custom hours such as \"22:00-07:00\", not {0:?}"
    )]
    Schedule(String),
    #[error(
        "menu.lengths are lengths like \"30m\", \"1h\" or \"1h30m\", from a minute to a day, not {0:?}"
    )]
    Lengths(Vec<String>),
}

/// The config. A missing file means no rules, as on a fresh brew install.
pub(crate) fn load(files: &dyn Files) -> Result<Config, ConfigError> {
    let path = || files.path(file::File::Config);
    let contents = match files.read(file::File::Config) {
        Ok(Some(contents)) => contents,
        Ok(None) => {
            return Ok(Config {
                rules: Vec::new(),
                websites: None,
                night_shift: NightShiftConfig::default(),
                lengths: default_lengths(),
                found: false,
            });
        }
        Err(error) => {
            return Err(ConfigError::Unreadable {
                path: path(),
                error,
            });
        }
    };
    let file = toml::from_slice(&contents).map_err(|error| ConfigError::Malformed {
        path: path(),
        error: Malformed::new(&error, &contents),
    })?;
    checked(file).map_err(|problem| ConfigError::Invalid {
        path: path(),
        problem,
    })
}

/// What a well-formed config declares, once it's checked.
fn checked(file: File) -> Result<Config, Problem> {
    if file.rules.app.is_some() || file.rules.domain.is_some() {
        return Err(Problem::UnsaidRules);
    }
    let mut rules: Vec<Rule> = Vec::new();
    for (says, lists) in [(OnOff::Off, &file.rules.off), (OnOff::On, &file.rules.on)] {
        let apps = lists
            .app
            .iter()
            .map(|entry| entry.parse().map(RuleIdentity::App));
        let domains = lists
            .domain
            .iter()
            .map(|entry| entry.parse().map(RuleIdentity::Domain));
        for identity in apps.chain(domains) {
            let identity = identity?;
            if let Some(other) = rules
                .iter()
                .find(|rule| rule.identity == identity && rule.says != says)
            {
                return Err(Problem::BothWays(other.identity.clone()));
            }
            rules.push(Rule {
                identity,
                source: RuleSource::Config,
                says,
            });
        }
    }
    let night_shift = night_shift(file.night_shift)?;
    let lengths = match file.menu.lengths {
        None => default_lengths(),
        Some(lengths) => lengths
            .iter()
            .map(|text| length(text))
            .collect::<Option<Vec<_>>>()
            .filter(|lengths| !lengths.is_empty())
            .ok_or(Problem::Lengths(lengths))?,
    };
    Ok(Config {
        rules,
        websites: file.websites,
        night_shift,
        lengths,
        found: true,
    })
}

/// A preset length: a whole number of minutes, from one to a day.
fn length(text: &str) -> Option<SignedDuration> {
    let length: SignedDuration = text.parse().ok()?;
    (length >= SignedDuration::from_mins(1)
        && length <= SignedDuration::from_hours(24)
        && length.as_secs() % 60 == 0
        && length.subsec_nanos() == 0)
        .then_some(length)
}

fn night_shift(file: NightShiftFile) -> Result<NightShiftConfig, Problem> {
    if let Some(strength) = file.strength
        && strength > 100
    {
        return Err(Problem::Strength(strength));
    }
    let schedule = file
        .schedule
        .map(|text| schedule(&text).ok_or(Problem::Schedule(text)))
        .transpose()?;
    Ok(NightShiftConfig {
        strength: file.strength,
        schedule,
    })
}

fn schedule(text: &str) -> Option<Schedule> {
    match text {
        "off" => Some(Schedule::Off),
        "sunset-to-sunrise" => Some(Schedule::SunsetToSunrise),
        _ => {
            let (from, to) = text.split_once('-')?;
            Some(Schedule::Custom {
                from: time_of_day(from)?,
                to: time_of_day(to)?,
            })
        }
    }
}

fn time_of_day(text: &str) -> Option<TimeOfDay> {
    jiff::civil::Time::strptime("%H:%M", text)
        .ok()
        .map(schedule::time_of_day)
}
