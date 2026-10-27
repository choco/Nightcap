//! Cores staged in memory, for tests and the dev build's snapshots: no thread of their own, no
//! socket and no disk. Each decides an input on the caller's thread before it returns. A scene is
//! the files' text and what happens after, and the core gets there by its own code. Release
//! builds have none of this, so the app there can't stage a core.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{Command, Decision, ErrorReply, Install, Instance, Request, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::file::{File, Memory, Paths};
use crate::server::react::{Input, react};
use crate::server::start::{Clock, Setup, install_of, instance_of, zone_of};
use crate::server::state::Inner;
use crate::server::{Published, Snapshot};
use crate::{Core, Fact, Outcome, Runs, UiCommand};

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
    /// The shell reports a fact.
    Fact(Fact),
    /// The menu-bar UI sends a command. One the core refuses fails the staging.
    Command(Command),
    /// The menu-bar UI sends a command of its own.
    UiCommand(UiCommand),
    /// The config changes to this text, or goes: a reload reads it.
    Config(Option<String>),
}

impl From<Fact> for Step {
    fn from(fact: Fact) -> Self {
        Self::Fact(fact)
    }
}

impl From<Command> for Step {
    fn from(command: Command) -> Self {
        Self::Command(command)
    }
}

impl From<UiCommand> for Step {
    fn from(command: UiCommand) -> Self {
        Self::UiCommand(command)
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ScenarioError {
    /// The scenario names an app or a time zone the core doesn't know.
    #[error("{message}")]
    Invalid { message: String },
    /// The core refused the command at this step, counted from 0.
    #[error("step {step} was refused: {message}")]
    Refused { step: u32, message: String },
}

#[uniffi::export]
impl Core {
    /// A core staged in memory from `scenario`, which answers nothing over a socket and
    /// touches no file. It decides each fact and command before `report` or `send` returns,
    /// and tells no one their outcomes. Dev builds only.
    #[uniffi::constructor]
    pub fn staged(scenario: Scenario) -> Result<Arc<Self>, ScenarioError> {
        Ok(scenario.stage()?.core)
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

    /// The scenario with `step` after its other steps: a fact, or a command from the menu-bar
    /// UI.
    pub fn then(mut self, step: impl Into<Step>) -> Self {
        self.steps.push(step.into());
        self
    }

    /// Starts the scenario's core on files in memory and replays its steps, with their
    /// outcomes dropped.
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
                Step::Fact(fact) => _ = staged.report(fact),
                Step::Command(command) => {
                    if let (_, Err(refusal)) = staged.send(command) {
                        return Err(ScenarioError::Refused {
                            step,
                            message: refusal.message,
                        });
                    }
                }
                Step::UiCommand(command) => _ = staged.send_ui(command),
                Step::Config(text) => staged.write_config(text.as_deref()),
            }
        }
        Ok(staged)
    }
}

/// A staged core with what it runs on in reach: its files and its clock. The crate's
/// integration tests drive it, and read each input's outcome, as `react` decides it.
#[derive(Debug)]
pub struct Staged {
    core: Arc<Core>,
    inline: Arc<Inline>,
    files: Arc<Memory>,
    clock: Arc<ManualClock>,
    instance: Instance,
    install: Install,
    zone: TimeZone,
}

/// What a staged core decides on, and the snapshot its reads answer from.
pub(crate) struct Inline {
    inner: Mutex<Inner>,
    published: Arc<Published>,
}

impl fmt::Debug for Inline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Inline").finish_non_exhaustive()
    }
}

impl Inline {
    /// Runs `decide` on the staged core, then publishes what it left, as the core's thread does
    /// after each input.
    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    pub(crate) fn decide<T>(&self, decide: impl FnOnce(&mut Inner) -> T) -> T {
        let mut inner = self.inner.lock().expect("no panic happens while it's held");
        let answer = decide(&mut inner);
        self.published.publish(Snapshot::of(&inner));
        answer
    }
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
        let inner = Inner::start(Setup {
            instance,
            install,
            socket: None,
            files: files.clone(),
            clock: clock.clone(),
            zone: zone.clone(),
        });
        let published = Arc::new(Published::new(Snapshot::of(&inner)));
        let inline = Arc::new(Inline {
            inner: Mutex::new(inner),
            published: published.clone(),
        });
        Self {
            core: Arc::new(Core {
                published,
                runs: Runs::Inline(inline.clone()),
            }),
            inline,
            files,
            clock,
            instance,
            install,
            zone,
        }
    }

    /// The core, as the shell reaches it.
    pub fn core(&self) -> &Core {
        &self.core
    }

    /// Reports a fact as the shell does, and returns its outcome.
    pub fn report(&self, fact: Fact) -> Outcome {
        self.inline.decide(|inner| react(inner, Input::Fact(fact)))
    }

    /// Sends a command as the menu-bar UI does, and returns its outcome, which tells of a
    /// refusal as an event, and what the core answers.
    pub fn send(&self, command: Command) -> (Outcome, Result<Decision, ErrorReply>) {
        self.inline.decide(|inner| inner.choose(command))
    }

    /// Sends a command only the menu-bar UI has, and returns its outcome.
    pub fn send_ui(&self, command: UiCommand) -> Outcome {
        self.inline
            .decide(|inner| react(inner, Input::UiCommand(command)))
    }

    /// Quits as the shell does, and returns what it must carry out before it exits.
    pub fn stop(&self) -> Outcome {
        self.core.stop()
    }

    /// Sends a request as the CLI does, and returns its outcome and the response, which
    /// crosses the wire format both ways, as over the socket. A query is answered from the
    /// snapshot, as the socket's thread answers it, and leads to nothing.
    pub fn request(&self, request: Request) -> (Outcome, Response) {
        let (outcome, response) = match over_the_wire(&request) {
            Request::Query(query) => (
                Outcome::default(),
                self.inline.published.latest().answer(&query),
            ),
            request => self.inline.decide(|inner| inner.answer(&request)),
        };
        (outcome, over_the_wire(&response))
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
