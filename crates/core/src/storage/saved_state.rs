//! What of the core's state survives restarts, in `$XDG_STATE_HOME/<instance>/state.json`
//! (ADR-0008). Readers tolerate unknown keys, so only a breaking change bumps the version.

use std::io;
use std::sync::Arc;

use jiff::Timestamp;
use nightcap_protocol::{CheckKind, OnOff, PlaceLayers};
use serde::{Deserialize, Serialize};

use crate::storage::config::NightShiftConfig;
use crate::storage::file::{self, Files, Owned};

const VERSION: u32 = 1;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Persisted {
    /// Every place's hold and timer, each with its end (ADR-0018).
    pub(crate) layers: Vec<PlaceLayers>,
    /// Website checking as last chosen in onboarding, Settings or the CLI (ADR-0012).
    pub(crate) websites: bool,
    /// The last onboarding version completed; a later one shows only its new steps.
    pub(crate) onboarding: u32,
    /// Whether the user wants Nightcap's notifications: set once they're allowed, and cleared
    /// only when the user accepts them off. The notifications check compares against it.
    pub(crate) notifications_wanted: bool,
    /// Whether the user wants Nightcap to open at login: set when they choose it or the login
    /// item turns up enabled, cleared only when they choose otherwise (ADR-0011).
    pub(crate) open_at_login: bool,
    /// The problems already announced with a notification, so a restart doesn't repeat them.
    pub(crate) announced: Vec<CheckKind>,
    /// What Nightcap was holding Night Shift at: forced off before a crash, or released on a
    /// clean exit. The next run finds Night Shift there and knows it wasn't the user.
    pub(crate) night_shift: Option<OnOff>,
    /// The schedule transition that ends `night_shift`, if one was coming: macOS applies its own
    /// schedule there.
    pub(crate) until: Option<Timestamp>,
    /// The strength and schedule last applied from the config: a config value applies again
    /// only once it differs (ADR-0014).
    pub(crate) applied_from_config: NightShiftConfig,
    /// The user accepted the schedule off while macOS has sunset to sunrise set aside, which
    /// the Night Shift check compares against. Cleared once it isn't set aside.
    pub(crate) schedule_off_accepted: bool,
    /// The version that last ran: the first launch of another is an update, which a
    /// notification announces.
    pub(crate) last_version: Option<String>,
}

impl Persisted {
    /// Drops what ended while Nightcap wasn't running.
    pub(crate) fn expire(&mut self, now: Timestamp) {
        if self.until.is_some_and(|until| until <= now) {
            self.night_shift = None;
            self.until = None;
        }
        for layers in &mut self.layers {
            for layer in [&mut layers.hold, &mut layers.timer] {
                if layer
                    .as_ref()
                    .is_some_and(|layer| layer.until.is_some_and(|until| until <= now))
                {
                    *layer = None;
                }
            }
        }
        self.layers
            .retain(|layers| layers.hold.is_some() || layers.timer.is_some());
    }
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(flatten)]
    state: Persisted,
}

pub(crate) struct Store {
    files: Arc<dyn Files>,
}

impl Store {
    pub(crate) fn new(files: Arc<dyn Files>) -> Self {
        Self { files }
    }

    /// A missing, unreadable or newer file starts fresh: this state only carries intent
    /// that is cheap to lose.
    pub(crate) fn load(&self) -> Persisted {
        let path = self.files.path(file::File::State);
        let bytes = match self.files.read(file::File::State) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Persisted::default(),
            Err(error) => {
                tracing::warn!("couldn't read {}: {error}", path.display());
                return Persisted::default();
            }
        };
        match serde_json::from_slice::<File>(&bytes) {
            Ok(file) if file.version <= VERSION => file.state,
            Ok(file) => {
                tracing::warn!(
                    "{} is version {}, newer than this build's {VERSION}; starting fresh",
                    path.display(),
                    file.version
                );
                Persisted::default()
            }
            Err(error) => {
                tracing::warn!("couldn't parse {}: {error}", path.display());
                Persisted::default()
            }
        }
    }

    pub(crate) fn save(&self, state: &Persisted) -> io::Result<()> {
        let file = File {
            version: VERSION,
            state: state.clone(),
        };
        self.files
            .write(Owned::State, &serde_json::to_vec_pretty(&file)?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use nightcap_protocol::{Layer, Place, Schedule, TimeOfDay};

    use super::*;
    use crate::storage::file::{Memory, Paths};

    /// Files in memory, with `state.json` holding `text`.
    fn holding(text: Option<&str>) -> Arc<Memory> {
        let files = Arc::new(Memory::new(Paths::in_folder(Path::new("/nightcap"))));
        files.put(file::File::State, text.map(Vec::from));
        files
    }

    fn at(instant: &str) -> Timestamp {
        instant.parse().unwrap()
    }

    #[test]
    fn saved_state_loads_back() {
        let store = Store::new(holding(None));
        let figma = Place::App("com.figma.Desktop".parse().unwrap());
        let linked = |with| Layer {
            state: OnOff::On,
            until: Some(at("2026-10-17T07:00:00+02:00")),
            with: Some(with),
        };
        let state = Persisted {
            layers: vec![
                PlaceLayers {
                    place: figma.clone(),
                    hold: Some(linked(Place::Everywhere)),
                    timer: None,
                },
                PlaceLayers {
                    place: Place::Everywhere,
                    hold: Some(linked(figma)),
                    timer: Some(Layer {
                        state: OnOff::Off,
                        until: Some(at("2026-10-17T00:00:00+02:00")),
                        with: None,
                    }),
                },
            ],
            websites: true,
            onboarding: 1,
            notifications_wanted: true,
            open_at_login: true,
            announced: vec![CheckKind::Accessibility],
            night_shift: Some(OnOff::Off),
            until: Some(at("2026-10-17T07:00:00+02:00")),
            applied_from_config: NightShiftConfig {
                strength: Some(40),
                schedule: Some(Schedule::Custom {
                    from: TimeOfDay {
                        hour: 22,
                        minute: 0,
                    },
                    to: TimeOfDay { hour: 7, minute: 0 },
                }),
            },
            schedule_off_accepted: true,
            last_version: Some("1.0.0".to_owned()),
        };

        store.save(&state).unwrap();

        assert_eq!(store.load(), state);
    }

    #[test]
    fn state_from_an_earlier_build_loads_without_its_manual_state_snoozes_or_timed_disable() {
        let files = holding(Some(
            r#"{"version":1,"manual_state":"off","snoozes":["app:com.figma.Desktop"],"timed_disable":"2026-10-16T22:00:00Z","websites":true,"onboarding":1}"#,
        ));

        let state = Store::new(files).load();

        assert_eq!(state.layers, Vec::new());
        assert!(state.websites);
        assert_eq!(state.onboarding, 1);
    }

    #[test]
    fn missing_state_starts_fresh() {
        assert_eq!(Store::new(holding(None)).load(), Persisted::default());
    }

    #[test]
    fn state_from_a_newer_build_starts_fresh() {
        let files = holding(Some(r#"{"version":2,"login_item_registered":true}"#));

        assert_eq!(Store::new(files).load(), Persisted::default());
    }

    #[test]
    fn state_with_keys_this_build_doesnt_know_still_loads() {
        let files = holding(Some(
            r#"{"version":1,"login_item_registered":true,"websites":true}"#,
        ));

        assert!(Store::new(files).load().websites);
    }
}
