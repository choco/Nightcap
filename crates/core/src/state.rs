//! What the core believes about the system, and what of it survives restarts, in
//! `$XDG_STATE_HOME/<instance>/state.json` (ADR-0008). Readers tolerate unknown keys, so only a
//! breaking change bumps the version.

use std::io;
use std::sync::Arc;

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{CheckKind, NightShift, NotificationPermission, OnOff, PlaceLayers};
use serde::{Deserialize, Serialize};

use crate::config::NightShiftConfig;
use crate::file::{self, Files, Owned};
use crate::places::{Context, Places, World};
use crate::schedule::Timeline;
use crate::{Core, LoginItem, Session, Unreported, Watch, adhoc, config, tour, wanted};

const VERSION: u32 = 1;

/// What the core believes about the system, as last reported by the shell, plus what it
/// keeps across restarts.
pub(crate) struct State {
    pub(crate) facts: Facts,
    pub(crate) places: Places,
    pub(crate) strength: Option<u8>,
    pub(crate) config_error: Option<config::ConfigError>,
    /// Whether the config file existed when it was last loaded.
    pub(crate) config_found: bool,
    /// The menu's preset lengths, from the config or else 30 minutes, 1 hour and 2 hours.
    pub(crate) lengths: Vec<SignedDuration>,
    /// Why `rules.toml` can't be used; while set, ad-hoc rules are off and the file is left be.
    pub(crate) ad_hoc_error: Option<adhoc::AdHocError>,
    pub(crate) onboarding: u32,
    pub(crate) config_night_shift: NightShiftConfig,
    pub(crate) applied_from_config: NightShiftConfig,
    /// Nightcap switched the schedule off while Night Shift was on, and macOS switches it off
    /// with the schedule: the edit's callback turns it back on, as choosing "By hand" means.
    pub(crate) keep_on_with_schedule_off: bool,
    /// Whether the frontmost app is in the shell's browser list.
    pub(crate) is_browser: bool,
    pub(crate) watch: Option<Watch>,
    /// The generation of the last watch started.
    pub(crate) generation: u64,
    /// The write the shell was last asked for, until a status callback reports the result.
    pub(crate) pending_write: Option<bool>,
    pub(crate) unreported: Option<Unreported>,
    pub(crate) session: Session,
    /// Where the last run left Night Shift, which the first status callback will show.
    pub(crate) left_by_last_run: Option<OnOff>,
    /// The wakeup the shell was last asked for.
    pub(crate) wakeup: Option<Timestamp>,
    /// Since when sunset to sunrise has had no transition ahead, for want of sun times.
    pub(crate) sun_times_awaited_since: Option<Timestamp>,
    /// macOS has sunset to sunrise set aside, and the schedule is off.
    pub(crate) schedule_set_aside: bool,
    /// See `Persisted::schedule_off_accepted`.
    pub(crate) schedule_off_accepted: bool,
    /// None until the shell first reports it.
    pub(crate) accessibility: Option<bool>,
    pub(crate) notifications: NotificationPermission,
    /// See `Persisted::notifications_wanted`.
    pub(crate) notifications_wanted: bool,
    /// See `Persisted::announced`.
    pub(crate) announced: Vec<CheckKind>,
    /// None until the shell first reports it.
    pub(crate) login_item: Option<LoginItem>,
    /// See `Persisted::open_at_login`.
    pub(crate) open_at_login: bool,
    /// A self-managed release, which opens itself at login if the user wants (ADR-0011).
    pub(crate) opens_itself_at_login: bool,
    /// See `Persisted::last_version`.
    pub(crate) last_version: Option<String>,
    /// What the running tour's steps follow, while one runs (ADR-0016).
    pub(crate) tour: Option<tour::Tour>,
    /// What was last written to `state.json`.
    pub(crate) saved: Persisted,
}

/// What the place stack reads but doesn't own, lent to it as a `World` for each call.
pub(crate) struct Facts {
    pub(crate) night_shift: NightShift,
    pub(crate) timeline: Timeline,
    pub(crate) context: Context,
    /// Every schedule transition up to here has been applied.
    pub(crate) applied_until: Timestamp,
    /// Website checking as the user chose it; `websites` in the config wins (ADR-0012).
    pub(crate) websites: bool,
    pub(crate) config_websites: Option<bool>,
}

impl Facts {
    pub(crate) fn world(&self, now: Timestamp) -> World<'_> {
        World {
            context: &self.context,
            timeline: &self.timeline,
            applied_until: self.applied_until,
            now,
            observed: OnOff::from(self.night_shift == NightShift::On),
            website_checking: self.website_checking(),
        }
    }

    pub(crate) fn website_checking(&self) -> bool {
        self.config_websites.unwrap_or(self.websites)
    }
}

impl State {
    /// Mid-tour, the state the tour's end would leave, with Night Shift as the tour holds it: a
    /// run after a crash finds it there, as the last run left it, and puts it back (ADR-0016).
    fn snapshot(&self, now: Timestamp) -> Persisted {
        Persisted {
            layers: self.places.kept().layers.clone(),
            websites: self.facts.websites,
            onboarding: self.onboarding,
            notifications_wanted: self.notifications_wanted,
            open_at_login: self.open_at_login,
            announced: self.announced.clone(),
            night_shift: wanted(&self.places.look(&self.facts.world(now)).decision)
                .map(OnOff::from),
            until: self
                .facts
                .timeline
                .next_transition(self.facts.applied_until),
            applied_from_config: self.applied_from_config,
            schedule_off_accepted: self.schedule_off_accepted,
            last_version: self.last_version.clone(),
        }
    }
}

impl Core {
    /// Writes `state.json` whenever what it holds changes, so a crash loses nothing (ADR-0008).
    pub(crate) fn save(&self, state: &mut State) {
        let snapshot = state.snapshot(self.clock.now());
        if snapshot == state.saved {
            return;
        }
        match self.store.save(&snapshot) {
            Ok(()) => state.saved = snapshot,
            Err(error) => tracing::warn!("couldn't save the state: {error}"),
        }
    }
}

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
    use crate::file::{Memory, Paths};

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
