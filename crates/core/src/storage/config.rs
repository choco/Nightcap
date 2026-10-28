//! `config.toml`: the config rules, declared by the user or their installer (ADR-0007,
//! ADR-0006). It is strict, so a typo is an error rather than a rule that silently never matches.

use std::path::PathBuf;
use std::{fmt, io};

use jiff::SignedDuration;
use nightcap_protocol::{OnOff, Problem, RuleIdentity, Schedule, TimeOfDay};
use serde::{Deserialize, Serialize};
use toml::Spanned;

use crate::rules::schedule;
use crate::storage::file::{self, Files};

/// The file as toml reads it. What's checked after reading keeps its place in the file, so a
/// problem with it names its line.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    /// Website checking, which the app and the CLI then can't change (ADR-0012).
    websites: Option<bool>,
    /// The lengths a change can be picked for, such as "30m", "1h" or "1h30m" (ADR-0018).
    lengths: Option<Spanned<Vec<String>>>,
    #[serde(default)]
    night_shift: NightShiftFile,
    #[serde(default)]
    rules: Rules,
    /// A `[menu]` table, read only to point its `lengths` at the top-level key.
    menu: Option<Spanned<serde::de::IgnoredAny>>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NightShiftFile {
    strength: Option<Spanned<u8>>,
    /// "off", "sunset-to-sunrise", or custom hours such as "22:00-07:00".
    schedule: Option<Spanned<String>>,
}

/// Night Shift's strength and schedule as the config sets them, each optional. Nightcap
/// applies a value when it changes, and leaves alone a change made since (ADR-0014).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NightShiftConfig {
    pub strength: Option<u8>,
    pub schedule: Option<Schedule>,
}

