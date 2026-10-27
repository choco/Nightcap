//! `rules.toml`: the ad-hoc rules, kept by the core until they graduate into the config
//! (ADR-0007). Only the core writes it. It has the config's rule tables plus a version, and
//! readers tolerate unknown keys, so only a breaking change bumps the version.

use std::fs;
use std::io;
use std::path::PathBuf;

use nightcap_protocol::{ExportFormat, IdentityError, OnOff, RuleIdentity};
use serde::{Deserialize, Serialize};

use crate::file::write_whole_synced;

/// Version 2 lists rules by what they say (ADR-0006).
const VERSION: u32 = 2;

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(default)]
    rules: Rules,
}

#[derive(Default, Serialize, Deserialize)]
struct Rules {
    #[serde(default, skip_serializing_if = "RuleLists::is_empty")]
    off: RuleLists,
    #[serde(default, skip_serializing_if = "RuleLists::is_empty")]
    on: RuleLists,
    /// Version 1's lists, from before rules said off or on, when every rule was off.
    #[serde(default, skip_serializing)]
    app: Vec<String>,
    #[serde(default, skip_serializing)]
    domain: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct RuleLists {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    app: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    domain: Vec<String>,
}

impl RuleLists {
    fn is_empty(&self) -> bool {
        self.app.is_empty() && self.domain.is_empty()
    }
}

/// Why `rules.toml` can't be used, which turns ad-hoc rules off. What it says can quote the
/// file, and so a domain, so the log gets only its `kind` (ADR-0009).
#[derive(Debug, thiserror::Error)]
pub(crate) enum AdHocError {
    #[error("couldn't read {}: {error}", .path.display())]
    Unreadable { path: PathBuf, error: io::Error },
    #[error("{}: {error}", .path.display())]
    Malformed {
        path: PathBuf,
        error: toml::de::Error,
    },
    #[error("{} is version {version}, from a newer Nightcap than this one", .path.display())]
    Newer { path: PathBuf, version: u32 },
    #[error("{}: {error}", .path.display())]
    Invalid { path: PathBuf, error: IdentityError },
}

impl AdHocError {
    /// What went wrong, as the log may say it: never what the file holds.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Unreadable { .. } => "can't be read",
            Self::Malformed { .. } => "is malformed",
            Self::Newer { .. } => "is from a newer Nightcap",
            Self::Invalid { .. } => "is invalid",
        }
    }
}

pub(crate) struct Store {
    path: PathBuf,
}

impl Store {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The ad-hoc rules and what each says, or why there are none to use: an invalid file, or
    /// one from a newer Nightcap, which must never be overwritten. A missing file means no
    /// rules.
    pub(crate) fn load(&self) -> Result<Vec<(RuleIdentity, OnOff)>, AdHocError> {
        let path = || self.path.clone();
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(AdHocError::Unreadable {
                    path: path(),
                    error,
                });
            }
        };
        let file: File = toml::from_str(&text).map_err(|error| AdHocError::Malformed {
            path: path(),
            error,
        })?;
        if file.version > VERSION {
            return Err(AdHocError::Newer {
                path: path(),
                version: file.version,
            });
        }
        let Rules {
            off,
            on,
            app,
            domain,
        } = file.rules;
        let earlier = RuleLists { app, domain };
        let mut rules = Vec::new();
        for (says, lists) in [(OnOff::Off, earlier), (OnOff::Off, off), (OnOff::On, on)] {
            let apps = lists
                .app
                .iter()
                .map(|entry| entry.parse().map(RuleIdentity::App));
            let domains = lists
                .domain
                .iter()
                .map(|entry| entry.parse().map(RuleIdentity::Domain));
            for identity in apps.chain(domains) {
                let identity = identity.map_err(|error| AdHocError::Invalid {
                    path: path(),
                    error,
                })?;
                rules.push((identity, says));
            }
        }
        Ok(rules)
    }

    pub(crate) fn save(&self, rules: &[(RuleIdentity, OnOff)]) -> io::Result<()> {
        let file = File {
            version: VERSION,
            rules: Rules::from(rules),
        };
        let text = toml::to_string(&file).map_err(io::Error::other)?;
        write_whole_synced(
            &self.path,
            format!("# Written by Nightcap: change it with `nightcap rules`.\n{text}").as_bytes(),
        )
    }
}

/// The ad-hoc rules, ready to paste into the config of the given format. Bundle IDs and
/// domains never hold quotes or backslashes, so they need no escaping.
pub(crate) fn export(
    rules: &[(RuleIdentity, OnOff)],
    format: ExportFormat,
) -> Result<String, toml::ser::Error> {
    let rules = Rules::from(rules);
    Ok(match format {
        ExportFormat::Nix => {
            let list = |items: &[String]| {
                let quoted: Vec<_> = items.iter().map(|item| format!("\"{item}\" ")).collect();
                format!("[ {}]", quoted.concat())
            };
            let lists = |lists: &RuleLists| {
                format!(
                    "{{ app = {}; domain = {}; }}",
                    list(&lists.app),
                    list(&lists.domain)
                )
            };
            format!(
                "{{ off = {}; on = {}; }}\n",
                lists(&rules.off),
                lists(&rules.on)
            )
        }
        ExportFormat::Toml => {
            #[derive(Serialize)]
            struct Export<'a> {
                rules: &'a Rules,
            }
            toml::to_string(&Export { rules: &rules })?
        }
    })
}

impl From<&[(RuleIdentity, OnOff)]> for Rules {
    fn from(rules: &[(RuleIdentity, OnOff)]) -> Self {
        let mut lists = Self::default();
        for (identity, says) in rules {
            let list = match says {
                OnOff::Off => &mut lists.off,
                OnOff::On => &mut lists.on,
            };
            match identity {
                RuleIdentity::App(bundle_id) => list.app.push(bundle_id.to_string()),
                RuleIdentity::Domain(domain) => list.domain.push(domain.to_string()),
            }
        }
        lists
    }
}
