//! `rules.toml`: the ad-hoc rules, kept by the core until they graduate into the config
//! (ADR-0007). Only the core writes it. It has the config's rule tables plus a version, and
//! readers tolerate unknown keys, so only a breaking change bumps the version.

use std::io;
use std::sync::Arc;

use nightcap_protocol::{ExportFormat, OnOff, Problem, RuleIdentity};
use serde::Serialize;
use toml::Spanned;

use crate::storage::config::identities;
use crate::storage::document::{self, Table, Unusable, Why};
use crate::storage::file::{self, Files, Owned};

/// Version 2 lists rules by what they say (ADR-0006).
const VERSION: u32 = 2;

/// The file as Nightcap writes it.
#[derive(Serialize)]
struct File {
    version: u32,
    rules: Rules,
}

#[derive(Default, Serialize)]
struct Rules {
    #[serde(skip_serializing_if = "RuleLists::is_empty")]
    off: RuleLists,
    #[serde(skip_serializing_if = "RuleLists::is_empty")]
    on: RuleLists,
}

#[derive(Default, Serialize)]
struct RuleLists {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    app: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    domain: Vec<String>,
}

impl RuleLists {
    fn is_empty(&self) -> bool {
        self.app.is_empty() && self.domain.is_empty()
    }
}

pub(crate) struct Store {
    files: Arc<dyn Files>,
}

impl Store {
    pub(crate) fn new(files: Arc<dyn Files>) -> Self {
        Self { files }
    }

    /// The ad-hoc rules and what each says, or why there are none to use, which turns ad-hoc
    /// rules off: an invalid file, or one from a newer Nightcap, which must never be
    /// overwritten. A missing file means no rules.
    pub(crate) fn load(&self) -> Result<Vec<(RuleIdentity, OnOff)>, Unusable> {
        let unusable = |why| Unusable {
            file: file::File::Rules,
            path: self.files.path(file::File::Rules),
            why,
        };
        let text = match self.files.read(file::File::Rules) {
            Ok(Some(text)) => text,
            Ok(None) => return Ok(Vec::new()),
            Err(error) => return Err(unusable(Why::Unreadable(error))),
        };
        let document = document::parse(&text).map_err(|error| unusable(Why::Malformed(error)))?;
        let invalid = |problem: Spanned<Problem>| unusable(Why::invalid(&text, problem));
        let top = Table::top(&document);
        let version = top
            .required("version", top.integer("version").map_err(invalid)?)
            .map_err(invalid)?
            .into_inner();
        if version > i64::from(VERSION) {
            return Err(unusable(Why::Newer { version }));
        }
        read(&top).map_err(invalid)
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

/// The rules `[rules.off]` and `[rules.on]` list, each with what it says (ADR-0006).
fn read(top: &Table<'_, '_>) -> Result<Vec<(RuleIdentity, OnOff)>, Spanned<Problem>> {
    let Some(rules) = top.table("rules")? else {
        return Ok(Vec::new());
    };
    let mut lists = Vec::new();
    for (says, key) in [(OnOff::Off, "off"), (OnOff::On, "on")] {
        if let Some(table) = rules.table(key)? {
            lists.push((says, identities(&table)?));
        }
    }
    Ok(lists
        .into_iter()
        .flat_map(|(says, identities)| {
            identities
                .into_iter()
                .map(move |identity| (identity.into_inner(), says))
        })
        .collect())
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
            toml::to_string(&Export { rules: &rules }).expect("rules always serialize")
        }
    }
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
            Err(Unusable {
                why: Why::Newer { version: 3 },
                ..
            })
        ));
    }

    #[test]
    fn ad_hoc_rules_with_keys_this_build_doesnt_know_still_load() {
        let files = holding(Some(
            "version = 2\nlabel = \"x\"\n[rules]\nkeyword = []\n[rules.off]\napp = [\"com.figma.Desktop\"]\nwindow = []\n",
        ));

        assert_eq!(
            Store::new(files).load().unwrap(),
            vec![(figma(), OnOff::Off)]
        );
    }
}
