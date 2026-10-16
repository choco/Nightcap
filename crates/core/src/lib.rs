uniffi::setup_scaffolding!();

mod config;
mod engine;
mod ipc;
mod logging;
mod schedule;
mod state;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::time::SystemTime;

use etcetera::BaseStrategy;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use nightcap_protocol::{
    Command, Decision, ErrorCode, ErrorReply, Install, Instance, NightShift,
    NotificationPermission, OnOff, Payload, Permissions, Query, Reason, Request, Response, Rule,
    RuleIdentity, Schedule, Status, TimeOfDay, Verdict,
};

use engine::{Context, Inputs};
use schedule::{SunTimes, Timeline};

#[uniffi::remote(Enum)]
pub enum NotificationPermission {
    NotRequested,
    Denied,
    Granted,
}

#[uniffi::remote(Enum)]
pub enum Schedule {
    Off,
    SunsetToSunrise,
    Custom { from: TimeOfDay, to: TimeOfDay },
}

#[uniffi::remote(Record)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

/// Lets the core's own threads ask the shell for a `tick()` on the main thread (ADR-0002).
#[uniffi::export(foreign)]
pub trait Waker: Send + Sync {
    fn wake(&self);
}

#[derive(Debug, uniffi::Enum)]
pub enum Event {
    Launched,
    /// An app came to the front; some processes have no bundle ID.
    AppActivated {
        bundle_id: Option<String>,
    },
    /// The login item exists, whether the shell just registered it or found it in place.
    LoginItemRegistered,
    PermissionsChanged {
        accessibility: bool,
        notifications: NotificationPermission,
    },
    /// Night Shift as CoreBrightness reports it. Custom hours must be valid times of day.
    StatusChanged {
        enabled: bool,
        schedule: Schedule,
    },
    /// Sunrises and sunsets from CoreBrightness, from the day before to the day after. The
    /// shell reports them at launch, on waking and before each `TimerFired`, so they never run
    /// out.
    SunTimesChanged {
        sunrises: Vec<SystemTime>,
        sunsets: Vec<SystemTime>,
    },
    /// The system time zone changed, to this IANA time zone ID.
    TimeZoneChanged {
        iana_id: String,
    },
    /// The wakeup the core last asked for came due.
    TimerFired,
    /// The Mac woke from sleep, so schedule transitions may have passed unseen.
    Woke,
    /// This user's login session became active or inactive, as with fast user switching.
    SessionChanged {
        active: bool,
    },
    Terminating,
}

#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum Effect {
    RegisterLoginItem,
    /// Starts Sparkle's daily checks, in self-managed releases only (ADR-0015).
    StartUpdater,
    /// Starts Sparkle's own check, as if the user had asked for one.
    CheckForUpdates,
    SetNightShift {
        enabled: bool,
    },
    /// Replaces any earlier wakeup; the shell answers with `TimerFired`.
    ScheduleWakeup {
        at: SystemTime,
    },
    CancelWakeup,
    Notify {
        title: String,
        body: String,
    },
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

/// How far from a computed schedule transition a status callback can arrive and still be macOS
/// performing it: CoreBrightness's clock and the core's can differ slightly (ADR-0005).
const TRANSITION_TOLERANCE: SignedDuration = SignedDuration::from_mins(2);

/// How long after waking macOS may take to perform a schedule transition it missed during sleep.
/// A guess until measured.
const POST_WAKE_GRACE: SignedDuration = SignedDuration::from_mins(5);

/// Where the core reads the time, so that tests control it completely.
trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

#[derive(uniffi::Object)]
pub struct Core {
    instance: Instance,
    install: Install,
    socket: PathBuf,
    store: state::Store,
    config_file: PathBuf,
    clock: Arc<dyn Clock>,
    requests: Mutex<mpsc::Receiver<ipc::Pending>>,
    state: Mutex<State>,
}

/// Where a core lives and what kind of install it serves.
struct Setup {
    instance: Instance,
    install: Install,
    socket: PathBuf,
    state_file: PathBuf,
    config_file: PathBuf,
    clock: Arc<dyn Clock>,
    zone: TimeZone,
}

