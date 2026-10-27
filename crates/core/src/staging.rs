//! Cores staged in memory, for tests and the dev build's snapshots: no socket, no IPC thread
//! and no disk. A scene is the files' text and what happens after, and the core gets there by
//! its own code. Release builds have none of this, so the app there can't stage a core.

use std::sync::{Arc, Mutex, MutexGuard, mpsc};

use jiff::Timestamp;

use crate::file::{File, Memory, Paths};
use crate::ipc::Pending;
use crate::{Clock, Core, Event, Setup, install_of, instance_of, zone_of};

/// Where a staged core starts: the time and the time zone, the app it runs as, its files in
/// their own formats, then what happens, in order.
#[derive(Debug, uniffi::Record)]
pub struct Scenario {
    /// The time the core's clock reads, and stays at.
    pub now: Timestamp,
    /// The system's IANA time zone ID.
    #[uniffi(default = "Europe/Zurich")]
    pub time_zone: String,
    /// The app's bundle ID, which picks the instance.
    #[uniffi(default = "io.github.choco.nightcap")]
    pub bundle_id: String,
    /// Whether the user can replace the app bundle, which makes it a self-managed install.
    #[uniffi(default = false)]
    pub bundle_writable: bool,
    /// `config.toml`, or None for no such file.
    #[uniffi(default = None)]
    pub config: Option<String>,
    /// `rules.toml`, or None for no such file.
    #[uniffi(default = None)]
    pub rules: Option<String>,
    /// `state.json`, or None for no such file.
    #[uniffi(default = None)]
    pub state: Option<String>,
    /// What happens once the core has read its files, each a turn of its own.
    #[uniffi(default)]
    pub steps: Vec<Step>,
}

#[derive(Debug, uniffi::Enum)]
pub enum Step {
    /// The shell reports an event. A menu command the core refuses fails the staging.
    Event(Event),
    /// The config changes to this text, or goes: a reload reads it.
    Config(Option<String>),
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ScenarioError {
    /// The scenario names an app or a time zone the core doesn't know.
    #[error("{message}")]
    Invalid { message: String },
    /// The core refused the menu command at this step, counted from 0.
    #[error("step {step} was refused: {message}")]
    Refused { step: u32, message: String },
}

#[uniffi::export]
impl Core {
    /// A core staged in memory from `scenario`, which answers nothing over a socket and
    /// touches no file. Dev builds only.
    #[uniffi::constructor]
    pub fn staged(scenario: Scenario) -> Result<Arc<Self>, ScenarioError> {
        let (core, _, _) = scenario.stage(None, mpsc::channel().1)?;
        Ok(Arc::new(core))
    }
}

impl Scenario {
    /// Starts the scenario's core on `files`, or on files of its own, answering `requests` on
    /// each tick, and replays its steps with their effects dropped. The scenario's files'
    /// text replaces what `files` holds.
    pub(crate) fn stage(
        self,
        files: Option<Arc<Memory>>,
        requests: mpsc::Receiver<Pending>,
    ) -> Result<(Core, Arc<Memory>, Arc<ManualClock>), ScenarioError> {
        let invalid = |error: crate::CoreError| ScenarioError::Invalid {
            message: error.to_string(),
        };
        let instance = instance_of(&self.bundle_id).map_err(invalid)?;
        let zone = zone_of(&self.time_zone).map_err(invalid)?;
        let files = match files {
            Some(files) => files,
            None => Arc::new(Memory::new(Paths::xdg(instance).map_err(invalid)?)),
        };
        for (file, text) in [
            (File::Config, self.config),
            (File::Rules, self.rules),
            (File::State, self.state),
        ] {
            if let Some(text) = text {
                files.put(file, Some(text.into_bytes()));
            }
        }
        let clock = Arc::new(ManualClock::new(self.now));
        let core = Core::start(Setup {
            instance,
            install: install_of(self.bundle_writable),
            socket: None,
            requests,
            files: files.clone(),
            clock: clock.clone(),
            zone,
        });
        for (step, happening) in (0..).zip(self.steps) {
            match happening {
                Step::Event(Event::CommandChosen { command }) => {
                    if let (_, Err(refusal)) = core.choose(&command) {
                        return Err(ScenarioError::Refused {
                            step,
                            message: refusal.message,
                        });
                    }
                }
                Step::Event(event) => _ = core.handle(event),
                Step::Config(text) => files.put(File::Config, text.map(String::into_bytes)),
            }
        }
        Ok((core, files, clock))
    }
}

/// A clock that reads what it's set to.
#[derive(Debug)]
pub(crate) struct ManualClock(Mutex<Timestamp>);

impl ManualClock {
    pub(crate) fn new(now: Timestamp) -> Self {
        Self(Mutex::new(now))
    }

    #[cfg(test)]
    pub(crate) fn set(&self, now: Timestamp) {
        *self.time() = now;
    }

    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    fn time(&self) -> MutexGuard<'_, Timestamp> {
        self.0.lock().expect("no panic happens while it's held")
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        *self.time()
    }
}
