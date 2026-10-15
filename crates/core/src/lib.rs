uniffi::setup_scaffolding!();

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
    Command, ErrorCode, ErrorReply, Install, Instance, NightShift, NotificationPermission, OnOff,
    Payload, Permissions, Query, Request, Response, Schedule, Status, TimeOfDay,
};

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
    Terminating,
}

#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum Effect {
    RegisterLoginItem,
    SetNightShift {
        enabled: bool,
    },
    /// Replaces any earlier wakeup; the shell answers with `TimerFired`.
    ScheduleWakeup {
        at: SystemTime,
    },
    CancelWakeup,
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

/// How long before a computed schedule transition a status callback can arrive and still be that
/// transition: CoreBrightness's sun times and the core's can differ slightly (ADR-0005).
const TRANSITION_TOLERANCE: SignedDuration = SignedDuration::from_mins(2);

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
    clock: Arc<dyn Clock>,
    zone: TimeZone,
}

/// What the core believes about the system, as last reported by the shell, plus what it
/// keeps across restarts.
struct State {
    night_shift: NightShift,
    manual_state: Option<OnOff>,
    timeline: Timeline,
    /// Every schedule transition up to here has been applied.
    applied_until: Timestamp,
    /// The wakeup the shell was last asked for.
    wakeup: Option<Timestamp>,
    accessibility: bool,
    notifications: NotificationPermission,
    persisted: state::Persisted,
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
        let state_folder = etcetera::base_strategy::Xdg::new()
            .ok()
            .and_then(|xdg| xdg.state_dir())
            .ok_or_else(|| CoreError::Io {
                message: "can't find the XDG state directory".to_owned(),
            })?;
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
        self.reschedule(&mut effects);
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
        self.reschedule(&mut effects);
        effects
    }
}

impl Core {
    fn react(&self, event: Event) -> Vec<Effect> {
        match event {
            // Dev builds never start at login: they'd shadow the installed app (ADR-0014).
            Event::Launched => {
                let registered = self.state().persisted.login_item_registered;
                match (self.install, self.instance, registered) {
                    (Install::SelfManaged, Instance::Release, false) => {
                        vec![Effect::RegisterLoginItem]
                    }
                    _ => Vec::new(),
                }
            }
            Event::LoginItemRegistered => {
                let mut state = self.state();
                state.persisted.login_item_registered = true;
                if let Err(error) = self.store.save(&state.persisted) {
                    tracing::warn!("couldn't save the state: {error}");
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
                let mut state = self.state();
                state.accessibility = accessibility;
                state.notifications = notifications;
                Vec::new()
            }
            Event::StatusChanged { enabled, schedule } => {
                let mut state = self.state();
                state.night_shift = if enabled {
                    NightShift::On
                } else {
                    NightShift::Off
                };
                let previous = state.timeline.schedule.replace(schedule);
                if previous.is_some_and(|previous| previous != schedule) {
                    // macOS applies an edited schedule at once, so the edit is a schedule
                    // transition and whatever it switched isn't an external change.
                    tracing::info!("schedule edited");
                    state.manual_state = None;
                } else {
                    self.reconcile(&mut state, enabled);
                }
                Vec::new()
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
            Event::Woke => {
                tracing::debug!("woke");
                Vec::new()
            }
            Event::Terminating => {
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
        let persisted = store.load();
        Ok(Self {
            instance: setup.instance,
            install: setup.install,
            socket: setup.socket,
            store,
            clock: setup.clock.clone(),
            requests: Mutex::new(requests),
            state: Mutex::new(State {
                night_shift: NightShift::Unavailable,
                manual_state: None,
                timeline: Timeline {
                    schedule: None,
                    sun_times: SunTimes::default(),
                    zone: setup.zone,
                },
                applied_until: setup.clock.now(),
                wakeup: None,
                accessibility: false,
                notifications: NotificationPermission::NotRequested,
                persisted,
            }),
        })
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
                let enabled = match command {
                    Command::On => true,
                    Command::Off => false,
                    Command::Toggle => night_shift != NightShift::On,
                };
                self.set_manual_state(enabled, effects);
                self.status()
            }
            Request::Query(Query::Status) => self.status(),
        }
    }

    /// Reconciles a status callback with what the core wants (ADR-0005). A match needs nothing,
    /// a change just ahead of a computed schedule transition is that transition, and anything
    /// else is an external change, handled like the equivalent command (ADR-0006).
    fn reconcile(&self, state: &mut State, enabled: bool) {
        let Some(scheduled) = state.timeline.scheduled_state(state.applied_until) else {
            return;
        };
        let wanted = state
            .manual_state
            .map_or(scheduled, |manual| manual == OnOff::On);
        if enabled == wanted {
            return;
        }
        let now = self.clock.now();
        if let Some(transition) = state.timeline.next_transition(state.applied_until)
            && transition.duration_since(now) <= TRANSITION_TOLERANCE
            && state.timeline.scheduled_state(transition) == Some(enabled)
        {
            tracing::info!(
                "schedule transition at {transition}, reported {:#} early",
                transition.duration_since(now)
            );
            state.manual_state = None;
            state.applied_until = transition;
            return;
        }
        tracing::info!(
            "Night Shift turned {} outside Nightcap",
            if enabled { "on" } else { "off" }
        );
        state.manual_state = Some(on_off(enabled));
    }

    /// Applies every schedule transition that came due since the last event: the manual state
    /// lasts only until the next one.
    fn catch_up(&self) {
        let now = self.clock.now();
        let mut state = self.state();
        if now <= state.applied_until {
            return;
        }
        if let Some(transition) = state.timeline.next_transition(state.applied_until)
            && transition <= now
        {
            tracing::info!("schedule transition at {transition}");
            state.manual_state = None;
        }
        state.applied_until = now;
    }

    /// Keeps the shell's one wakeup at the next schedule transition.
    fn reschedule(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        let next = state.timeline.next_transition(state.applied_until);
        if next != state.wakeup {
            state.wakeup = next;
            effects.push(match next {
                Some(at) => Effect::ScheduleWakeup { at: at.into() },
                None => Effect::CancelWakeup,
            });
        }
    }

    /// Writes Night Shift only when it differs from what was last observed (ADR-0005).
    fn set_manual_state(&self, enabled: bool, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        state.manual_state = Some(on_off(enabled));
        let wanted = if enabled {
            NightShift::On
        } else {
            NightShift::Off
        };
        if state.night_shift != wanted {
            effects.push(Effect::SetNightShift { enabled });
        }
    }

    fn status(&self) -> Response {
        let state = self.state();
        Response::Ok(Payload::Status(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: self.install,
            night_shift: state.night_shift,
            manual_state: state.manual_state,
            schedule: state.timeline.schedule,
            scheduled_state: state
                .timeline
                .scheduled_state(state.applied_until)
                .map(on_off),
            next_transition: state.timeline.next_transition(state.applied_until),
            permissions: Permissions {
                accessibility: state.accessibility,
                notifications: state.notifications,
            },
        }))
    }
}

fn on_off(on: bool) -> OnOff {
    if on { OnOff::On } else { OnOff::Off }
}

#[cfg(test)]
mod tests;