/// What the core believes about the system, as last reported by the shell, plus what it
/// keeps across restarts.
struct State {
    night_shift: NightShift,
    manual_state: Option<OnOff>,
    timeline: Timeline,
    context: Context,
    rules: Vec<Rule>,
    config_error: Option<String>,
    /// Rules muted until the next schedule transition.
    snoozes: Vec<RuleIdentity>,
    /// When the timed disable ends, if one is running.
    timed_disable: Option<Timestamp>,
    /// The write the shell was last asked for, until a status callback reports the result.
    pending_write: Option<bool>,
    unreported: Option<Unreported>,
    /// Set on the way out: forces no longer apply.
    released: bool,
    session: Session,
    /// Where the last run left Night Shift, which the first status callback will show.
    left_by_last_run: Option<OnOff>,
    /// Every schedule transition up to here has been applied.
    applied_until: Timestamp,
    /// The wakeup the shell was last asked for.
    wakeup: Option<Timestamp>,
    accessibility: bool,
    notifications: NotificationPermission,
    login_item_registered: bool,
    /// What was last written to `state.json`.
    saved: state::Persisted,
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
        let instance =
            Instance::from_bundle_id(&bundle_id).ok_or_else(|| CoreError::UnknownBundleId {
                bundle_id: bundle_id.clone(),
            })?;
        logging::init(&bundle_id);
        let socket = instance.socket_path().map_err(|error| CoreError::Io {
            message: error.to_string(),
        })?;
        let missing = |what: &str| CoreError::Io {
            message: format!("can't find the XDG {what} directory"),
        };
        let xdg = etcetera::base_strategy::Xdg::new().map_err(|_| missing("base"))?;
        let state_folder = xdg.state_dir().ok_or_else(|| missing("state"))?;
        let zone = TimeZone::get(&time_zone)
            .map_err(|_| CoreError::UnknownTimeZone { iana_id: time_zone })?;
        let install = if bundle_writable {
            Install::SelfManaged
        } else {
            Install::Managed
        };
        let core = Self::start(
            Setup {
                instance,
                install,
                socket,
                state_file: state_folder
                    .join(instance.xdg_folder_name())
                    .join("state.json"),
                config_file: xdg
                    .config_dir()
                    .join(instance.xdg_folder_name())
                    .join("config.toml"),
                clock: Arc::new(SystemClock),
                zone,
            },
            waker,
        )?;
        tracing::info!(
            "core {} listening, {install:?} install",
            env!("CARGO_PKG_VERSION")
        );
        Ok(Arc::new(core))
    }

    pub fn handle(&self, event: Event) -> Vec<Effect> {
        self.catch_up();
        let mut effects = self.react(event);
        self.enforce(&mut effects);
        self.reschedule(&mut effects);
        self.save();
        effects
    }

    pub fn tick(&self) -> Vec<Effect> {
        let requests = self
            .requests
            .lock()
            .expect("no panic happens while it's held");
        self.catch_up();
        let mut effects = Vec::new();
        for pending in requests.try_iter() {
            let response = self.answer(&pending.request, &mut effects);
            pending.reply(response);
        }
        self.enforce(&mut effects);
        self.reschedule(&mut effects);
        self.save();
        effects
    }
}

