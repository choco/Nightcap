//! Cores staged in memory, for tests and the dev build's snapshots: no socket, no IPC thread
//! and no disk. A scene is the files' text and what happens after, and the core gets there by
//! its own code. Release builds have none of this, so the app there can't stage a core.

use std::sync::{Arc, Mutex, MutexGuard, mpsc};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{Command, Decision, ErrorReply, Install, Instance, Request, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::file::{File, Memory, Paths};
use crate::ipc::Pending;
use crate::start::{Clock, Setup, install_of, instance_of, zone_of};
use crate::{Core, Effect, Event};

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
        Ok(Arc::new(scenario.stage()?.core))
    }
}

impl Scenario {
    /// A managed release in Zurich at `now`, with no files and nothing happening after.
    pub fn at(now: Timestamp) -> Self {
        Self {
            now,
            time_zone: "Europe/Zurich".to_owned(),
            bundle_id: "io.github.choco.nightcap".to_owned(),
            bundle_writable: false,
            config: None,
            rules: None,
            state: None,
            steps: Vec::new(),
        }
    }

    /// The scenario run as this instance, in this kind of install.
    pub fn installed(self, instance: Instance, install: Install) -> Self {
        Self {
            bundle_id: match instance {
                Instance::Release => "io.github.choco.nightcap",
                Instance::Dev => "io.github.choco.nightcap.dev",
            }
            .to_owned(),
            bundle_writable: install == Install::SelfManaged,
            ..self
        }
    }

    /// The scenario with `config.toml` holding `text`.
    pub fn config(self, text: impl Into<String>) -> Self {
        Self {
            config: Some(text.into()),
            ..self
        }
    }

    /// The scenario with `rules.toml` holding `text`.
    pub fn rules(self, text: impl Into<String>) -> Self {
        Self {
            rules: Some(text.into()),
            ..self
        }
    }

    /// The scenario with `state.json` holding `text`.
    pub fn state(self, text: impl Into<String>) -> Self {
        Self {
            state: Some(text.into()),
            ..self
        }
    }

    /// The scenario with the shell reporting `event` after its other steps.
    pub fn then(mut self, event: Event) -> Self {
        self.steps.push(Step::Event(event));
        self
    }

    /// Starts the scenario's core on files in memory and replays its steps, with their effects
    /// dropped.
    pub fn stage(self) -> Result<Staged, ScenarioError> {
        let invalid = |error: crate::CoreError| ScenarioError::Invalid {
            message: error.to_string(),
        };
        let instance = instance_of(&self.bundle_id).map_err(invalid)?;
        let zone = zone_of(&self.time_zone).map_err(invalid)?;
        let files = Arc::new(Memory::new(Paths::xdg(instance).map_err(invalid)?));
        for (file, text) in [
            (File::Config, self.config),
            (File::Rules, self.rules),
            (File::State, self.state),
        ] {
            if let Some(text) = text {
                files.put(file, Some(text.into_bytes()));
            }
        }
        let staged = Staged::start(
            instance,
            install_of(self.bundle_writable),
            zone,
            files,
            self.now,
        );
        for (step, happening) in (0..).zip(self.steps) {
            match happening {
                Step::Event(Event::CommandChosen { command }) => {
                    if let (_, Err(refusal)) = staged.choose(&command) {
                        return Err(ScenarioError::Refused {
                            step,
                            message: refusal.message,
                        });
                    }
                }
                Step::Event(event) => _ = staged.core.handle(event),
                Step::Config(text) => staged.write_config(text.as_deref()),
            }
        }
        Ok(staged)
    }
}

/// A staged core with what it runs on in reach: its files, its clock, and the requests the IPC
/// thread would queue. The crate's integration tests drive it.
#[derive(Debug)]
pub struct Staged {
    core: Core,
    files: Arc<Memory>,
    clock: Arc<ManualClock>,
    requests: mpsc::Sender<Pending>,
    instance: Instance,
    install: Install,
    zone: TimeZone,
}

impl Staged {
    fn start(
        instance: Instance,
        install: Install,
        zone: TimeZone,
        files: Arc<Memory>,
        now: Timestamp,
    ) -> Self {
        let clock = Arc::new(ManualClock::new(now));
        let (requests, receiver) = mpsc::channel();
        let core = Core::start(Setup {
            instance,
            install,
            socket: None,
            requests: receiver,
            files: files.clone(),
            clock: clock.clone(),
            zone: zone.clone(),
        });
        Self {
            core,
            files,
            clock,
            requests,
            instance,
            install,
            zone,
        }
    }

    /// The core, as the shell reaches it.
    pub fn core(&self) -> &Core {
        &self.core
    }

    /// Sends a request as the CLI does, and returns the tick's effects and the response. Both
    /// cross the wire format, as over the socket.
    #[expect(
        clippy::expect_used,
        reason = "the core holds the receiver, and its tick answers every request it finds"
    )]
    pub fn request(&self, request: Request) -> (Vec<Effect>, Response) {
        let (pending, response) = Pending::new(over_the_wire(&request));
        self.requests
            .send(pending)
            .expect("the core is there to receive it");
        let effects = self.core.tick();
        let response = response.try_recv().expect("the tick answers the request");
        (effects, over_the_wire(&response))
    }

    /// Runs a command as the menu does, with what the core answers.
    pub fn choose(&self, command: &Command) -> (Vec<Effect>, Result<Decision, ErrorReply>) {
        self.core.choose(command)
    }

    /// What `file` holds, or None where there's no such file.
    #[expect(
        clippy::expect_used,
        reason = "the scenario's files, the config's changes and the core's writes are all text"
    )]
    pub fn file(&self, file: File) -> Option<String> {
        self.files
            .get(file)
            .map(|contents| String::from_utf8(contents).expect("the files are UTF-8"))
    }

    /// Changes the config to `text`, or takes it away, as the user or their installer would.
    /// The core reads it on its next reload.
    pub fn write_config(&self, text: Option<&str>) {
        self.files
            .put(File::Config, text.map(|text| text.as_bytes().to_owned()));
    }

    /// Moves the core's clock to `now`.
    pub fn set_now(&self, now: Timestamp) {
        self.clock.set(now);
    }

    /// Starts a second core on this one's files, as the same app, as launchd does after a
    /// restart or a crash.
    pub fn restart(&self) -> Self {
        self.restart_at(self.clock.now())
    }

    /// As `restart`, with the clock at `now`.
    pub fn restart_at(&self, now: Timestamp) -> Self {
        Self::start(
            self.instance,
            self.install,
            self.zone.clone(),
            self.files.clone(),
            now,
        )
    }
}

/// `message` as the other end of the socket reads it.
#[expect(
    clippy::expect_used,
    reason = "the protocol's messages serialize, and read back as they were"
)]
fn over_the_wire<T: Serialize + DeserializeOwned>(message: &T) -> T {
    serde_json::to_vec(message)
        .and_then(|json| serde_json::from_slice(&json))
        .expect("a message crosses the wire format")
}

/// A clock that reads what it's set to.
#[derive(Debug)]
pub(crate) struct ManualClock(Mutex<Timestamp>);

impl ManualClock {
    pub(crate) fn new(now: Timestamp) -> Self {
        Self(Mutex::new(now))
    }

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
