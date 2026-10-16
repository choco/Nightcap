uniffi::setup_scaffolding!();

mod adhoc;
mod config;
mod domain;
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
    RuleIdentity, RuleSource, Schedule, Status, TimeOfDay, Verdict,
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

// A menu item carries the very command the CLI would send (ADR-0002).
uniffi::custom_type!(SignedDuration, i64, {
    remote,
    lower: |duration| duration.as_secs(),
    try_lift: |seconds| Ok(SignedDuration::from_secs(seconds)),
});

#[uniffi::remote(Enum)]
pub enum RuleIdentity {
    App(String),
    Domain(String),
}

#[uniffi::remote(Enum)]
pub enum Command {
    On,
    Off,
    Toggle,
    Disable(SignedDuration),
    Resume,
    AddRule(RuleIdentity),
    RemoveRule(RuleIdentity),
    Snooze(RuleIdentity),
    Unsnooze(RuleIdentity),
}

/// What the menu shows. The core decides it; the shell only draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Menu {
    pub icon: MenuIcon,
    /// What Night Shift is doing and why, as a sentence.
    pub summary: String,
    pub entries: Vec<MenuEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MenuIcon {
    On,
    Off,
    ForcedOff,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MenuEntry {
    Item { item: MenuItem },
    Submenu { title: String, items: Vec<MenuItem> },
    Separator,
}

/// Choosing it sends `command` back as `Event::MenuItemChosen`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MenuItem {
    pub title: String,
    pub checked: bool,
    pub command: Command,
}

/// Lets the core's own threads ask the shell for a `tick()` on the main thread (ADR-0002).
#[uniffi::export(foreign)]
pub trait Waker: Send + Sync {
    fn wake(&self);
}