pub(crate) struct Config {
    /// The indefinite rules the config sets, at apps and sites, with what each says.
    pub rules: Vec<(RuleIdentity, OnOff)>,
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
    app: Option<Spanned<serde::de::IgnoredAny>>,
    domain: Option<Spanned<serde::de::IgnoredAny>>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleLists {
    #[serde(default)]
    app: Vec<Spanned<String>>,
    #[serde(default)]
    domain: Vec<Spanned<String>>,
}

/// The lengths a change can be picked for when the config sets none.
pub(crate) fn default_lengths() -> Vec<SignedDuration> {
    vec![
        SignedDuration::from_mins(30),
        SignedDuration::from_hours(1),
        SignedDuration::from_hours(2),
    ]
}

/// Why the config can't be used, which each client words from the checks. It reads as the log
/// may say it: never what the file holds, which can quote a domain (ADR-0009).
#[derive(Debug, thiserror::Error)]
pub(crate) enum ConfigError {
    #[error("the config can't be read: {error}")]
    Unreadable { path: PathBuf, error: io::Error },
    #[error("the config is malformed")]
    Malformed { path: PathBuf, error: Malformed },
    #[error("the config is invalid at line {line}")]
    Invalid {
        path: PathBuf,
        line: u32,
        problem: Problem,
    },
}

/// Why toml can't read a file as what it holds, in one line: the line, where toml knows it,
/// then what.
#[derive(Debug, thiserror::Error)]
pub(crate) struct Malformed {
    pub(crate) line: Option<u32>,
    /// toml's message.
    pub(crate) message: String,
}

impl Malformed {
    /// `error`, which toml found in `contents`.
    pub(crate) fn new(error: &toml::de::Error, contents: &[u8]) -> Self {
        Self {
            line: error.span().map(|span| line_at(contents, span.start)),
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

/// The line, counted from 1, that the byte at `offset` in `contents` is on. A file past
/// `u32::MAX` lines is past any the core reads, so its last lines all count as that one.
pub(crate) fn line_at(contents: &[u8], offset: usize) -> u32 {
    let breaks = contents
        .iter()
        .take(offset)
        .filter(|&&byte| byte == b'\n')
        .count();
    u32::try_from(breaks).map_or(u32::MAX, |breaks| breaks.saturating_add(1))
}

/// The config. A missing file means no rules, as on a fresh brew install.
pub(crate) fn load(files: &dyn Files) -> Result<Config, ConfigError> {
    let path = || files.path(file::File::Config);
    let text = match files.read(file::File::Config) {
        Ok(Some(text)) => text,
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
    let file = toml::from_str(&text).map_err(|error| ConfigError::Malformed {
        path: path(),
        error: Malformed::new(&error, text.as_bytes()),
    })?;
    checked(file).map_err(|problem| ConfigError::Invalid {
        path: path(),
        line: line_at(text.as_bytes(), problem.span().start),
        problem: problem.into_inner(),
    })
}

/// What a well-formed config declares, once it's checked, or its first problem and where it is.
fn checked(file: File) -> Result<Config, Spanned<Problem>> {
    if let Some(unsaid) = file.rules.app.as_ref().or(file.rules.domain.as_ref()) {
        return Err(Spanned::new(unsaid.span(), Problem::UnsaidRules));
    }
    let mut rules: Vec<(RuleIdentity, OnOff)> = Vec::new();
    for (says, lists) in [(OnOff::Off, &file.rules.off), (OnOff::On, &file.rules.on)] {
        let apps = lists
            .app
            .iter()
            .map(|entry| (entry, app_entry(entry.get_ref())));
        let domains = lists
            .domain
            .iter()
            .map(|entry| (entry, domain_entry(entry.get_ref())));
        for (entry, identity) in apps.chain(domains) {
            let at = |problem| Spanned::new(entry.span(), problem);
            let identity = identity.map_err(at)?;
            if let Some((other, _)) = rules
                .iter()
                .find(|(other, said)| *other == identity && *said != says)
            {
                return Err(at(Problem::BothWays {
                    identity: other.clone(),
                }));
            }
            rules.push((identity, says));
        }
    }
    if let Some(menu) = &file.menu {
        return Err(Spanned::new(menu.span(), Problem::LengthsUnderMenu));
    }
    let night_shift = night_shift(file.night_shift)?;
    let lengths = match file.lengths {
        None => default_lengths(),
        Some(lengths) => {
            let read = lengths
                .get_ref()
                .iter()
                .map(|text| length(text))
                .collect::<Option<Vec<_>>>()
                .filter(|read| !read.is_empty());
            let span = lengths.span();
            read.ok_or_else(|| {
                Spanned::new(
                    span,
                    Problem::NotLengths {
                        lengths: lengths.into_inner(),
                    },
                )
            })?
        }
    };
    Ok(Config {
        rules,
        websites: file.websites,
        night_shift,
        lengths,
        found: true,
    })
}

/// The rule identity an app's entry names, or why it names none.
pub(crate) fn app_entry(entry: &str) -> Result<RuleIdentity, Problem> {
    entry
        .parse()
        .map(RuleIdentity::App)
        .map_err(|_| Problem::NotABundleId {
            entry: entry.to_owned(),
        })
}

/// The rule identity a site's entry names, or why it names none.
pub(crate) fn domain_entry(entry: &str) -> Result<RuleIdentity, Problem> {
    entry
        .parse()
        .map(RuleIdentity::Domain)
        .map_err(|_| Problem::NotADomain {
            entry: entry.to_owned(),
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

fn night_shift(file: NightShiftFile) -> Result<NightShiftConfig, Spanned<Problem>> {
    if let Some(strength) = &file.strength
        && *strength.get_ref() > 100
    {
        return Err(Spanned::new(
            strength.span(),
            Problem::StrengthOutOfRange {
                strength: *strength.get_ref(),
            },
        ));
    }
    let schedule = file
        .schedule
        .map(|text| {
            let span = text.span();
            schedule(text.get_ref()).ok_or_else(|| {
                Spanned::new(
                    span,
                    Problem::NotASchedule {
                        schedule: text.into_inner(),
                    },
                )
            })
        })
        .transpose()?;
    Ok(NightShiftConfig {
        strength: file.strength.map(Spanned::into_inner),
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
