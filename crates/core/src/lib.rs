#![warn(clippy::expect_used, clippy::indexing_slicing, clippy::unreachable)]

uniffi::setup_scaffolding!();

mod adhoc;
mod checks;
mod commands;
mod config;
mod domain;
mod file;
mod ipc;
mod logging;
mod menu;
mod onboarding;
mod places;
mod react;
mod rows;
mod schedule;
mod settings;
mod shell;
#[cfg(any(test, debug_assertions))]
mod staging;
mod start;
mod state;
mod status;
mod tour;
mod turn;
mod watch;

pub use checks::{Fix, FixButton, ProblemLine, SettingsCheck};
pub use menu::{Menu, MenuEntry, MenuIcon, NightShiftItem};
pub use onboarding::{OnboardingNightShift, OnboardingSteps};
pub use rows::{MenuItem, PlaceRow, RowImage, RowItem};
pub use settings::{RuleAction, Settings, SettingsLayers, SettingsRule, SettingsSwitch};
pub use shell::{
    CoreError, Effect, Event, Fact, LoginItem, Outcome, UiCommand, Waker, is_bundle_id,
    normalize_domain,
};
pub use tour::{
    MenuPath, TourApp, TourFacts, TourPlace, TourSite, TourSnapshot, TourTitles, tour_titles,
};

#[cfg(any(test, debug_assertions))]
pub use file::File;
#[cfg(any(test, debug_assertions))]
pub use staging::{Scenario, ScenarioError, Staged, Step};

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};

use nightcap_protocol::{Command, ExportFormat};

#[cfg(not(any(test, debug_assertions)))]
use file::File;
use file::{Disk, Paths};
use react::{Input, react};
use start::{Setup, SystemClock, install_of, instance_of, zone_of};
use state::Inner;

#[derive(uniffi::Object)]
pub struct Core {
    requests: Mutex<mpsc::Receiver<ipc::Pending>>,
    inner: Mutex<Inner>,
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
    /// self-managed install. `time_zone` is the system's IANA time zone ID.
    #[uniffi::constructor]
    pub fn new(
        bundle_id: String,
        bundle_writable: bool,
        time_zone: String,
        waker: Arc<dyn Waker>,
    ) -> Result<Arc<Self>, CoreError> {
        let instance = instance_of(&bundle_id)?;
        logging::init(&bundle_id);
        let socket = instance.socket_path().map_err(|error| CoreError::Io {
            message: error.to_string(),
        })?;
        let files = Disk(Paths::xdg(instance)?);
        let zone = zone_of(&time_zone)?;
        let install = install_of(bundle_writable);
        // Before any file is read, so a second instance stops here.
        let requests = ipc::serve(&socket, waker)?;
        let inner = Inner::start(Setup {
            instance,
            install,
            socket: Some(socket),
            files: Arc::new(files),
            clock: Arc::new(SystemClock),
            zone,
        });
        tracing::info!(
            "core {} listening, {install:?} install",
            env!("CARGO_PKG_VERSION")
        );
        Ok(Arc::new(Self::serving(inner, requests)))
    }

    /// Something macOS observed.
    pub fn report(&self, fact: Fact) -> Outcome {
        react(&mut self.inner(), Input::Fact(fact))
    }

    /// A command from the menu-bar UI, which hears of a refusal as an event.
    pub fn send(&self, command: Command) -> Outcome {
        react(&mut self.inner(), Input::Command(command))
    }

    /// A command only the menu-bar UI sends.
    pub fn send_ui(&self, command: UiCommand) -> Outcome {
        react(&mut self.inner(), Input::UiCommand(command))
    }

    /// Releases every force as the app quits (ADR-0008), with what the shell must carry out
    /// before it exits.
    pub fn stop(&self) -> Outcome {
        react(&mut self.inner(), Input::Terminating)
    }

    /// The ad-hoc rules, written for the config (ADR-0007).
    #[expect(
        clippy::expect_used,
        reason = "TOML refuses only shapes the rule lists never have, and the shell's call can't fail"
    )]
    pub fn export(&self, format: ExportFormat) -> String {
        adhoc::export(&self.inner().places.kept().ad_hoc, format).expect("rules always serialize")
    }

    pub fn onboarding(&self) -> OnboardingSteps {
        OnboardingSteps::new(&self.inner())
    }

    pub fn settings(&self) -> Settings {
        let inner = self.inner();
        Settings::new(&inner, inner.clock.now(), &inner.files.path(File::Config))
    }

    pub fn menu(&self) -> Menu {
        let inner = self.inner();
        Menu::new(&inner, inner.clock.now())
    }

    /// What the tour's steps follow and the titles they quote, or None while no tour runs.
    pub fn tour(&self) -> Option<TourSnapshot> {
        let inner = self.inner();
        let tour = inner.tour.as_ref()?;
        Some(tour.report(&inner.places.look(&inner.facts.world(inner.clock.now()))))
    }

    /// Answers the CLI's requests waiting since the last tick, each in a turn of its own.
    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    pub fn tick(&self) -> Outcome {
        let requests = self
            .requests
            .lock()
            .expect("no panic happens while it's held");
        let mut inner = self.inner();
        let mut outcome = Outcome::default();
        for pending in requests.try_iter() {
            let (answered, response) = inner.answer(&pending.request);
            pending.reply(response);
            outcome.effects.extend(answered.effects);
            outcome.events.extend(answered.events);
        }
        outcome
    }
}

impl Core {
    /// A core deciding from `inner`, answering the requests the IPC thread queues on each tick.
    pub(crate) fn serving(inner: Inner, requests: mpsc::Receiver<ipc::Pending>) -> Self {
        Self {
            requests: Mutex::new(requests),
            inner: Mutex::new(inner),
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    pub(crate) fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().expect("no panic happens while it's held")
    }
}
