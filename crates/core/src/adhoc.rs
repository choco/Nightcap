//! `rules.toml`: the ad-hoc rules, kept by the core until they graduate into the config
//! (ADR-0007). Only the core writes it. It has the config's rule tables plus a version, and
//! readers tolerate unknown keys, so only a breaking change bumps the version.

use std::fs;
use std::io;
use std::path::PathBuf;

use nightcap_protocol::{ExportFormat, IdentityError, OnOff, RuleIdentity};
use serde::{Deserialize, Serialize};

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
    pub(crate) fn load(&self) -> Result<Vec<(RuleIdentity, OnOff)>, String> {
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
                let identity = identity
                    .map_err(|error: IdentityError| format!("{}: {error}", self.path.display()))?;
                rules.push((identity, says));
            }
        }
        Ok(rules)
    }

    /// Writes beside the file and renames over it, so a crash never leaves half a file.
    pub(crate) fn save(&self, rules: &[(RuleIdentity, OnOff)]) -> io::Result<()> {
        let file = File {
            version: VERSION,
            rules: Rules::from(rules),
        };
        let text = toml::to_string(&file).map_err(io::Error::other)?;
        let folder = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other(format!("{} has no folder", self.path.display())))?;
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
