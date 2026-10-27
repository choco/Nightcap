//! `rules.toml`: the ad-hoc rules, kept by the core until they graduate into the config
//! (ADR-0007). Only the core writes it. It has the config's rule tables plus a version, and
//! readers tolerate unknown keys, so only a breaking change bumps the version.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use nightcap_protocol::{ExportFormat, IdentityError, OnOff, RuleIdentity};
use serde::{Deserialize, Serialize};

use crate::config::Malformed;
use crate::file::{self, Files, Owned};

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
    Malformed { path: PathBuf, error: Malformed },
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
    files: Arc<dyn Files>,
}

impl Store {
    pub(crate) fn new(files: Arc<dyn Files>) -> Self {
        Self { files }
    }

    /// The ad-hoc rules and what each says, or why there are none to use: an invalid file, or
    /// one from a newer Nightcap, which must never be overwritten. A missing file means no
    /// rules.
    pub(crate) fn load(&self) -> Result<Vec<(RuleIdentity, OnOff)>, AdHocError> {
        let path = || self.files.path(file::File::Rules);
        let contents = match self.files.read(file::File::Rules) {
            Ok(Some(contents)) => contents,
            Ok(None) => return Ok(Vec::new()),
            Err(error) => {
                return Err(AdHocError::Unreadable {
                    path: path(),
                    error,
                });
            }
        };
        let file: File = toml::from_slice(&contents).map_err(|error| AdHocError::Malformed {
            path: path(),
            error: Malformed::new(&error, &contents),
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
        self.files.write(
            Owned::Rules,
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::file::{Memory, Paths};

    /// Files in memory, with `rules.toml` holding `text`.
    fn holding(text: Option<&str>) -> Arc<Memory> {
        let files = Arc::new(Memory::new(Paths::in_folder(Path::new("/nightcap"))));
        files.put(file::File::Rules, text.map(Vec::from));
        files
    }

    fn figma() -> RuleIdentity {
        RuleIdentity::App("com.figma.Desktop".parse().unwrap())
    }

    #[test]
    fn saved_ad_hoc_rules_load_back() {
        let store = Store::new(holding(None));
        let rules = vec![
            (figma(), OnOff::Off),
            (
                RuleIdentity::Domain("youtube.com".parse().unwrap()),
                OnOff::On,
            ),
        ];

        store.save(&rules).unwrap();

        assert_eq!(store.load().unwrap(), rules);
    }

    #[test]
    fn ad_hoc_rules_from_a_newer_build_are_refused() {
        let files = holding(Some(
            "version = 3\n[rules.off]\napp = [\"com.figma.Desktop\"]\n",
        ));

        assert!(matches!(
            Store::new(files).load(),
            Err(AdHocError::Newer { version: 3, .. })
        ));
    }

    #[test]
    fn ad_hoc_rules_with_keys_this_build_doesnt_know_still_load() {
        let files = holding(Some(
            "version = 1\nlabel = \"x\"\n[rules]\napp = [\"com.figma.Desktop\"]\nkeyword = []\n",
        ));

        assert_eq!(
            Store::new(files).load().unwrap(),
            vec![(figma(), OnOff::Off)]
        );
    }
}
