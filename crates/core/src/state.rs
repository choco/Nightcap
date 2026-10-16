//! Runtime state that survives restarts, in `$XDG_STATE_HOME/<instance>/state.json`
//! (ADR-0011). Readers tolerate unknown keys, so only a breaking change bumps the version.

use std::fs;
use std::io;
use std::path::PathBuf;

use jiff::Timestamp;
use nightcap_protocol::{CheckKind, OnOff, RuleIdentity};
use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Persisted {
    pub(crate) manual_state: Option<OnOff>,
    pub(crate) snoozes: Vec<RuleIdentity>,
    pub(crate) timed_disable: Option<Timestamp>,
    /// Website checking as last chosen in onboarding, the menu or the CLI (ADR-0016).
    pub(crate) websites: bool,
    /// The last onboarding version completed; a later one shows only its new steps.
    pub(crate) onboarding: u32,
    /// Whether the user wants Nightcap's notifications: set once they're allowed, and cleared
    /// only when the user accepts them off. The notifications check compares against it.
    pub(crate) notifications_wanted: bool,
    /// Whether the user wants Nightcap to open at login: set when they choose it or the login
    /// item turns up enabled, cleared only when they choose otherwise (ADR-0017).
    pub(crate) open_at_login: bool,
    /// The problems already announced with a notification, so a restart doesn't repeat them.
    pub(crate) announced: Vec<CheckKind>,
    /// What Nightcap was holding Night Shift at: forced off before a crash, or released on a
    /// clean exit. The next run finds Night Shift there and knows it wasn't the user.
    pub(crate) night_shift: Option<OnOff>,
    /// The schedule transition that ends the manual state, the snoozes and `night_shift`, if
    /// one was coming: macOS applies its own schedule there.
    pub(crate) until: Option<Timestamp>,
}

impl Persisted {
    /// Drops what ended while Nightcap wasn't running.
    pub(crate) fn expire(&mut self, now: Timestamp) {
        if self.until.is_some_and(|until| until <= now) {
            self.manual_state = None;
            self.snoozes.clear();
            self.night_shift = None;
            self.until = None;
        }
        if self.timed_disable.is_some_and(|until| until <= now) {
            self.timed_disable = None;
        }
    }
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