impl Core {
    fn react(&self, event: Event) -> Vec<Effect> {
        match event {
            Event::Launched => {
                let state = self.state();
                let mut effects = Vec::new();
                if self.updates_itself() {
                    if !state.login_item_registered {
                        effects.push(Effect::RegisterLoginItem);
                    }
                    effects.push(Effect::StartUpdater);
                }
                if let Some(error) = &state.config_error {
                    effects.push(Effect::Notify {
                        title: "Nightcap's config is invalid".to_owned(),
                        body: format!("Running without its rules. {error}"),
                    });
                }
                effects
            }
            Event::AppActivated { bundle_id } => {
                tracing::debug!("frontmost app: {bundle_id:?}");
                self.state().context.app = bundle_id;
                Vec::new()
            }
            Event::LoginItemRegistered => {
                self.state().login_item_registered = true;
                Vec::new()
            }
            Event::PermissionsChanged {
                accessibility,
                notifications,
            } => {
                tracing::info!(
                    "permissions: accessibility {accessibility}, notifications {notifications:?}"
                );
                let mut state = self.state();
                state.accessibility = accessibility;
                state.notifications = notifications;
                Vec::new()
            }
            Event::StatusChanged { enabled, schedule } => {
                let mut state = self.state();
                if state.session == Session::Away {
                    tracing::debug!("ignored a status callback while dormant");
                    return Vec::new();
                }
                state.night_shift = if enabled {
                    NightShift::On
                } else {
                    NightShift::Off
                };
                state.pending_write = None;
                let previous = state.timeline.schedule.replace(schedule);
                if state.session == Session::Back {
                    // The shell's fresh read after returning: nobody could change Night Shift by
                    // hand while the session was away.
                    state.session = Session::Active;
                    Vec::new()
                } else if previous.is_some_and(|previous| previous != schedule) {
                    // macOS applies an edited schedule at once, so the edit is a schedule
                    // transition and whatever it switched isn't an external change.
                    tracing::info!("schedule edited");
                    state.pass_transition();
                    Vec::new()
                } else {
                    self.reconcile(&mut state, enabled).into_iter().collect()
                }
            }
            Event::TimerFired => {
                // The shell's timer is spent, whether or not its transition came due.
                self.state().wakeup = None;
                Vec::new()
            }
            Event::SunTimesChanged { sunrises, sunsets } => {
                let timestamps = |times: Vec<SystemTime>| -> Vec<Timestamp> {
                    times
                        .into_iter()
                        .map(|time| {
                            Timestamp::try_from(time).expect("sun times are within jiff's range")
                        })
                        .collect()
                };
                self.state().timeline.sun_times =
                    SunTimes::new(&timestamps(sunrises), &timestamps(sunsets));
                Vec::new()
            }
            Event::TimeZoneChanged { iana_id } => {
                match TimeZone::get(&iana_id) {
                    Ok(zone) => {
                        tracing::info!("time zone changed to {iana_id}");
                        self.state().timeline.zone = zone;
                    }
                    Err(error) => {
                        tracing::error!("keeping the old time zone: {iana_id} is unknown: {error}")
                    }
                }
                Vec::new()
            }
            Event::SessionChanged { active } => {
                tracing::info!("session active: {active}");
                self.state().session = if active { Session::Back } else { Session::Away };
                Vec::new()
            }
            Event::Woke => {
                tracing::debug!("woke");
                let now = self.clock.now();
                if let Some(unreported) = &mut self.state().unreported {
                    unreported.until = now + POST_WAKE_GRACE;
                }
                Vec::new()
            }
            Event::Terminating => {
                // Nightcap can't tell a restart from being removed, so it never leaves a force
                // behind (ADR-0011).
                self.state().released = true;
                if let Err(error) = std::fs::remove_file(&self.socket) {
                    tracing::warn!("couldn't remove {}: {error}", self.socket.display());
                }
                Vec::new()
            }
        }
    }

    fn start(setup: Setup, waker: Arc<dyn Waker>) -> Result<Self, CoreError> {
        let requests = ipc::serve(&setup.socket, waker)?;
        let store = state::Store::new(setup.state_file);
        let mut persisted = store.load();
        persisted.expire(setup.clock.now());
        let (rules, config_error) = match config::load(&setup.config_file) {
            Ok(rules) => (rules, None),
            Err(error) => {
                tracing::warn!("running without config rules: {error}");
                (Vec::new(), Some(error))
            }
        };
        Ok(Self {
            instance: setup.instance,
            install: setup.install,
            socket: setup.socket,
            store,
            config_file: setup.config_file,
            clock: setup.clock.clone(),
            requests: Mutex::new(requests),
            state: Mutex::new(State {
                night_shift: NightShift::Unavailable,
                manual_state: persisted.manual_state,
                timeline: Timeline {
                    schedule: None,
                    sun_times: SunTimes::default(),
                    zone: setup.zone,
                },
                context: Context::default(),
                rules,
                config_error,
                snoozes: persisted.snoozes.clone(),
                timed_disable: persisted.timed_disable,
                pending_write: None,
                unreported: None,
                released: false,
                session: Session::Active,
                left_by_last_run: persisted.night_shift,
                applied_until: setup.clock.now(),
                wakeup: None,
                accessibility: false,
                notifications: NotificationPermission::NotRequested,
                login_item_registered: persisted.login_item_registered,
                saved: persisted,
            }),
        })
    }

    /// Only a self-managed release starts at login and updates through Sparkle: an installer
    /// owns a managed one, and a dev build would shadow the installed app (ADR-0015).
    fn updates_itself(&self) -> bool {
        self.install == Install::SelfManaged && self.instance == Instance::Release
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("no panic happens while it's held")
    }

