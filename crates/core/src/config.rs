//! `config.toml`: the config rules, declared by the user or their installer (ADR-0009). It is
//! strict, so a typo is an error rather than a rule that silently never matches.

use std::io::ErrorKind;
use std::path::Path;

use nightcap_protocol::{Rule, RuleIdentity, RuleSource};
use serde::Deserialize;

use crate::domain;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    /// Website checking, which Settings and the CLI then can't change (ADR-0016).
    websites: Option<bool>,
    #[serde(default)]
    rules: Rules,
}

pub(crate) struct Config {
    pub rules: Vec<Rule>,
    pub websites: Option<bool>,
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
        found: true,
    })
}
