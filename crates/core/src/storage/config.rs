//! `config.toml`: the config rules, declared by the user or their installer (ADR-0007,
//! ADR-0006). It is strict, so a typo is an error rather than a rule that silently never matches.

use std::io;
use std::path::PathBuf;

use jiff::SignedDuration;
use nightcap_protocol::{OnOff, Problem, RuleIdentity, Schedule, TimeOfDay};
use serde::{Deserialize, Serialize};
use toml::Spanned;

use crate::rules::schedule;
use crate::storage::document::{self, Malformed, Table};
use crate::storage::file::{self, Files};

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

/// The lengths a change can be picked for when the config sets none.
pub(crate) fn default_lengths() -> Vec<SignedDuration> {
    vec![
        SignedDuration::from_mins(30),
        SignedDuration::from_hours(1),
        SignedDuration::from_hours(2),
    ]
}

/// Why the config can't be used, which each client words from the checks. It reads as the log
/// may say it: never what the file holds, such as a key or a value, which can be a domain
/// (ADR-0009).
#[derive(Debug, thiserror::Error)]
pub(crate) enum ConfigError {
    #[error("the config can't be read: {error}")]
    Unreadable { path: PathBuf, error: io::Error },
    #[error("the config {error}")]
    Malformed { path: PathBuf, error: Malformed },
    #[error("the config is invalid at line {line}")]
    Invalid {
        path: PathBuf,
        line: u32,
        problem: Problem,
    },
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
    let document = document::parse(&text).map_err(|error| ConfigError::Malformed {
        path: path(),
        error,
    })?;
    read(Table::top(&document)).map_err(|problem| ConfigError::Invalid {
        path: path(),
        line: document::position(&text, problem.span().start).line,
        problem: problem.into_inner(),
    })
}

/// What a well-formed config declares, once it's checked, or its first problem and where it is.
fn read(top: Table<'_, '_>) -> Result<Config, Spanned<Problem>> {
    let top = top.taking(&["websites", "lengths", "night_shift", "rules"])?;
    let websites = top.boolean("websites")?.map(Spanned::into_inner);
    let rules = rules(top.table("rules")?)?;
    let night_shift = night_shift(top.table("night_shift")?)?;
    let lengths = match top.strings("lengths")? {
        None => default_lengths(),
        Some(lengths) => {
            let read = lengths
                .get_ref()
                .iter()
                .map(|text| length(text.get_ref()))
                .collect::<Option<Vec<_>>>()
                .filter(|read| !read.is_empty());
            let span = lengths.span();
            read.ok_or_else(|| {
                Spanned::new(
                    span,
                    Problem::NotLengths {
                        lengths: lengths
                            .into_inner()
                            .into_iter()
                            .map(Spanned::into_inner)
                            .collect(),
                    },
                )
            })?
        }
    };
    Ok(Config {
        rules,
        websites,
        night_shift,
        lengths,
        found: true,
    })
}

/// The rules `[rules.off]` and `[rules.on]` declare, each with what it says (ADR-0006).
fn rules(table: Option<Table<'_, '_>>) -> Result<Vec<(RuleIdentity, OnOff)>, Spanned<Problem>> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let table = table.taking(&["off", "on"])?;
    let mut rules: Vec<(RuleIdentity, OnOff)> = Vec::new();
    for (says, key) in [(OnOff::Off, "off"), (OnOff::On, "on")] {
        let Some(lists) = table.table(key)? else {
            continue;
        };
        for identity in identities(&lists.taking(&["app", "domain"])?)? {
            let at = identity.span();
            let identity = identity.into_inner();
            if let Some((other, _)) = rules
                .iter()
                .find(|(other, said)| *other == identity && *said != says)
            {
                return Err(Spanned::new(
                    at,
                    Problem::BothWays {
                        identity: other.clone(),
                    },
                ));
            }
            rules.push((identity, says));
        }
    }
    Ok(rules)
}

/// The rule identities a table's `app` and `domain` lists name, apps first, each with where it
/// is, or the first entry that names none.
pub(crate) fn identities(
    table: &Table<'_, '_>,
) -> Result<Vec<Spanned<RuleIdentity>>, Spanned<Problem>> {
    let entries = |key, identity: fn(&str) -> Result<RuleIdentity, Problem>| {
        Ok::<_, Spanned<Problem>>(
            table
                .strings(key)?
                .into_iter()
                .flat_map(Spanned::into_inner)
                .map(move |entry| {
                    let at = entry.span();
                    identity(entry.get_ref())
                        .map(|identity| Spanned::new(at.clone(), identity))
                        .map_err(|problem| Spanned::new(at, problem))
                }),
        )
    };
    entries("app", app_entry)?
        .chain(entries("domain", domain_entry)?)
        .collect()
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

/// The strength and schedule `[night_shift]` sets (ADR-0014).
fn night_shift(table: Option<Table<'_, '_>>) -> Result<NightShiftConfig, Spanned<Problem>> {
    let Some(table) = table else {
        return Ok(NightShiftConfig::default());
    };
    let table = table.taking(&["strength", "schedule"])?;
    let strength = table
        .integer("strength")?
        .map(|strength| {
            let value = *strength.get_ref();
            u8::try_from(value)
                .ok()
                .filter(|value| *value <= 100)
                .ok_or_else(|| {
                    Spanned::new(
                        strength.span(),
                        Problem::StrengthOutOfRange { strength: value },
                    )
                })
        })
        .transpose()?;
    let schedule = table
        .string("schedule")?
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
    Ok(NightShiftConfig { strength, schedule })
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::storage::file::{Memory, Paths};

    /// Why the config can't be used, with `text` in it.
    fn error(text: &str) -> ConfigError {
        let files = Memory::new(Paths::in_folder(Path::new("/nightcap")));
        files.put(file::File::Config, Some(text.to_owned()));
        load(&files).err().unwrap()
    }

    #[test]
    fn why_the_config_cant_be_used_reads_for_the_log_without_what_the_file_holds() {
        let logged = [
            "[rules.off]\ndomain = [\"secret.example\"\n",
            "[rules.off]\ndomain = \"secret.example\"\n",
            "[rules.off]\n\"secret.example\" = true\n",
            "[rules.off]\ndomain = [\"https://secret.example\"]\n",
        ]
        .map(|text| error(text).to_string());

        assert!(
            logged[0].starts_with("the config isn't valid TOML at line 2, column 27: "),
            "{}",
            logged[0]
        );
        assert_eq!(
            logged[1..],
            [
                "the config is invalid at line 2",
                "the config is invalid at line 2",
                "the config is invalid at line 2",
            ]
        );
        assert!(
            logged.iter().all(|text| !text.contains("secret")),
            "{logged:?}"
        );
    }

    #[test]
    fn a_domain_typed_as_a_key_reaches_the_clients_as_the_key_the_table_doesnt_take() {
        let ConfigError::Invalid { line, problem, .. } =
            error("[rules.off]\n\"secret.example\" = true\n")
        else {
            panic!("expected the config invalid");
        };

        assert_eq!(line, 2);
        assert_eq!(
            problem,
            Problem::UnknownKey {
                table: vec!["rules".to_owned(), "off".to_owned()],
                key: "secret.example".to_owned(),
                expected: vec!["app".to_owned(), "domain".to_owned()],
            }
        );
    }
}