    fn answer(&self, request: &Request, effects: &mut Vec<Effect>) -> Response {
        match request {
            Request::Command(command) => {
                let night_shift = self.state().night_shift;
                if night_shift == NightShift::Unavailable {
                    return Response::Error(ErrorReply {
                        code: ErrorCode::NightShiftUnavailable,
                        message: "Night Shift isn't available on this Mac".to_owned(),
                    });
                }
                let mut state = self.state();
                match command {
                    Command::On => {
                        state.set_manual_state(OnOff::On);
                    }
                    Command::Off => {
                        state.set_manual_state(OnOff::Off);
                    }
                    Command::Toggle => {
                        state.set_manual_state(on_off(night_shift != NightShift::On));
                    }
                    Command::Disable(duration) => {
                        let Ok(until) = self.clock.now().checked_add(*duration) else {
                            return Response::Error(ErrorReply {
                                code: ErrorCode::BadRequest,
                                message: format!("{duration:#} is too long"),
                            });
                        };
                        tracing::info!("timed disable until {until}");
                        state.timed_disable = Some(until);
                    }
                    Command::Resume => {
                        tracing::info!("resumed");
                        state.timed_disable = None;
                        state.snoozes.clear();
                        state.manual_state = None;
                    }
                }
                Response::Ok(Payload::Decision(state.decide()))
            }
            Request::Query(Query::Status) => self.status(),
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
            Request::Reload => match config::load(&self.config_file) {
                Ok(rules) => {
                    tracing::info!("reloaded the config: {} rules", rules.len());
                    let mut state = self.state();
                    state.rules = rules;
                    state.config_error = None;
                    drop(state);
                    self.status()
                }
                Err(error) => {
                    tracing::warn!("keeping the rules loaded before: {error}");
                    self.state().config_error = Some(error.clone());
                    Response::Error(ErrorReply {
                        code: ErrorCode::InvalidConfig,
                        message: error,
                    })
                }
            },
        }
    }

