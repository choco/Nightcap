//! `config.toml`: the config rules, declared by the user or their installer (ADR-0009,
//! ADR-0025). It is strict, so a typo is an error rather than a rule that silently never matches.

use std::io::ErrorKind;
use std::path::Path;

use jiff::SignedDuration;
use nightcap_protocol::{OnOff, Rule, RuleIdentity, RuleSource, Schedule, TimeOfDay};
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
    #[serde(default)]
    menu: MenuFile,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MenuFile {
    /// The menu's preset lengths, such as "30m", "1h" or "1h30m" (ADR-0024).
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
    pub lengths: Vec<SignedDuration>,
    /// Whether the file exists: a missing one is an empty config.
    pub found: bool,
}

/// Rules by what they say: `[rules.off]` and `[rules.on]` (ADR-0025).
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rules {
    #[serde(default)]
    off: RuleLists,
    #[serde(default)]
    on: RuleLists,
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

/// The config in `path`. A missing file means no rules, as on a fresh brew install.
pub(crate) fn load(path: &Path) -> Result<Config, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(Config {
                rules: Vec::new(),
                websites: None,
                night_shift: NightShiftConfig::default(),
                lengths: default_lengths(),
                found: false,
            });
        }
        Err(error) => return Err(format!("couldn't read {}: {error}", path.display())),
    };
    let file: File =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    let in_file = |error: String| format!("{}: {error}", path.display());
    let mut rules: Vec<Rule> = Vec::new();
    for (says, lists) in [(OnOff::Off, &file.rules.off), (OnOff::On, &file.rules.on)] {
        let domains = lists
            .domain
            .iter()
            .map(|entry| {
                domain::normalize(entry).ok_or_else(|| {
                    in_file(format!(
                        "{entry:?} isn't a domain like figma.com: no scheme, path, port or wildcard"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let identities = lists
            .app
            .iter()
            .cloned()
            .map(RuleIdentity::App)
            .chain(domains.into_iter().map(RuleIdentity::Domain));
        for identity in identities {
            if let Some(other) = rules
                .iter()
                .find(|rule| same_identity(&rule.identity, &identity) && rule.says != says)
            {
                return Err(in_file(format!(
                    "{} is in both rules.off and rules.on: an app or domain has one rule",
                    other.identity
                )));
            }
            rules.push(Rule {
                identity,
                source: RuleSource::Config,
                says,
            });
        }
    }
    let night_shift = night_shift(file.night_shift).map_err(in_file)?;
    let lengths = match file.menu.lengths {
        None => default_lengths(),
        Some(lengths) => lengths
            .iter()
            .map(|text| length(text))
            .collect::<Option<Vec<_>>>()
            .filter(|lengths| !lengths.is_empty())
            .ok_or_else(|| {
                in_file(format!(
                    "menu.lengths are lengths like \"30m\", \"1h\" or \"1h30m\", from a minute to a day, not {lengths:?}"
                ))
            })?,
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

/// Bundle IDs compare in any case, domains as they're normalized.
fn same_identity(a: &RuleIdentity, b: &RuleIdentity) -> bool {
    match (a, b) {
        (RuleIdentity::App(a), RuleIdentity::App(b)) => a.eq_ignore_ascii_case(b),
        (RuleIdentity::Domain(a), RuleIdentity::Domain(b)) => a == b,
        _ => false,
    }
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
