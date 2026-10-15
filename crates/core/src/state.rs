//! Runtime state that survives restarts, in `$XDG_STATE_HOME/<instance>/state.json`
//! (ADR-0011). Readers tolerate unknown keys, so only a breaking change bumps the version.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Persisted {
    /// Set once the login item exists, so one the user later removes stays removed: macOS
    /// reports a removed item exactly like one that was never registered.
    pub(crate) login_item_registered: bool,
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(flatten)]
    state: Persisted,
}

pub(crate) struct Store {
    path: PathBuf,
}

impl Store {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// A missing, unreadable or newer file starts fresh: this state only carries intent
    /// that is cheap to lose.
    pub(crate) fn load(&self) -> Persisted {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Persisted::default(),
            Err(error) => {
                tracing::warn!("couldn't read {}: {error}", self.path.display());
                return Persisted::default();
            }
        };
        match serde_json::from_slice::<File>(&bytes) {
            Ok(file) if file.version <= VERSION => file.state,
            Ok(file) => {
                tracing::warn!(
                    "{} is version {}, newer than this build's {VERSION}; starting fresh",
                    self.path.display(),
                    file.version
                );
                Persisted::default()
            }
            Err(error) => {
                tracing::warn!("couldn't parse {}: {error}", self.path.display());
                Persisted::default()
            }
        }
    }

    /// Writes beside the file and renames over it, so a crash never leaves half a file.
    pub(crate) fn save(&self, state: &Persisted) -> io::Result<()> {
        let folder = self.path.parent().expect("the state file has a folder");
        fs::create_dir_all(folder)?;
        let file = File {
            version: VERSION,
            state: state.clone(),
        };
        let partial = self.path.with_extension("json.partial");
        fs::write(&partial, serde_json::to_vec_pretty(&file)?)?;
        fs::rename(&partial, &self.path)
    }
}
