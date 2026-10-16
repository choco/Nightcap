//! `rules.toml`: the ad-hoc rules, kept by the core until they graduate into the config
//! (ADR-0009). Only the core writes it. It has the config's schema plus a version, and readers
//! tolerate unknown keys, so only a breaking change bumps the version.

use std::fs;
use std::io;
use std::path::PathBuf;

use nightcap_protocol::{ExportFormat, RuleIdentity};
use serde::{Deserialize, Serialize};

use crate::domain;

const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(default)]
    rules: Rules,
}

#[derive(Default, Serialize, Deserialize)]
struct Rules {
    #[serde(default)]
    app: Vec<String>,
    #[serde(default)]
    domain: Vec<String>,
}

pub(crate) struct Store {
    path: PathBuf,
}

impl Store {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The ad-hoc rules, or why there are none to use: an invalid file, or one from a newer
    /// Nightcap, which must never be overwritten. A missing file means no rules.
    pub(crate) fn load(&self) -> Result<Vec<RuleIdentity>, String> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("couldn't read {}: {error}", self.path.display())),
        };
        let file: File =
            toml::from_str(&text).map_err(|error| format!("{}: {error}", self.path.display()))?;
        if file.version > VERSION {
            return Err(format!(
                "{} is version {}, from a newer Nightcap than this one",
                self.path.display(),
                file.version
            ));
        }
        let domains = file
            .rules
            .domain
            .iter()
            .map(|entry| {
                domain::normalize(entry)
                    .ok_or_else(|| format!("{}: {entry:?} isn't a domain", self.path.display()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(file
            .rules
            .app
            .into_iter()
            .map(RuleIdentity::App)
            .chain(domains.into_iter().map(RuleIdentity::Domain))
            .collect())
    }

    /// Writes beside the file and renames over it, so a crash never leaves half a file.
    pub(crate) fn save(&self, rules: &[RuleIdentity]) -> io::Result<()> {
        let file = File {
            version: VERSION,
            rules: Rules::from(rules),
        };
        let text = toml::to_string(&file).map_err(io::Error::other)?;
        let folder = self.path.parent().expect("the rules file has a folder");
        fs::create_dir_all(folder)?;
        let partial = self.path.with_extension("toml.partial");
        fs::write(
            &partial,
            format!("# Written by Nightcap: change it with `nightcap rules`.\n{text}"),
        )?;
        fs::rename(&partial, &self.path)
    }
}

/// The ad-hoc rules, ready to paste into the config of the given format. Bundle IDs and
/// domains never hold quotes or backslashes, so they need no escaping.
pub(crate) fn export(rules: &[RuleIdentity], format: ExportFormat) -> String {
    let rules = Rules::from(rules);
    match format {
        ExportFormat::Nix => {
            let list = |items: &[String]| {
                let quoted: Vec<_> = items.iter().map(|item| format!("\"{item}\" ")).collect();
                format!("[ {}]", quoted.concat())
            };
            format!(
                "{{ app = {}; domain = {}; }}\n",
                list(&rules.app),
                list(&rules.domain)
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

impl From<&[RuleIdentity]> for Rules {
    fn from(identities: &[RuleIdentity]) -> Self {
        let mut rules = Self::default();
        for identity in identities {
            match identity {
                RuleIdentity::App(bundle_id) => rules.app.push(bundle_id.clone()),
                RuleIdentity::Domain(domain) => rules.domain.push(domain.clone()),
            }
        }
        rules
    }
}
