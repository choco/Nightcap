//! What crosses between the shell and the core (ADR-0002): the facts the shell reports, the
//! commands only the app sends, the outcome of each, with what it answers, the effects it
//! carries out and the events it tells, and the listener that hears it with the snapshot its
//! input left.

use std::sync::Arc;
use std::time::SystemTime;

use jiff::Timestamp;
use nightcap_protocol::{
    Check, Command, LoginItem, NotificationPermission, OnOff, Place, Refusal, RuleIdentity,
    Schedule,
};

use crate::Snapshot;

/// Something macOS observed, which the shell reports.
#[derive(Debug, uniffi::Enum)]
pub enum Fact {
    Launched,
    /// An app came to the front; some processes have no bundle ID. `is_browser` says whether
    /// it's in the shell's browser list, so its host can be watched.
    AppActivated {
        bundle_id: Option<String>,
        is_browser: bool,
    },
    /// The watched browser's host, read under `generation`; None when the read failed or timed
    /// out, which leaves only app rules.
    HostChanged {
        generation: u64,
        host: Option<String>,
    },
    /// The shell read the login item's state, which the user can change in System Settings.
    LoginItemChanged {
        item: LoginItem,
    },
    PermissionsChanged {
        accessibility: bool,
        notifications: NotificationPermission,
    },
    /// Night Shift as CoreBrightness reports it. Custom hours must be valid times of day.
    StatusChanged {
        enabled: bool,
        schedule: Schedule,
        /// Whether macOS has sunset to sunrise set aside, to put back once Location Services
        /// is on.
        sun_schedule_set_aside: bool,
    },
    /// The strength, 0 to 100, as the shell read it. CoreBrightness announces no strength
    /// change, so the shell reads it whenever something is about to show it.
    StrengthChanged {
        value: u8,
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
}

/// A command only the app sends, which never goes on the wire: the CLI begins no trial
/// (ADR-0016) and has no onboarding.
#[derive(Debug, uniffi::Enum)]
pub enum AppCommand {
    /// Begins a trial, unless one runs (ADR-0016), which stands in for the schedule with Night
    /// Shift on if `stands_in`.
    BeginTrial { stands_in: bool },
    /// Ends the trial, which puts back what it found.
    EndTrial,
    /// Finishes onboarding, with whether the user chose to check websites, or None where the
    /// config sets it and onboarding never asked.
    FinishOnboarding { websites: Option<bool> },
}

#[derive(Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Effect {
    /// Registers Nightcap to open at login, as the user chose (ADR-0011).
    RegisterLoginItem,
    UnregisterLoginItem,
    /// Starts Sparkle's daily checks, in self-managed releases only (ADR-0011).
    StartUpdater,
    /// Starts Sparkle's own check, as if the user had asked for one.
    CheckForUpdates,
    /// Opens onboarding, skipping the steps of versions up to `completed`.
    ShowOnboarding {
        completed: u32,
    },
    /// Watches this browser's host, replacing any earlier watch; reports carry `generation`.
    WatchBrowser {
        bundle_id: String,
        generation: u64,
    },
    StopWatching,
    SetNightShift {
        enabled: bool,
    },
    /// Writes Night Shift's schedule. macOS applies it at once, so it can switch Night Shift
    /// on or off.
    SetSchedule {
        schedule: Schedule,
    },
    /// Writes the strength, 0 to 100. Uncommitted, it's a preview: macOS shows it, even with
    /// Night Shift off, but doesn't save it.
    SetStrength {
        value: u8,
        commit: bool,
    },
    /// Replaces any earlier wakeup; the shell answers with `TimerFired`.
    ScheduleWakeup {
        at: SystemTime,
    },
    CancelWakeup,
}

/// Something that happened, which a client may tell the user, once.
#[derive(Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Event {
    /// Night Shift switched outside Nightcap, taken as flipping what the user sees (ADR-0018):
    /// `place` has a rule saying `state` until the schedule turns, at `until` if it has a turn,
    /// snoozing its indefinite rule, if one decided.
    Adopted {
        place: Place,
        snoozed: Option<RuleIdentity>,
        state: OnOff,
        until: Option<Timestamp>,
    },
    /// A check became a problem the user didn't cause, to announce once: what it found, and
    /// where that leaves it. Night Shift being unavailable, notifications switched off and a
    /// login item waiting for approval never are.
    ProblemAppeared { check: Check },
    /// The core refused a command from the app, other than a reload, whose refusal the config
    /// file's check shows; the CLI hears its refusal as the reply.
    Refused { command: Command, why: Refusal },
}

/// What an outcome answers (ADR-0002), as a fact about its input: what that means for the user
/// is each client's to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Cause {
    /// The shell's launch, `Fact::Launched`.
    Launch,
    /// Any other fact the shell reported.
    Fact,
    /// A command from the app or the CLI, the app's own among them.
    Command,
    /// The app quitting, whose outcome `Core::stop` returns.
    Quit,
}

/// What one fact or command led to, in the order the core decided it.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct Outcome {
    /// What the outcome answers.
    pub cause: Cause,
    /// What macOS must do, in order.
    pub effects: Vec<Effect>,
    /// What happened, for the clients to tell.
    pub events: Vec<Event>,
}

impl Outcome {
    /// An outcome of `cause` with nothing decided yet.
    pub fn new(cause: Cause) -> Self {
        Self {
            cause,
            effects: Vec::new(),
            events: Vec::new(),
        }
    }
}

/// Hears the outcome of every fact and command, whoever sent it, in the order the core decided
/// them (ADR-0002). The core calls it on its own thread, between inputs and outside any lock, so
/// it hands the outcome on and returns: the shell's hops to the main queue.
#[uniffi::export(foreign)]
pub trait Listener: Send + Sync {
    /// `snapshot` was published after this outcome's input, before the next is decided.
    fn outcome(&self, outcome: Outcome, snapshot: Arc<Snapshot>);
}
