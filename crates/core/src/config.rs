//! `config.toml`: the config rules, declared by the user or their installer (ADR-0009). It is
//! strict, so a typo is an error rather than a rule that silently never matches.

use std::io::ErrorKind;
use std::path::Path;

use nightcap_protocol::{Rule, RuleIdentity, RuleSource};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    rules: Rules,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rules {
    #[serde(default)]
    app: Vec<String>,
}

/// The config rules in `path`. A missing file means no rules, as on a fresh brew install.
pub(crate) fn load(path: &Path) -> Result<Vec<Rule>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("couldn't read {}: {error}", path.display())),
    };
    let file: File =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(file
        .rules
        .app
        .into_iter()
        .map(|bundle_id| Rule {
            identity: RuleIdentity::App(bundle_id),
            source: RuleSource::Config,
        })
        .collect())
}
