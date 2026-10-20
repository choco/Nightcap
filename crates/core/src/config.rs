//! `config.toml`: the config rules, declared by the user or their installer (ADR-0009). It is
//! strict, so a typo is an error rather than a rule that silently never matches.

use std::io::ErrorKind;
use std::path::Path;

use nightcap_protocol::{Rule, RuleIdentity, RuleSource, Schedule, TimeOfDay};
use serde::{Deserialize, Serialize};

use crate::domain;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    /// Website checking, which Settings and the CLI then can't change (ADR-0016).
    websites: Option<bool>,
    #[serde(default)]
    night_shift: NightShiftFile,
    #[serde(default)]
    rules: Rules,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NightShiftFile {
    strength: Option<u8>,
    /// "off", "sunset-to-sunrise", or custom hours such as "22:00-07:00".
    schedule: Option<String>,
}

/// Night Shift's strength and schedule as the config sets them, each optional. Nightcap
/// applies a value when it changes, and leaves alone a change made since (ADR-0020).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NightShiftConfig {
    pub strength: Option<u8>,
    pub schedule: Option<Schedule>,
}

pub(crate) struct Config {
    pub rules: Vec<Rule>,
    pub websites: Option<bool>,
    pub night_shift: NightShiftConfig,
    /// Whether the file exists: a missing one is an empty config.
    pub found: bool,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rules {
    #[serde(default)]
    app: Vec<String>,
    #[serde(default)]
    domain: Vec<String>,
}

/// The config in `path`. A missing file means no rules, as on a fresh brew install.
pub(crate) fn load(path: &Path) -> Result<Config, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(Config {
                rules: Vec::new(),
                websites: None,
                night_shift: NightShiftConfig::default(),
                found: false,
            });
        }
        Err(error) => return Err(format!("couldn't read {}: {error}", path.display())),
    };
    let file: File =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    let domains = file
        .rules
        .domain
        .iter()
        .map(|entry| {
            domain::normalize(entry).ok_or_else(|| {
                format!(
                    "{}: {entry:?} isn't a domain like figma.com: no scheme, path, port or wildcard",
                    path.display()
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let night_shift =
        night_shift(file.night_shift).map_err(|error| format!("{}: {error}", path.display()))?;
    let identities = file
        .rules
        .app
        .into_iter()
        .map(RuleIdentity::App)
        .chain(domains.into_iter().map(RuleIdentity::Domain));
    Ok(Config {
        rules: identities
            .map(|identity| Rule {
                identity,
                source: RuleSource::Config,
            })
            .collect(),
        websites: file.websites,
        night_shift,
        found: true,
    })
}

fn night_shift(file: NightShiftFile) -> Result<NightShiftConfig, String> {
    if let Some(strength) = file.strength
        && strength > 100
    {
        return Err(format!("night_shift.strength is 0 to 100, not {strength}"));
    }
    let schedule = file
        .schedule
        .map(|text| {
            schedule(&text).ok_or_else(|| {
                format!(
                    "night_shift.schedule is \"off\", \"sunset-to-sunrise\" or custom hours such as \"22:00-07:00\", not {text:?}"
                )
            })
        })
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
    let time = jiff::civil::Time::strptime("%H:%M", text).ok()?;
    Some(TimeOfDay {
        hour: u8::try_from(time.hour()).ok()?,
        minute: u8::try_from(time.minute()).ok()?,
    })
}
