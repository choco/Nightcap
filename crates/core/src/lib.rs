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
mod rows;
mod schedule;
mod settings;
mod shell;
#[cfg(any(test, debug_assertions))]
mod staging;
mod start;
mod state;
mod tour;

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
use std::time::SystemTime;

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use nightcap_protocol::{
    BundleId, Command, Decision, ErrorCode, ErrorReply, ExportFormat, Install, Instance,
    NightShift, NotificationPermission, OnOff, Payload, Permissions, Query, Request, Response,
    Schedule, Status, WebsiteChecking,
};

use commands::refused;
use domain::Host;
#[cfg(not(any(test, debug_assertions)))]
use file::File;
use file::{Disk, Files, Paths};
use onboarding::ONBOARDING;
use rows::{name_in_front, until_text};
use schedule::{CheckedSchedule, SunTimes};
use start::{Clock, Setup, SystemClock, install_of, instance_of, zone_of};
use state::{Facts, State};

/// How far from a computed schedule transition a status callback can arrive and still be macOS
/// performing it: CoreBrightness's clock and the core's can differ slightly (ADR-0004).
const TRANSITION_TOLERANCE: SignedDuration = SignedDuration::from_mins(2);

/// How long after waking macOS may take to perform a schedule transition it missed during sleep.
/// A guess until measured.
const POST_WAKE_GRACE: SignedDuration = SignedDuration::from_mins(5);

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
    /// One turn of the core, for an event or a tick: it applies the schedule transitions and
    /// the ends that came due, lets `react` change the state, then rewatches, writes Night
    /// Shift, reschedules, announces the problems that appeared unless `aloud` is false, and
    /// saves.
    fn turn<T>(
        &self,
        aloud: bool,
        react: impl FnOnce(&mut State, &mut Vec<Effect>) -> T,
    ) -> (Vec<Effect>, T) {
        let mut state = self.state();
        self.catch_up(&mut state);
        let mut effects = Vec::new();
        let outcome = react(&mut state, &mut effects);
        self.rewatch(&mut state, &mut effects);
        self.enforce(&mut state, &mut effects);
        self.reschedule(&mut state, &mut effects);
        self.announce(&mut state, aloud, &mut effects);
        self.save(&mut state);
        (effects, outcome)
    }

    fn react(&self, state: &mut State, event: Event) -> Vec<Effect> {
        match event {
            Event::Launched => {
                let mut effects = Vec::new();
                // Updates install silently, so the first launch of a new version says what's new.
                // A first run has nothing to compare; one from before versions were kept has
                // finished onboarding.
                let version = env!("CARGO_PKG_VERSION");
                let updated = match &state.last_version {
                    Some(last) => last != version,
                    None => state.onboarding > 0,
                };
                if updated {
                    tracing::info!("updated to {version}");
                    effects.push(Effect::Notify {
                        title: format!("Nightcap updated to {version}"),
                        body: "Click to see what’s new.".to_owned(),
                        opens: Some(format!("https://nightcap.choco.me/changelog/#v{version}")),
                    });
                }
                state.last_version = Some(version.to_owned());
                if state.onboarding < ONBOARDING {
                    // The config can answer website checking and Night Shift; opening at login
                    // it can't.
                    let night_shift = state.onboarding_night_shift();
                    let night_shift_answered =
                        !night_shift.ask_strength && !night_shift.ask_schedule;
                    if state.facts.config_websites.is_some()
                        && night_shift_answered
                        && !state.opens_itself_at_login
                    {
                        state.onboarding = ONBOARDING;
                    } else {
                        effects.push(Effect::ShowOnboarding {
                            completed: state.onboarding,
                        });
                    }
                }
                if self.updates_itself() {
                    effects.push(Effect::StartUpdater);
                }
                state.apply_config_night_shift(&mut effects);
                effects
            }
            Event::AppActivated {
                bundle_id,
                name,
                is_browser,
            } => {
                tracing::debug!("frontmost app: {bundle_id:?}, browser: {is_browser}");
                // An app whose bundle ID no rule could name counts as having none.
                let bundle_id = bundle_id.and_then(|bundle_id| {
                    bundle_id
                        .parse()
                        .inspect_err(|error| tracing::debug!("{error}; it counts as having none"))
                        .ok()
                });
                state.facts.context.app = bundle_id;
                state.facts.context.app_name = name;
                state.is_browser = is_browser;
                Vec::new()
            }
            // The host itself is never logged (ADR-0009).
            Event::HostChanged { generation, host } => {
                match &mut state.watch {
                    Some(watch) if watch.generation == generation => {
                        watch.reported = true;
                        state.facts.context.host = host.as_deref().and_then(Host::new);
                        tracing::debug!("host reported for watch {generation}");
                    }
                    _ => tracing::debug!("dropped a host from stale watch {generation}"),
                }
                Vec::new()
            }
            Event::LoginItemChanged { item } => {
                tracing::info!("login item: {item:?}");
                state.login_item = Some(item);
                // An enabled item becomes the user's wish, however it got enabled.
                if item == LoginItem::Enabled {
                    state.open_at_login = true;
                }
                Vec::new()
            }
            Event::PermissionsChanged {
                accessibility,
                notifications,
            } => {
                tracing::info!(
                    "permissions: accessibility {accessibility}, notifications {notifications:?}"
                );
                state.accessibility = Some(accessibility);
                state.notifications = notifications;
                // Allowed notifications become the user's wish, however they got allowed.
                if notifications == NotificationPermission::Granted {
                    state.notifications_wanted = true;
                }
                Vec::new()
            }
            Event::StatusChanged {
                enabled,
                schedule,
                sun_schedule_set_aside,
            } => {
                if state.session == Session::Away {
                    tracing::debug!("ignored a status callback while dormant");
                    return Vec::new();
                }
                state.facts.night_shift = if enabled {
                    NightShift::On
                } else {
                    NightShift::Off
                };
                state.pending_write = None;
                let keep_on = std::mem::take(&mut state.keep_on_with_schedule_off);
                // A shell bug, logged rather than a panic, which would stop the app (ADR-0002).
                let checked = CheckedSchedule::try_from(schedule)
                    .inspect_err(|error| {
                        tracing::error!("the shell reported a schedule it can't have: {error}");
                    })
                    .ok();
                let previous = std::mem::replace(&mut state.facts.timeline.schedule, checked);
                state.schedule_set_aside = sun_schedule_set_aside && schedule == Schedule::Off;
                if !state.schedule_set_aside {
                    state.schedule_off_accepted = false;
                }
                if state.session == Session::Back {
                    // The shell's fresh read after returning: nobody could change Night Shift by
                    // hand while the session was away.
                    state.session = Session::Active;
                    Vec::new()
                } else if previous
                    .zip(checked)
                    .is_some_and(|(previous, checked)| previous != checked)
                {
                    // macOS applies an edited schedule at once, so the edit is a schedule
                    // transition and whatever it switched isn't an external change.
                    tracing::info!("schedule edited");
                    state.places.schedule_edited();
                    if keep_on && schedule == Schedule::Off && !enabled {
                        let now = self.clock.now();
                        state.places.switch(OnOff::On, &state.facts.world(now));
                    }
                    Vec::new()
                } else {
                    self.reconcile(state, enabled).into_iter().collect()
                }
            }
            Event::StrengthChanged { value } => {
                state.strength = Some(value);
                Vec::new()
            }
            Event::OnboardingFinished { websites } => {
                state.onboarding = ONBOARDING;
                if state.facts.config_websites.is_none() {
                    tracing::info!("onboarding finished; website checking {websites}");
                    state.facts.websites = websites;
                }
                Vec::new()
            }
            Event::TourStarted {
                sample,
                site,
                example,
            } => {
                let stands_in = state.facts.night_shift == NightShift::Off;
                if !state.places.start_tour(stands_in) {
                    tracing::warn!("a tour started while one runs; keeping the first");
                    return Vec::new();
                }
                state.tour = Some(tour::Tour::new(&sample, site.as_ref(), &example));
                tracing::info!(
                    "tour started{}",
                    if stands_in {
                        ", standing in for the schedule with Night Shift on"
                    } else {
                        ""
                    }
                );
                Vec::new()
            }
            Event::TourEnded => {
                let now = self.clock.now();
                state.places.end_tour(now);
                state.tour = None;
                Vec::new()
            }
            Event::FixChosen { fix } => {
                let mut effects = Vec::new();
                match fix {
                    Fix::StopCheckingWebsites => {
                        if let Err(error) =
                            self.run_command(state, &Command::Websites(OnOff::Off), &mut effects)
                        {
                            tracing::warn!("a fix was refused: {:?}", error.code);
                        }
                    }
                    Fix::KeepNotificationsOff => state.notifications_wanted = false,
                    Fix::KeepScheduleOff => state.schedule_off_accepted = true,
                    Fix::OpenAtLogin => effects.push(Effect::RegisterLoginItem),
                    Fix::DontOpenAtLogin => {
                        state.open_at_login = false;
                        // An item waiting for approval would still ask for it.
                        if state.login_item != Some(LoginItem::NotRegistered) {
                            effects.push(Effect::UnregisterLoginItem);
                        }
                    }
                    // An invalid config shows in its check again.
                    Fix::Reload => _ = self.reload(state, &mut effects),
                    Fix::AskForAccessibility
                    | Fix::AllowNotifications
                    | Fix::OpenLoginItems
                    | Fix::OpenLocationServices => {
                        tracing::debug!("{fix:?} is the shell's to carry out");
                    }
                }
                effects
            }
            // `handle` runs it through `choose`.
            Event::CommandChosen { .. } => Vec::new(),
            Event::TimerFired => {
                // The shell's timer is spent, whether or not its transition came due.
                state.wakeup = None;
                Vec::new()
            }
            Event::SunTimesChanged { sunrises, sunsets } => {
                let timestamps = |times: Vec<SystemTime>| {
                    times
                        .into_iter()
                        .map(Timestamp::try_from)
                        .collect::<Result<Vec<_>, _>>()
                };
                let sun_times = match (timestamps(sunrises), timestamps(sunsets)) {
                    (Ok(sunrises), Ok(sunsets)) => SunTimes::new(&sunrises, &sunsets),
                    // A shell bug, logged rather than a panic, which would stop the app
                    // (ADR-0002).
                    (Err(error), _) | (_, Err(error)) => {
                        tracing::error!("the shell reported a sun time out of range: {error}");
                        SunTimes::default()
                    }
                };
                state.facts.timeline.sun_times = sun_times;
                Vec::new()
            }
            Event::TimeZoneChanged { iana_id } => {
                match TimeZone::get(&iana_id) {
                    Ok(zone) => {
                        tracing::info!("time zone changed to {iana_id}");
                        state.facts.timeline.zone = zone;
                    }
                    Err(error) => {
                        tracing::error!("keeping the old time zone: {iana_id} is unknown: {error}");
                    }
                }
                Vec::new()
            }
            Event::SessionChanged { active } => {
                tracing::info!("session active: {active}");
                state.session = if active { Session::Back } else { Session::Away };
                Vec::new()
            }
            Event::Woke => {
                tracing::debug!("woke");
                let now = self.clock.now();
                if let Some(unreported) = &mut state.unreported {
                    unreported.until = now + POST_WAKE_GRACE;
                }
                Vec::new()
            }
            Event::Terminating => {
                // Nightcap can't tell a restart from being removed, so it never leaves a force
                // behind (ADR-0008), nor Night Shift on for a tour (ADR-0016).
                let now = self.clock.now();
                state.places.release(now);
                state.tour = None;
                if let Some(socket) = &self.socket
                    && let Err(error) = std::fs::remove_file(socket)
                {
                    tracing::warn!("couldn't remove {}: {error}", socket.display());
                }
                Vec::new()
            }
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("no panic happens while it's held")
    }

    fn answer(&self, state: &mut State, request: &Request, effects: &mut Vec<Effect>) -> Response {
        match request {
            Request::Command(command) => match self.run_command(state, command, effects) {
                Ok(decision) => Response::Ok(Payload::Decision(decision)),
                Err(error) => Response::Error(error),
            },
            Request::Query(Query::Status) => self.status(state),
            Request::Query(Query::Rules) => {
                let look = state.places.look(&state.facts.world(self.clock.now()));
                Response::Ok(Payload::Rules(
                    look.rules
                        .into_iter()
                        .map(|(rule, _)| rule.clone())
                        .collect(),
                ))
            }
            Request::Query(Query::Export(format)) => {
                match adhoc::export(&state.places.kept().ad_hoc, *format) {
                    Ok(text) => Response::Ok(Payload::Export(text)),
                    Err(error) => Response::Error(refused(
                        ErrorCode::Internal,
                        format!("couldn't write the rules as TOML: {error}"),
                    )),
                }
            }
            Request::Update => {
                if self.updates_itself() {
                    tracing::info!("checking for updates");
                    effects.push(Effect::CheckForUpdates);
                    return Response::Ok(Payload::UpdateCheckStarted);
                }
                Response::Error(ErrorReply {
                    code: ErrorCode::NoUpdater,
                    message: match self.instance {
                        Instance::Dev => "Dev builds don't update themselves; rebuild instead",
                        Instance::Release => {
                            "This install is managed by its installer, which keeps it up to date"
                        }
                    }
                    .to_owned(),
                })
            }
            Request::Reload => match self.reload(state, effects) {
                Ok(()) => self.status(state),
                Err(error) => Response::Error(error),
            },
        }
    }

    /// Reconciles a status callback with what the core wants (ADR-0004). macOS catching up on a
    /// transition the core already applied, or running just ahead of the next one, is that
    /// transition; a match needs nothing; anything else is an external change, handled like the
    /// equivalent command (ADR-0018).
    fn reconcile(&self, state: &mut State, enabled: bool) -> Option<Effect> {
        if state.left_by_last_run.take() == Some(OnOff::from(enabled)) {
            tracing::info!("Night Shift is where the last run left it");
            return None;
        }
        let now = self.clock.now();
        match state.unreported.take() {
            Some(unreported) if now <= unreported.until && enabled == unreported.scheduled => {
                tracing::info!(
                    "macOS performed the schedule transition at {}, {:#} late",
                    unreported.transition,
                    now.duration_since(unreported.transition)
                );
                return None;
            }
            Some(unreported) if now <= unreported.until => state.unreported = Some(unreported),
            _ => {}
        }
        let wanted = wanted(&state.places.look(&state.facts.world(now)).decision)?;
        if enabled == wanted {
            return None;
        }
        if let Some(transition) = state
            .facts
            .timeline
            .next_transition(state.facts.applied_until)
            && transition.duration_since(now) <= TRANSITION_TOLERANCE
            && state.facts.timeline.scheduled_state(transition) == Some(enabled)
        {
            tracing::info!(
                "schedule transition at {transition}, reported {:#} early",
                transition.duration_since(now)
            );
            state.places.expire(transition);
            state.facts.applied_until = transition;
            return None;
        }
        let observed = OnOff::from(enabled);
        tracing::info!("Night Shift turned {observed} outside Nightcap");
        let snooze = state.places.adopt(observed, &state.facts.world(now))?;
        Some(Effect::Notify {
            title: format!("Night Shift turned {} outside Nightcap", snooze.state),
            body: format!(
                "Snoozed the {} rule {}.",
                name_in_front(&snooze.rule, &state.facts.context),
                until_text(snooze.until, snooze.state, &state.facts.timeline.zone)
            ),
            opens: None,
        })
    }

    /// Applies every schedule transition that came due since the last event, and ends the
    /// timers and holds that ran out.
    fn catch_up(&self, state: &mut State) {
        let now = self.clock.now();
        if now <= state.facts.applied_until {
            return;
        }
        state.places.expire(now);
        if let Some(transition) = state
            .facts
            .timeline
            .next_transition(state.facts.applied_until)
            && transition <= now
        {
            tracing::info!("schedule transition at {transition}");
            state.unreported =
                state
                    .facts
                    .timeline
                    .scheduled_state(now)
                    .map(|scheduled| Unreported {
                        transition,
                        scheduled,
                        until: now + TRANSITION_TOLERANCE,
                    });
        }
        state.facts.applied_until = now;
    }

    /// Watches the frontmost browser while website checking is on and its host could change
    /// anything: not on the way out, nor while its own place says off and no site could say on
    /// (ADR-0008, ADR-0012, ADR-0018).
    fn rewatch(&self, state: &mut State, effects: &mut Vec<Effect>) {
        let host_matters = state
            .places
            .look(&state.facts.world(self.clock.now()))
            .host_matters;
        let wanted = state
            .facts
            .context
            .app
            .clone()
            .filter(|_| state.is_browser && state.facts.website_checking() && host_matters);
        match (&state.watch, wanted) {
            (Some(watch), Some(bundle_id)) if watch.bundle_id == bundle_id => {}
            (_, Some(bundle_id)) => {
                state.generation += 1;
                let generation = state.generation;
                state.facts.context.host = None;
                state.watch = Some(Watch {
                    bundle_id: bundle_id.clone(),
                    generation,
                    reported: false,
                });
                effects.push(Effect::WatchBrowser {
                    bundle_id: bundle_id.into(),
                    generation,
                });
            }
            (Some(_), None) => {
                state.facts.context.host = None;
                state.watch = None;
                effects.push(Effect::StopWatching);
            }
            (None, None) => {}
        }
    }

    /// Keeps the shell's one wakeup at the next schedule transition or the end of a timer or
    /// hold, whichever comes first. Sunset to sunrise with no transition ahead lacks the sun
    /// times, which CoreBrightness can take a minute or two after login to work out, so the
    /// wakeup comes at the next whole minute instead: the shell reads them again before each.
    fn reschedule(&self, state: &mut State, effects: &mut Vec<Effect>) {
        let now = self.clock.now();
        let transition = state
            .facts
            .timeline
            .next_transition(state.facts.applied_until);
        let awaits_sun_times = transition.is_none()
            && state.facts.timeline.schedule == Some(CheckedSchedule::SunsetToSunrise);
        state.sun_times_awaited_since =
            awaits_sun_times.then(|| state.sun_times_awaited_since.unwrap_or(now));
        #[expect(
            clippy::expect_used,
            reason = "rounding fails only beside jiff's limits, millennia from any clock"
        )]
        let retry = awaits_sun_times.then(|| {
            now.round(
                jiff::TimestampRound::new()
                    .smallest(jiff::Unit::Minute)
                    .mode(jiff::RoundMode::Trunc),
            )
            .expect("a minute rounds any time")
                + SignedDuration::from_mins(1)
        });
        let look = state.places.look(&state.facts.world(now));
        let ends = look
            .layers
            .iter()
            .flat_map(|layers| [&layers.hold, &layers.timer])
            .flatten()
            .filter_map(|layer| layer.until);
        let next = transition.or(retry).into_iter().chain(ends).min();
        if next != state.wakeup {
            state.wakeup = next;
            effects.push(match next {
                Some(at) => Effect::ScheduleWakeup { at: at.into() },
                None => Effect::CancelWakeup,
            });
        }
    }

    /// Writes Night Shift when what the core wants differs from what was last observed
    /// (ADR-0004), once until a status callback reports the result.
    fn enforce(&self, state: &mut State, effects: &mut Vec<Effect>) {
        if state.facts.night_shift == NightShift::Unavailable || state.session != Session::Active {
            return;
        }
        // A new watch keeps Night Shift as it is until its first read (ADR-0012).
        if state.watch.as_ref().is_some_and(|watch| !watch.reported) {
            return;
        }
        let look = state.places.look(&state.facts.world(self.clock.now()));
        let Some(wanted) = wanted(&look.decision) else {
            return;
        };
        if wanted != (state.facts.night_shift == NightShift::On)
            && state.pending_write != Some(wanted)
        {
            state.pending_write = Some(wanted);
            effects.push(Effect::SetNightShift { enabled: wanted });
        }
    }

    fn status(&self, state: &State) -> Response {
        let facts = &state.facts;
        let look = state.places.look(&facts.world(self.clock.now()));
        Response::Ok(Payload::Status(Box::new(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: self.install,
            night_shift: facts.night_shift,
            strength: state.strength,
            config_strength: state.config_night_shift.strength,
            decision: look.decision,
            schedule: facts.timeline.schedule.map(Schedule::from),
            config_schedule: state.config_night_shift.schedule,
            scheduled_state: facts
                .timeline
                .scheduled_state(facts.applied_until)
                .map(OnOff::from),
            next_transition: facts.timeline.next_transition(facts.applied_until),
            frontmost_app: facts.context.app.clone().map(String::from),
            watching: state.watch.is_some(),
            host: facts
                .context
                .host
                .as_ref()
                .map(|host| host.as_str().to_owned()),
            rules: look
                .rules
                .into_iter()
                .map(|(rule, _)| rule.clone())
                .collect(),
            config_error: state.config_error.as_ref().map(ToString::to_string),
            ad_hoc_error: state.ad_hoc_error.as_ref().map(ToString::to_string),
            website_checking: WebsiteChecking {
                state: OnOff::from(facts.website_checking()),
                from_config: facts.config_websites.is_some(),
            },
            layers: look.layers.to_vec(),
            permissions: Permissions {
                accessibility: state.accessibility == Some(true),
                notifications: state.notifications,
            },
            checks: state.checks(self.clock.now()),
        })))
    }
}

/// A schedule transition the core applied on its own clock that macOS hasn't reported
/// performing yet. Until `until`, a callback matching it is macOS catching up, not an external
/// change.
struct Unreported {
    transition: Timestamp,
    /// Whether the schedule has Night Shift on after the transition.
    scheduled: bool,
    until: Timestamp,
}

/// Whether Night Shift is this user's to change: with fast user switching, another user's
/// session can be in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Session {
    Active,
    /// Another user is in front: status callbacks aren't this user's, and nothing is written.
    Away,
    /// Back in front, waiting for the shell's fresh read of a status that went unwatched.
    Back,
}

/// The browser being watched, and whether its first read under this generation has arrived.
struct Watch {
    bundle_id: BundleId,
    generation: u64,
    reported: bool,
}

impl State {}

/// The Night Shift state a decision asks for, if it asks for one.
fn wanted(decision: &Decision) -> Option<bool> {
    decision.state.map(|state| state == OnOff::On)
}
