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
pub use shell::{CoreError, Effect, Event, LoginItem, Waker, is_bundle_id, normalize_domain};
pub use tour::{
    MenuPath, TourApp, TourFacts, TourPlace, TourSite, TourSnapshot, TourTitles, tour_titles,
};

#[cfg(any(test, debug_assertions))]
pub use file::File;
#[cfg(any(test, debug_assertions))]
pub use staging::{Scenario, ScenarioError, Staged, Step};

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};

use nightcap_protocol::{ExportFormat, Install, Instance};

#[cfg(not(any(test, debug_assertions)))]
use file::File;
use file::{Disk, Files, Paths};
use start::{Clock, Setup, SystemClock, install_of, instance_of, zone_of};
use state::State;

#[derive(uniffi::Object)]
pub struct Core {
    instance: Instance,
    install: Install,
    socket: Option<PathBuf>,
    files: Arc<dyn Files>,
    store: state::Store,
    ad_hoc_store: adhoc::Store,
    clock: Arc<dyn Clock>,
    requests: Mutex<mpsc::Receiver<ipc::Pending>>,
    state: Mutex<State>,
}

impl fmt::Debug for Core {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Core")
            .field("instance", &self.instance)
            .field("install", &self.install)
            .field("socket", &self.socket)
            .finish_non_exhaustive()
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
        let core = Self::start(Setup {
            instance,
            install,
            socket: Some(socket),
            requests,
            files: Arc::new(files),
            clock: Arc::new(SystemClock),
            zone,
        });
        tracing::info!(
            "core {} listening, {install:?} install",
            env!("CARGO_PKG_VERSION")
        );
        Ok(Arc::new(core))
    }

    pub fn handle(&self, event: Event) -> Vec<Effect> {
        if let Event::CommandChosen { command } = &event {
            let (effects, outcome) = self.choose(command);
            if let Err(error) = outcome {
                // The message can name a domain, so only the code is logged (ADR-0009).
                tracing::warn!("a menu command was refused: {:?}", error.code);
            }
            return effects;
        }
        // The user is looking at whatever their own choice in Nightcap's UI got wrong.
        let aloud = !matches!(
            event,
            Event::FixChosen { .. } | Event::OnboardingFinished { .. }
        );
        let (effects, ()) = self.turn(aloud, |state, effects| {
            effects.extend(self.react(state, event));
        });
        effects
    }

    /// The ad-hoc rules, written for the config (ADR-0007).
    #[expect(
        clippy::expect_used,
        reason = "TOML refuses only shapes the rule lists never have, and the shell's call can't fail"
    )]
    pub fn export(&self, format: ExportFormat) -> String {
        adhoc::export(&self.state().places.kept().ad_hoc, format).expect("rules always serialize")
    }

    pub fn onboarding(&self) -> OnboardingSteps {
        OnboardingSteps::new(&self.state())
    }

    pub fn settings(&self) -> Settings {
        Settings::new(
            &self.state(),
            self.clock.now(),
            &self.files.path(File::Config),
        )
    }

    pub fn menu(&self) -> Menu {
        Menu::new(&self.state(), self.clock.now())
    }

    /// What the tour's steps follow and the titles they quote, or None while no tour runs.
    pub fn tour(&self) -> Option<TourSnapshot> {
        let state = self.state();
        let tour = state.tour.as_ref()?;
        Some(tour.report(&state.places.look(&state.facts.world(self.clock.now()))))
    }

    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    pub fn tick(&self) -> Vec<Effect> {
        let requests = self
            .requests
            .lock()
            .expect("no panic happens while it's held");
        // The CLI already printed what went wrong.
        let (effects, ()) = self.turn(false, |state, effects| {
            for pending in requests.try_iter() {
                let response = self.answer(state, &pending.request, effects);
                pending.reply(response);
            }
        });
        effects
    }
}

impl Core {
    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("no panic happens while it's held")
    }
}
