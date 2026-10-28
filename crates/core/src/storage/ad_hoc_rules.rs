//! `rules.toml`: the ad-hoc rules, kept by the core until they graduate into the config
//! (ADR-0007). Only the core writes it. It has the config's rule tables plus a version, and
//! readers tolerate unknown keys, so only a breaking change bumps the version.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use nightcap_protocol::{ExportFormat, OnOff, Problem, RuleIdentity};
use serde::{Deserialize, Serialize};
use toml::Spanned;

use crate::storage::config::{Malformed, app_entry, domain_entry, line_at};
use crate::storage::file::{self, Files, Owned};

/// Version 2 lists rules by what they say (ADR-0006).
const VERSION: u32 = 2;

/// The file, with each entry as written: as Nightcap writes it, and as toml reads it, where each
/// keeps its place in the file, so an invalid one names its line. Its lists default to empty,
/// whatever their entries.
#[derive(Serialize, Deserialize)]
#[serde(bound(deserialize = "Entry: Deserialize<'de>"))]
struct File<Entry> {
    version: u32,
    #[serde(default)]
    rules: Rules<Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(bound(deserialize = "Entry: Deserialize<'de>"))]
struct Rules<Entry> {
    #[serde(default, skip_serializing_if = "RuleLists::is_empty")]
    off: RuleLists<Entry>,
    #[serde(default, skip_serializing_if = "RuleLists::is_empty")]
    on: RuleLists<Entry>,
    /// Version 1's lists, from before rules said off or on, when every rule was off.
    #[serde(default, skip_serializing)]
    app: Vec<Entry>,
    #[serde(default, skip_serializing)]
    domain: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(bound(deserialize = "Entry: Deserialize<'de>"))]
struct RuleLists<Entry> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    app: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    domain: Vec<Entry>,
}

impl<Entry> RuleLists<Entry> {
    fn is_empty(&self) -> bool {
        self.app.is_empty() && self.domain.is_empty()
    }
}

impl<Entry> Default for Rules<Entry> {
    fn default() -> Self {
        Self {
            off: RuleLists::default(),
            on: RuleLists::default(),
            app: Vec::new(),
            domain: Vec::new(),
        }
    }
}

impl<Entry> Default for RuleLists<Entry> {
    fn default() -> Self {
        Self {
            app: Vec::new(),
            domain: Vec::new(),
        }
    }
}

/// Why `rules.toml` can't be used, which turns ad-hoc rules off, and which each client words
/// from the checks. It reads as the log may say it: never what the file holds, which can quote a
/// domain (ADR-0009).
#[derive(Debug, thiserror::Error)]
pub(crate) enum AdHocError {
    #[error("rules.toml can't be read: {error}")]
    Unreadable { path: PathBuf, error: io::Error },
    #[error("rules.toml is malformed")]
    Malformed { path: PathBuf, error: Malformed },
    #[error("rules.toml is version {version}, from a newer Nightcap than this one")]
    Newer { path: PathBuf, version: u32 },
    #[error("rules.toml is invalid at line {line}")]
    Invalid {
        path: PathBuf,
        line: u32,
        problem: Problem,
    },
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
        let text = match self.files.read(file::File::Rules) {
            Ok(Some(text)) => text,
            Ok(None) => return Ok(Vec::new()),
            Err(error) => {
                return Err(AdHocError::Unreadable {
                    path: path(),
                    error,
                });
            }
        };
        let file: File<Spanned<String>> =
            toml::from_str(&text).map_err(|error| AdHocError::Malformed {
                path: path(),
                error: Malformed::new(&error, text.as_bytes()),
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
                .map(|entry| (entry, app_entry(entry.get_ref())));
            let domains = lists
                .domain
                .iter()
                .map(|entry| (entry, domain_entry(entry.get_ref())));
            for (entry, identity) in apps.chain(domains) {
                let identity = identity.map_err(|problem| AdHocError::Invalid {
                    path: path(),
                    line: line_at(text.as_bytes(), entry.span().start),
                    problem,
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
            &format!("# Written by Nightcap: change it with `nightcap rules`.\n{text}"),
        )
    }
}

/// The ad-hoc rules, ready to paste into the config of the given format. Bundle IDs and
/// domains never hold quotes or backslashes, so they need no escaping.
#[expect(
    clippy::expect_used,
    reason = "TOML refuses only shapes the rule lists never have"
)]
pub(crate) fn export(rules: &[(RuleIdentity, OnOff)], format: ExportFormat) -> String {
    let rules = Rules::from(rules);
    match format {
        ExportFormat::Nix => {
            let list = |items: &[String]| {
                let quoted: Vec<_> = items.iter().map(|item| format!("\"{item}\" ")).collect();
                format!("[ {}]", quoted.concat())
            };
            let lists = |lists: &RuleLists<String>| {
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
                rules: &'a Rules<String>,
            }
            toml::to_string(&Export { rules: &rules }).expect("rules always serialize")
        }
    }
}

impl From<&[(RuleIdentity, OnOff)]> for Rules<String> {
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
    use crate::storage::file::{Memory, Paths};

    /// Files in memory, with `rules.toml` holding `text`.
    fn holding(text: Option<&str>) -> Arc<Memory> {
        let files = Arc::new(Memory::new(Paths::in_folder(Path::new("/nightcap"))));
        files.put(file::File::Rules, text.map(str::to_owned));
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