    /// Reconciles a status callback with what the core wants (ADR-0005). macOS catching up on a
    /// transition the core already applied, or running just ahead of the next one, is that
    /// transition; a match needs nothing; anything else is an external change, handled like the
    /// equivalent command (ADR-0006).
    fn reconcile(&self, state: &mut State, enabled: bool) -> Option<Effect> {
        if state.left_by_last_run.take() == Some(on_off(enabled)) {
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
        let wanted = wanted(&state.decide())?;
        if enabled == wanted {
            return None;
        }
        if let Some(transition) = state.timeline.next_transition(state.applied_until)
            && transition.duration_since(now) <= TRANSITION_TOLERANCE
            && state.timeline.scheduled_state(transition) == Some(enabled)
        {
            tracing::info!(
                "schedule transition at {transition}, reported {:#} early",
                transition.duration_since(now)
            );
            state.pass_transition();
            state.applied_until = transition;
            return None;
        }
        tracing::info!(
            "Night Shift turned {} outside Nightcap",
            if enabled { "on" } else { "off" }
        );
        let overridden = state.set_manual_state(on_off(enabled));
        state.collision_notice(&overridden)
    }

    /// Applies every schedule transition that came due since the last event: the manual state
    /// lasts only until the next one.
    fn catch_up(&self) {
        let now = self.clock.now();
        let mut state = self.state();
        if now <= state.applied_until {
            return;
        }
        if state.timed_disable.is_some_and(|until| until <= now) {
            tracing::info!("the timed disable ended");
            state.timed_disable = None;
        }
        if let Some(transition) = state.timeline.next_transition(state.applied_until)
            && transition <= now
        {
            tracing::info!("schedule transition at {transition}");
            state.pass_transition();
            state.unreported = state
                .timeline
                .scheduled_state(now)
                .map(|scheduled| Unreported {
                    transition,
                    scheduled,
                    until: now + TRANSITION_TOLERANCE,
                });
        }
        state.applied_until = now;
    }

    /// Writes `state.json` whenever what it holds changes, so a crash loses nothing (ADR-0011).
    fn save(&self) {
        let mut state = self.state();
        let snapshot = state.snapshot();
        if snapshot == state.saved {
            return;
        }
        match self.store.save(&snapshot) {
            Ok(()) => state.saved = snapshot,
            Err(error) => tracing::warn!("couldn't save the state: {error}"),
        }
    }

    /// Keeps the shell's one wakeup at the next schedule transition or the end of the timed
    /// disable, whichever comes first.
    fn reschedule(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        let next = state
            .timeline
            .next_transition(state.applied_until)
            .into_iter()
            .chain(state.timed_disable)
            .min();
        if next != state.wakeup {
            state.wakeup = next;
            effects.push(match next {
                Some(at) => Effect::ScheduleWakeup { at: at.into() },
                None => Effect::CancelWakeup,
            });
        }
    }

    /// Writes Night Shift when what the core wants differs from what was last observed
    /// (ADR-0005), once until a status callback reports the result.
    fn enforce(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        if state.night_shift == NightShift::Unavailable || state.session != Session::Active {
            return;
        }
        let Some(wanted) = wanted(&state.decide()) else {
            return;
        };
        if wanted != (state.night_shift == NightShift::On) && state.pending_write != Some(wanted) {
            state.pending_write = Some(wanted);
            effects.push(Effect::SetNightShift { enabled: wanted });
        }
    }

    fn status(&self) -> Response {
        let state = self.state();
        Response::Ok(Payload::Status(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: self.install,
            night_shift: state.night_shift,
            decision: state.decide(),
            manual_state: state.manual_state,
            schedule: state.timeline.schedule,
            scheduled_state: state
                .timeline
                .scheduled_state(state.applied_until)
                .map(on_off),
            next_transition: state.timeline.next_transition(state.applied_until),
            frontmost_app: state.context.app.clone(),
            rules: state.rules.clone(),
            config_error: state.config_error.clone(),
            snoozes: state.snoozes.clone(),
            timed_disable: state.timed_disable,
            permissions: Permissions {
                accessibility: state.accessibility,
                notifications: state.notifications,
            },
        }))
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

/// What turning Night Shift on overrode.
#[derive(Default)]
struct Overridden {
    snoozed: Vec<RuleIdentity>,
    timed_disable: bool,
}

impl State {
    /// Sets the manual state the way `on`, `off` and external changes do (ADR-0006): turning
    /// Night Shift on while it's forced off snoozes the forcing rules and cancels a timed
    /// disable.
    fn set_manual_state(&mut self, state: OnOff) -> Overridden {
        let mut overridden = Overridden::default();
        let decision = self.decide();
        if state == OnOff::On && decision.verdict == Verdict::ForceOff {
            for reason in decision.reasons {
                match reason {
                    Reason::Rule(rule) => overridden.snoozed.push(rule.identity),
                    Reason::TimedDisable { .. } => overridden.timed_disable = true,
                    _ => {}
                }
            }
            for identity in &overridden.snoozed {
                tracing::info!("snoozed {identity} until the next schedule transition");
            }
            self.snoozes.extend(overridden.snoozed.iter().cloned());
            if overridden.timed_disable {
                tracing::info!("cancelled the timed disable");
                self.timed_disable = None;
            }
        }
        self.manual_state = Some(state);
        overridden
    }

    /// Tells the user what an external change overrode, if it collided with a force (ADR-0006).
    fn collision_notice(&self, overridden: &Overridden) -> Option<Effect> {
        if overridden.snoozed.is_empty() && !overridden.timed_disable {
            return None;
        }
        let until = match self.timeline.next_transition(self.applied_until) {
            Some(transition) => format!(
                "until {}",
                self.timeline.zone.to_datetime(transition).strftime("%H:%M")
            ),
            None => "until you resume".to_owned(),
        };
        let mut sentences = Vec::new();
        if !overridden.snoozed.is_empty() {
            let snoozed: Vec<_> = overridden.snoozed.iter().map(ToString::to_string).collect();
            sentences.push(format!("Snoozed {} {until}.", snoozed.join(", ")));
        }
        if overridden.timed_disable {
            sentences.push("Cancelled the timed disable.".to_owned());
        }
        Some(Effect::Notify {
            title: "Night Shift turned on outside Nightcap".to_owned(),
            body: sentences.join(" "),
        })
    }

    fn snapshot(&self) -> state::Persisted {
        state::Persisted {
            login_item_registered: self.login_item_registered,
            manual_state: self.manual_state,
            snoozes: self.snoozes.clone(),
            timed_disable: self.timed_disable,
            night_shift: wanted(&self.decide()).map(on_off),
            until: self.timeline.next_transition(self.applied_until),
        }
    }

    /// Snoozes and the manual state last only until the next schedule transition (ADR-0006).
    fn pass_transition(&mut self) {
        self.manual_state = None;
        self.snoozes.clear();
    }

    fn decide(&self) -> Decision {
        engine::decide(&Inputs {
            context: &self.context,
            rules: if self.released { &[] } else { &self.rules },
            snoozes: &self.snoozes,
            timed_disable: self.timed_disable.filter(|_| !self.released),
            manual_state: self.manual_state,
            scheduled_state: self
                .timeline
                .scheduled_state(self.applied_until)
                .map(on_off),
            next_transition: self.timeline.next_transition(self.applied_until),
        })
    }
}

/// The Night Shift state a decision asks for, if it asks for one.
fn wanted(decision: &Decision) -> Option<bool> {
    match decision.verdict {
        Verdict::ForceOff => Some(false),
        Verdict::HandsOff => decision.reasons.iter().find_map(|reason| match reason {
            Reason::ManualState { state, .. } | Reason::Schedule { state, .. } => {
                Some(*state == OnOff::On)
            }
            Reason::Rule(_) | Reason::TimedDisable { .. } | Reason::Snooze { .. } => None,
        }),
    }
}

fn on_off(on: bool) -> OnOff {
    if on { OnOff::On } else { OnOff::Off }
}

#[cfg(test)]
mod tests;
