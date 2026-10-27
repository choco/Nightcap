#![warn(clippy::expect_used, clippy::indexing_slicing, clippy::unreachable)]

uniffi::setup_scaffolding!();

mod commands;
mod ffi;
mod ipc;
mod logging;
mod menu;
mod onboarding;
mod rows;
mod rules;
mod server;
mod settings;
mod setup;
#[cfg(any(test, debug_assertions))]
mod staging;
mod storage;
mod tour;

pub use ffi::{is_bundle_id, normalize_domain};
pub use menu::{Menu, MenuEntry, MenuIcon, NightShiftItem};
pub use onboarding::{OnboardingNightShift, OnboardingSteps};
pub use rows::{MenuItem, PlaceRow, RowImage, RowItem};
pub use server::messages::{Effect, Event, Fact, Listener, LoginItem, Outcome, UiCommand};
pub use settings::{RuleAction, Settings, SettingsLayers, SettingsRule, SettingsSwitch};
pub use setup::checks::{Fix, FixButton, ProblemLine, SettingsCheck};
pub use tour::{
    MenuPath, TourApp, TourFacts, TourPlace, TourSite, TourSnapshot, TourTitles, tour_titles,
};

#[cfg(any(test, debug_assertions))]
pub use staging::{Scenario, ScenarioError, Staged, Step};
#[cfg(any(test, debug_assertions))]
pub use storage::file::File;

use std::fmt;
use std::os::unix::net::UnixListener;
use std::sync::{Arc, mpsc};

use nightcap_protocol::{Command, ExportFormat};

use server::react::Input;
#[cfg(any(test, debug_assertions))]
use server::react::react;
use server::start::{Setup, SystemClock, install_of, instance_of, zone_of};
use server::state::Inner;
use server::{Mail, Mailbox, Published, Snapshot};
use storage::ad_hoc_rules;
use storage::file::{Disk, Paths};

/// The core, a server on its own thread (ADR-0002): facts and commands go into its mailbox and
/// return at once, its listener hears each one's outcome, and the reads answer from the snapshot
/// it publishes after each.
#[derive(uniffi::Object)]
pub struct Core {
    published: Arc<Published>,
    runs: Runs,
}

/// Where a core decides its inputs.
enum Runs {
    /// On the core's own thread, which takes them from its mailbox.
    Thread(Mailbox),
    /// As each arrives, on the caller's thread: a staged core.
    #[cfg(any(test, debug_assertions))]
    Inline(Arc<staging::Inline>),
}

impl fmt::Debug for Core {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Core").finish_non_exhaustive()
    }
}

#[uniffi::export]
impl Core {
    /// Starts the core for the app with this bundle ID, which picks the instance: its
    /// subsystem for logging and its socket. Fails if that instance is already running.
    /// `bundle_writable` says whether the user can replace the app bundle, which makes it a
    /// self-managed install. `time_zone` is the system's IANA time zone ID. `listener` hears
    /// the outcome of every fact and command, on the core's thread.
    #[uniffi::constructor]
    pub fn start(
        bundle_id: String,
        bundle_writable: bool,
        time_zone: String,
        listener: Arc<dyn Listener>,
    ) -> Result<Arc<Self>, CoreError> {
        let instance = instance_of(&bundle_id)?;
        logging::init(&bundle_id);
        let path = instance.socket_path().map_err(|error| CoreError::Io {
            message: error.to_string(),
        })?;
        let files = Disk(Paths::xdg(instance)?);
        let zone = zone_of(&time_zone)?;
        let install = install_of(bundle_writable);
        // Before any file is read, so a second instance stops here.
        let socket = ipc::bind(&path)?;
        let inner = Inner::start(Setup {
            instance,
            install,
            socket: Some(path),
            files: Arc::new(files),
            clock: Arc::new(SystemClock),
            zone,
        });
        let core = Self::run(inner, Some(socket), listener)?;
        tracing::info!(
            "core {} listening, {install:?} install",
            env!("CARGO_PKG_VERSION")
        );
        Ok(Arc::new(core))
    }

    /// Something macOS observed.
    pub fn report(&self, fact: Fact) {
        self.post(Input::Fact(fact));
    }

    /// A command from the menu-bar UI, which hears of a refusal as an event.
    pub fn send(&self, command: Command) {
        self.post(Input::Command(command));
    }

    /// A command only the menu-bar UI sends.
    pub fn send_ui(&self, command: UiCommand) {
        self.post(Input::UiCommand(command));
    }

    /// Releases every force as the app quits (ADR-0008), once everything before it is decided,
    /// and returns what the shell must carry out before it exits. The core decides nothing
    /// after.
    pub fn stop(&self) -> Outcome {
        match &self.runs {
            Runs::Thread(mailbox) => {
                let (reply, released) = mpsc::channel();
                match mailbox.post(Mail::Stop { reply }) {
                    Ok(()) => released.recv().unwrap_or_else(|_| {
                        tracing::error!("the core's thread ended before releasing every force");
                        Outcome::default()
                    }),
                    Err(error) => {
                        tracing::warn!("asked to stop again: {error}");
                        Outcome::default()
                    }
                }
            }
            #[cfg(any(test, debug_assertions))]
            Runs::Inline(inline) => inline.decide(|inner| react(inner, Input::Terminating)),
        }
    }

    /// The ad-hoc rules, written for the config (ADR-0007).
    #[expect(
        clippy::expect_used,
        reason = "TOML refuses only shapes the rule lists never have, and the shell's call can't fail"
    )]
    pub fn export(&self, format: ExportFormat) -> String {
        ad_hoc_rules::export(&self.published.latest().ad_hoc, format)
            .expect("rules always serialize")
    }

    pub fn onboarding(&self) -> OnboardingSteps {
        self.published.latest().onboarding.clone()
    }

    pub fn settings(&self) -> Settings {
        self.published.latest().settings.clone()
    }

    pub fn menu(&self) -> Menu {
        self.published.latest().menu.clone()
    }

    /// What the tour's steps follow and the titles they quote, or None while no tour runs.
    pub fn tour(&self) -> Option<TourSnapshot> {
        self.published.latest().tour.clone()
    }
}

impl Core {
    /// A core deciding from `inner` on its own thread, which tells `listener` each outcome, with
    /// `socket`'s thread putting the CLI's requests into its mailbox.
    pub(crate) fn run(
        inner: Inner,
        socket: Option<UnixListener>,
        listener: Arc<dyn Listener>,
    ) -> Result<Self, CoreError> {
        let published = Arc::new(Published::new(Snapshot::of(&inner)));
        let mailbox = server::spawn(inner, published.clone(), listener)?;
        if let Some(socket) = socket {
            ipc::serve(socket, mailbox.clone(), published.clone())?;
        }
        Ok(Self {
            published,
            runs: Runs::Thread(mailbox),
        })
    }

    fn post(&self, input: Input) {
        match &self.runs {
            Runs::Thread(mailbox) => {
                if let Err(error) = mailbox.post(Mail::Input(input)) {
                    tracing::warn!("dropped an input: {error}");
                }
            }
            #[cfg(any(test, debug_assertions))]
            Runs::Inline(inline) => _ = inline.decide(|inner| react(inner, input)),
        }
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{bundle_id} isn't a Nightcap bundle ID")]
    UnknownBundleId { bundle_id: String },
    #[error("another Nightcap is already running at {path}")]
    AlreadyRunning { path: String },
    #[error("{iana_id} isn't a time zone this Mac knows")]
    UnknownTimeZone { iana_id: String },
    #[error("{message}")]
    Io { message: String },
}
