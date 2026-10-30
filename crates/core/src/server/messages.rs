//! What crosses between the shell and the core (ADR-0002): the facts the shell reports, the
//! commands only the app sends, the outcome of each, with what it answers, the effects the shell
//! carries out and the reply to the app's command, and the listener that hears it with the
//! snapshot its input left.

use std::sync::Arc;
use std::time::SystemTime;

use jiff::Timestamp;
use nightcap_protocol::{
    Done, LoginItem, NotificationPermission, OnOff, Place, Refusal, RuleIdentity, Schedule,
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
        /// When Night Shift was last written, by Nightcap or another app such as Control
        /// Center, as CoreBrightness marks it: None once the schedule has switched it since, or
        /// the schedule's mode changed (ADR-0004).
        written_at: Option<SystemTime>,
        /// Whose write, if any, this status is the first to show (ADR-0004).
        switched_by: SwitchedBy,
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

/// Whose write a status report is the first to show (ADR-0004). CoreBrightness marks every
/// write, Nightcap's and Control Center's alike, and none of the schedule's switches; the shell
/// knows which writes it made itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SwitchedBy {
    /// No new write: the schedule's own switch, or a report that shows nothing new.
    Schedule,
    /// Nightcap's own write, landing.
    Nightcap,
    /// A write by another app, such as Control Center.
    Outside,
}

/// A command only the app sends, which never goes on the wire: the CLI begins no trial
/// (ADR-0016).
#[derive(Debug, uniffi::Enum)]
pub enum AppCommand {
    /// Begins a trial, unless one runs (ADR-0016), which stands in for the schedule with Night
    /// Shift on if `stands_in`.
    BeginTrial { stands_in: bool },
    /// Ends the trial, which puts back what it found.
    EndTrial,
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
    /// Tells the user Night Shift switched outside Nightcap, taken as flipping what they see
    /// (ADR-0018): `place` has a rule saying `state` until the schedule turns, at `until` if it
    /// has a turn, snoozing its indefinite rule, if one decided.
    Adopted {
        place: Place,
        snoozed: Option<RuleIdentity>,
        state: OnOff,
        until: Option<Timestamp>,
    },
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
    /// What the shell must do, in order.
    pub effects: Vec<Effect>,
    /// The reply to the app's own command, under the ticket it sent the command with: None for
    /// a fact, quitting, a command sent with no ticket, and the CLI's, whose reply goes over
    /// its socket.
    pub reply: Option<Replied>,
}

impl Outcome {
    /// An outcome of `cause` with nothing decided yet.
    pub fn new(cause: Cause) -> Self {
        Self {
            cause,
            effects: Vec::new(),
            reply: None,
        }
    }
}

/// The reply to a command of the app's, with the ticket it sent the command with.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct Replied {
    pub ticket: u64,
    pub reply: Reply,
}

/// A command's reply, the same whoever sent it (ADR-0002).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Reply {
    /// What it did.
    Done(Done),
    /// Why the core refused it, which then changed nothing.
    Refused(Refusal),
}

impl From<Result<Done, Refusal>> for Reply {
    fn from(answer: Result<Done, Refusal>) -> Self {
        match answer {
            Ok(done) => Self::Done(done),
            Err(why) => Self::Refused(why),
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