#[derive(Debug, uniffi::Enum)]
pub enum Event {
    Launched,
    /// An app came to the front; some processes have no bundle ID. `is_browser` says whether
    /// it's in the shell's browser list, so its host can be watched.
    AppActivated {
        bundle_id: Option<String>,
        /// The app's name as the user sees it, for the menu.
        name: Option<String>,
        is_browser: bool,
    },
    /// The watched browser's host, read under `generation`; None when the read failed or timed
    /// out, which leaves only app rules.
    HostChanged {
        generation: u64,
        host: Option<String>,
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
    /// The user chose a menu item, which carries the command to run.
    MenuItemChosen {
        command: Command,
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
    /// Watches this browser's host, replacing any earlier watch; reports carry `generation`.
    WatchBrowser {
        bundle_id: String,
        generation: u64,
    },
    StopWatching,
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

/// The menu's timed disables, in minutes, shortest first.
const DISABLE_CHOICES: [(i64, &str); 3] = [(30, "30 Minutes"), (60, "1 Hour"), (120, "2 Hours")];

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
    ad_hoc_store: adhoc::Store,
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
    rules_file: PathBuf,
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
    /// The config rules and the ad-hoc ones not in the config, as the rule engine sees them.
    rules: Vec<Rule>,
    config_rules: Vec<Rule>,
    config_error: Option<String>,
    ad_hoc: Vec<RuleIdentity>,
    /// Why `rules.toml` can't be used; while set, ad-hoc rules are off and the file is left be.
    ad_hoc_error: Option<String>,
    /// The frontmost app's name, for the menu.
    app_name: Option<String>,
    /// Whether the frontmost app is in the shell's browser list.
    is_browser: bool,
    watch: Option<Watch>,
    /// The generation of the last watch started.
    generation: u64,
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
                rules_file: xdg
                    .data_dir()
                    .join(instance.xdg_folder_name())
                    .join("rules.toml"),
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
        self.rewatch(&mut effects);
        self.enforce(&mut effects);
        self.reschedule(&mut effects);
        self.save();
        effects
    }

    pub fn menu(&self) -> Menu {
        let state = self.state();
        let decision = state.decide();
        if state.night_shift == NightShift::Unavailable {
            return Menu {
                icon: MenuIcon::Unavailable,
                summary: "Night Shift isn't available on this Mac".to_owned(),
                entries: Vec::new(),
            };
        }
        let on = wanted(&decision).unwrap_or(state.night_shift == NightShift::On);
        let item = |title: &str, command| MenuItem {
            title: title.to_owned(),
            checked: false,
            command,
        };
        let mut entries = vec![MenuEntry::Item {
            item: if on {
                item("Turn Night Shift Off", Command::Off)
            } else {
                item("Turn Night Shift On", Command::On)
            },
        }];
        // A timed disable matters while Night Shift is on or forced off, or when the schedule
        // turns it on before the longest one would end.
        let longest = SignedDuration::from_mins(DISABLE_CHOICES[DISABLE_CHOICES.len() - 1].0);
        let comes_on_soon = state
            .timeline
            .next_transition(state.applied_until)
            .is_some_and(|transition| {
                transition.duration_since(self.clock.now()) <= longest
                    && state.timeline.scheduled_state(transition) == Some(true)
            });
        if on || decision.verdict == Verdict::ForceOff || comes_on_soon {
            entries.push(MenuEntry::Submenu {
                title: "Disable For".to_owned(),
                items: DISABLE_CHOICES
                    .map(|(minutes, title)| {
                        item(title, Command::Disable(SignedDuration::from_mins(minutes)))
                    })
                    .to_vec(),
            });
        }
        if state.timed_disable.is_some()
            || !state.snoozes.is_empty()
            || state.manual_state.is_some()
        {
            entries.push(MenuEntry::Item {
                item: item("Resume", Command::Resume),
            });
        }
        if let Some(bundle_id) = &state.context.app {
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Item {
                item: state.app_toggle(bundle_id),
            });
        }
        Menu {
            icon: match (decision.verdict, on) {
                (Verdict::ForceOff, _) => MenuIcon::ForcedOff,
                (Verdict::HandsOff, true) => MenuIcon::On,
                (Verdict::HandsOff, false) => MenuIcon::Off,
            },
            summary: format!("Night Shift {}", decision.describe(&state.timeline.zone)),
            entries,
        }
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
        self.rewatch(&mut effects);
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
            Event::AppActivated {
                bundle_id,
                name,
                is_browser,
            } => {
                tracing::debug!("frontmost app: {bundle_id:?}, browser: {is_browser}");
                let mut state = self.state();
                state.context.app = bundle_id;
                state.app_name = name;
                state.is_browser = is_browser;
                Vec::new()
            }
            // The host itself is never logged (ADR-0012).
            Event::HostChanged { generation, host } => {
                let mut state = self.state();
                match &mut state.watch {
                    Some(watch) if watch.generation == generation => {
                        watch.reported = true;
                        state.context.host = host.as_deref().and_then(domain::normalize);
                        tracing::debug!("host reported for watch {generation}");
                    }
                    _ => tracing::debug!("dropped a host from stale watch {generation}"),
                }
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
            Event::MenuItemChosen { command } => {
                if let Err(error) = self.run_command(&command) {
                    // The message can name a domain, so only the code is logged (ADR-0012).
                    tracing::warn!("a menu command was refused: {:?}", error.code);
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
        let (config_rules, config_error) = match config::load(&setup.config_file) {
            Ok(rules) => (rules, None),
            Err(error) => {
                // The error can quote a domain, so it goes to status, not the log (ADR-0012).
                tracing::warn!("running without config rules: the config is invalid; see status");
                (Vec::new(), Some(error))
            }
        };
        let ad_hoc_store = adhoc::Store::new(setup.rules_file);
        let (ad_hoc, ad_hoc_error) = match ad_hoc_store.load() {
            Ok(rules) => (rules, None),
            Err(error) => {
                tracing::warn!("ad-hoc rules are off: rules.toml can't be used; see status");
                (Vec::new(), Some(error))
            }
        };
        let core = Self {
            instance: setup.instance,
            install: setup.install,
            socket: setup.socket,
            store,
            config_file: setup.config_file,
            ad_hoc_store,
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
                rules: Vec::new(),
                config_rules,
                config_error,
                ad_hoc,
                ad_hoc_error,
                app_name: None,
                is_browser: false,
                watch: None,
                generation: 0,
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
        };
        core.graduate();
        Ok(core)
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
            Request::Command(command) => match self.run_command(command) {
                Ok(decision) => Response::Ok(Payload::Decision(decision)),
                Err(error) => Response::Error(error),
            },
            Request::Query(Query::Status) => self.status(),
            Request::Query(Query::Rules) => {
                Response::Ok(Payload::Rules(self.state().rules.clone()))
            }
            Request::Query(Query::Export(format)) => Response::Ok(Payload::Export(adhoc::export(
                &self.state().ad_hoc,
                *format,
            ))),
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
                    state.config_rules = rules;
                    state.config_error = None;
                    drop(state);
                    self.graduate();
                    self.status()
                }
                Err(error) => {
                    tracing::warn!("keeping the rules loaded before: the config is invalid");
                    self.state().config_error = Some(error.clone());
                    Response::Error(ErrorReply {
                        code: ErrorCode::InvalidConfig,
                        message: error,
                    })
                }
            },
        }
    }

    /// Runs a command from the CLI or the menu, answering with the resulting decision.
    fn run_command(&self, command: &Command) -> Result<Decision, ErrorReply> {
        let night_shift = self.state().night_shift;
        let needs_night_shift = matches!(
            command,
            Command::On | Command::Off | Command::Toggle | Command::Disable(_) | Command::Resume
        );
        if needs_night_shift && night_shift == NightShift::Unavailable {
            return Err(refused(
                ErrorCode::NightShiftUnavailable,
                "Night Shift isn't available on this Mac",
            ));
        }
        let mut state = self.state();
        let outcome = match command {
            Command::On => {
                state.set_manual_state(OnOff::On);
                Ok(())
            }
            Command::Off => {
                state.set_manual_state(OnOff::Off);
                Ok(())
            }
            Command::Toggle => {
                state.set_manual_state(on_off(night_shift != NightShift::On));
                Ok(())
            }
            Command::Disable(duration) => match self.clock.now().checked_add(*duration) {
                Ok(until) => {
                    tracing::info!("timed disable until {until}");
                    state.timed_disable = Some(until);
                    Ok(())
                }
                Err(_) => Err(refused(
                    ErrorCode::BadRequest,
                    format!("{duration:#} is too long"),
                )),
            },
            Command::Resume => {
                tracing::info!("resumed");
                state.timed_disable = None;
                state.snoozes.clear();
                state.manual_state = None;
                Ok(())
            }
            Command::AddRule(identity) => self.add_rule(&mut state, identity),
            Command::RemoveRule(identity) => self.remove_rule(&mut state, identity),
            Command::Snooze(identity) => state.snooze(identity, true),
            Command::Unsnooze(identity) => state.snooze(identity, false),
        };
        outcome.map(|()| state.decide())
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

    /// Watches the frontmost browser only while its host could change the decision (ADR-0010).
    fn rewatch(&self, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        let wanted = state
            .context
            .app
            .clone()
            .filter(|_| state.is_browser && engine::host_matters(&state.inputs()));
        match (&state.watch, wanted) {
            (Some(watch), Some(bundle_id)) if watch.bundle_id == bundle_id => {}
            (_, Some(bundle_id)) => {
                state.generation += 1;
                let generation = state.generation;
                state.context.host = None;
                state.watch = Some(Watch {
                    bundle_id: bundle_id.clone(),
                    generation,
                    reported: false,
                });
                effects.push(Effect::WatchBrowser {
                    bundle_id,
                    generation,
                });
            }
            (Some(_), None) => {
                state.context.host = None;
                state.watch = None;
                effects.push(Effect::StopWatching);
            }
            (None, None) => {}
        }
    }

    /// Drops the ad-hoc rules the config now has, which have graduated (ADR-0009), and lays out
    /// the rules the engine sees.
    fn graduate(&self) {
        let mut state = self.state();
        let config = state.config_rules.clone();
        let before = state.ad_hoc.len();
        state.ad_hoc.retain(|identity| {
            !config
                .iter()
                .any(|rule| same_rule(&rule.identity, identity))
        });
        let graduated = before - state.ad_hoc.len();
        if graduated > 0 {
            tracing::info!("{graduated} ad-hoc rules graduated into the config");
            if let Err(error) = self.ad_hoc_store.save(&state.ad_hoc) {
                tracing::warn!("couldn't save rules.toml: {error}");
            }
        }
        state.lay_out_rules();
    }

    fn add_rule(&self, state: &mut State, identity: &RuleIdentity) -> Result<(), ErrorReply> {
        let identity = normalize_identity(identity)?;
        if state
            .rules
            .iter()
            .any(|rule| same_rule(&rule.identity, &identity))
        {
            return Ok(());
        }
        state.check_ad_hoc_rules_on()?;
        state.ad_hoc.push(identity.clone());
        if let Err(error) = self.ad_hoc_store.save(&state.ad_hoc) {
            state.ad_hoc.pop();
            return Err(refused(
                ErrorCode::AdHocRulesOff,
                format!("couldn't save rules.toml: {error}"),
            ));
        }
        tracing::info!("added an ad-hoc rule: {}", loggable(&identity));
        state.lay_out_rules();
        Ok(())
    }

    fn remove_rule(&self, state: &mut State, identity: &RuleIdentity) -> Result<(), ErrorReply> {
        let identity = normalize_identity(identity)?;
        let Some(index) = state
            .ad_hoc
            .iter()
            .position(|kept| same_rule(kept, &identity))
        else {
            if state
                .config_rules
                .iter()
                .any(|rule| same_rule(&rule.identity, &identity))
            {
                return Err(refused(
                    ErrorCode::ConfigRule,
                    format!(
                        "{identity} comes from the config; snooze it instead with `nightcap rules snooze {identity}`"
                    ),
                ));
            }
            return Err(refused(
                ErrorCode::NoSuchRule,
                format!("{identity} isn't a rule"),
            ));
        };
        state.check_ad_hoc_rules_on()?;
        let removed = state.ad_hoc.remove(index);
        if let Err(error) = self.ad_hoc_store.save(&state.ad_hoc) {
            state.ad_hoc.insert(index, removed);
            return Err(refused(
                ErrorCode::AdHocRulesOff,
                format!("couldn't save rules.toml: {error}"),
            ));
        }
        tracing::info!("removed an ad-hoc rule: {}", loggable(&identity));
        state
            .snoozes
            .retain(|snoozed| !same_rule(snoozed, &identity));
        state.lay_out_rules();
        Ok(())
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
        // A new watch keeps Night Shift as it is until its first read (ADR-0010).
        let reading = state.watch.as_ref().is_some_and(|watch| !watch.reported);
        if state.night_shift == NightShift::Unavailable
            || state.session != Session::Active
            || reading
        {
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
        Response::Ok(Payload::Status(Box::new(Status {
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
            watching: state.watch.is_some(),
            host: state.context.host.clone(),
            rules: state.rules.clone(),
            config_error: state.config_error.clone(),
            ad_hoc_error: state.ad_hoc_error.clone(),
            snoozes: state.snoozes.clone(),
            timed_disable: state.timed_disable,
            permissions: Permissions {
                accessibility: state.accessibility,
                notifications: state.notifications,
            },
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
    bundle_id: String,
    generation: u64,
    reported: bool,
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
                tracing::info!(
                    "snoozed {} until the next schedule transition",
                    loggable(identity)
                );
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

    /// The menu's toggle for the frontmost app (ADR-0009): it adds an ad-hoc rule, removes
    /// one, snoozes a config rule, or undoes a snooze.
    fn app_toggle(&self, bundle_id: &str) -> MenuItem {
        let name = self.app_name.as_deref().unwrap_or(bundle_id);
        let identity = RuleIdentity::App(bundle_id.to_owned());
        let toggle = |title: String, checked, command| MenuItem {
            title,
            checked,
            command,
        };
        let Some(rule) = self
            .rules
            .iter()
            .find(|rule| same_rule(&rule.identity, &identity))
        else {
            return toggle(
                format!("Force Off in {name}"),
                false,
                Command::AddRule(identity),
            );
        };
        let identity = rule.identity.clone();
        if self
            .snoozes
            .iter()
            .any(|snoozed| same_rule(snoozed, &identity))
        {
            toggle(
                format!("Force Off in {name} (snoozed)"),
                false,
                Command::Unsnooze(identity),
            )
        } else if rule.source == RuleSource::Config {
            toggle(
                format!("Force Off in {name} (from the config)"),
                true,
                Command::Snooze(identity),
            )
        } else {
            toggle(
                format!("Force Off in {name}"),
                true,
                Command::RemoveRule(identity),
            )
        }
    }

    /// The config rules, then the ad-hoc rules the config doesn't have.
    fn lay_out_rules(&mut self) {
        let ad_hoc = self.ad_hoc.iter().cloned().map(|identity| Rule {
            identity,
            source: RuleSource::AdHoc,
        });
        self.rules = self.config_rules.iter().cloned().chain(ad_hoc).collect();
    }

    fn check_ad_hoc_rules_on(&self) -> Result<(), ErrorReply> {
        match &self.ad_hoc_error {
            None => Ok(()),
            Some(error) => Err(refused(
                ErrorCode::AdHocRulesOff,
                format!("ad-hoc rules are off: {error}"),
            )),
        }
    }

    /// Snoozes or unsnoozes an existing rule, config or ad-hoc, until the next schedule
    /// transition.
    fn snooze(&mut self, identity: &RuleIdentity, snoozed: bool) -> Result<(), ErrorReply> {
        let identity = normalize_identity(identity)?;
        let Some(rule) = self
            .rules
            .iter()
            .find(|rule| same_rule(&rule.identity, &identity))
        else {
            return Err(refused(
                ErrorCode::NoSuchRule,
                format!("{identity} isn't a rule"),
            ));
        };
        let rule = rule.identity.clone();
        self.snoozes.retain(|kept| !same_rule(kept, &rule));
        if snoozed {
            tracing::info!(
                "snoozed {} until the next schedule transition",
                loggable(&rule)
            );
            self.snoozes.push(rule);
        }
        Ok(())
    }

    /// Snoozes and the manual state last only until the next schedule transition (ADR-0006).
    fn pass_transition(&mut self) {
        self.manual_state = None;
        self.snoozes.clear();
    }

    fn decide(&self) -> Decision {
        engine::decide(&self.inputs())
    }

    fn inputs(&self) -> Inputs<'_> {
        Inputs {
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
        }
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

fn refused(code: ErrorCode, message: impl Into<String>) -> ErrorReply {
    ErrorReply {
        code,
        message: message.into(),
    }
}

/// `identity` as rules keep it: a bundle ID is letters, digits, hyphens and periods, and a
/// domain is bare and normalized (ADR-0008).
fn normalize_identity(identity: &RuleIdentity) -> Result<RuleIdentity, ErrorReply> {
    let normalized = match identity {
        RuleIdentity::App(bundle_id) => (!bundle_id.is_empty()
            && bundle_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'))
        .then(|| identity.clone()),
        RuleIdentity::Domain(domain) => domain::normalize(domain).map(RuleIdentity::Domain),
    };
    normalized.ok_or_else(|| {
        refused(
            ErrorCode::BadIdentity,
            format!(
                "{identity} isn't a rule identity like app:com.figma.Desktop or domain:figma.com"
            ),
        )
    })
}

/// Rules with the same identity are the same rule; bundle IDs compare case-insensitively.
fn same_rule(a: &RuleIdentity, b: &RuleIdentity) -> bool {
    match (a, b) {
        (RuleIdentity::App(a), RuleIdentity::App(b)) => a.eq_ignore_ascii_case(b),
        (RuleIdentity::Domain(a), RuleIdentity::Domain(b)) => a == b,
        _ => false,
    }
}

/// A rule identity as the log may show it: `tracing-oslog` logs everything as public, so a
/// domain rule is named by its kind only (ADR-0012).
fn loggable(identity: &RuleIdentity) -> String {
    match identity {
        RuleIdentity::App(_) => identity.to_string(),
        RuleIdentity::Domain(_) => "a domain rule".to_owned(),
    }
}

fn on_off(on: bool) -> OnOff {
    if on { OnOff::On } else { OnOff::Off }
}

#[cfg(test)]
mod tests;
