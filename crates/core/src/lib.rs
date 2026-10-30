#![warn(clippy::expect_used, clippy::indexing_slicing, clippy::unreachable)]

uniffi::setup_scaffolding!();

mod clock;
mod commands;
mod ffi;
mod ipc;
mod logging;
mod rules;
#[cfg(any(test, debug_assertions))]
mod scenario;
mod server;
mod setup;
mod storage;

pub use ffi::{is_bundle_id, normalize_domain};
pub use server::messages::{
    AppCommand, Cause, Effect, Fact, Listener, Outcome, Replied, Reply, SwitchedBy,
};
pub use server::snapshot::Snapshot;

#[cfg(any(test, debug_assertions))]
pub use scenario::{Scenario, ScenarioError, Staged, Step};
#[cfg(any(test, debug_assertions))]
pub use storage::file::File;

use std::fmt;
use std::sync::{Arc, mpsc};

use nightcap_protocol::{Command, IoFailure, PathError};

use clock::{Clock, SystemClock};
use ipc::Socket;
use server::mailbox::{Mail, Mailbox};
#[cfg(any(test, debug_assertions))]
use server::react::react;
use server::react::{Input, ReplyTo};
use server::snapshot::Published;
use server::start::{Setup, install_of, instance_of, zone_of};
use server::state::Inner;
use storage::file::{Disk, Paths};

/// The core, a server on its own thread (ADR-0002): facts and commands go into its mailbox and
/// return at once, its listener hears each one's outcome, and every read takes the snapshot it
/// publishes after each, never the mailbox.
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
    Inline(Arc<scenario::Inline>),
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
    /// `bundle_writable` says whether the user can replace the app bundle, which makes a
    /// release a self-managed install. `time_zone` is the system's IANA time zone ID. `listener` hears
    /// the outcome of every fact and command, with the snapshot its input left, on the core's
    /// thread.
    #[uniffi::constructor]
    pub fn start(
        bundle_id: String,
        bundle_writable: bool,
        time_zone: String,
        listener: Arc<dyn Listener>,
    ) -> Result<Arc<Self>, CoreError> {
        let instance = instance_of(&bundle_id)?;
        logging::init(&bundle_id);
        let path = instance.socket_path()?;
        let files = Disk(Paths::xdg(instance)?);
        let zone = zone_of(&time_zone)?;
        let install = install_of(instance, bundle_writable);
        // Before any file is read, so a second instance stops here.
        let socket = ipc::bind(&path)?;
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let inner = Inner::start(Setup {
            install,
            files: Arc::new(files),
            clock: clock.clone(),
            zone,
        });
        let core = Self::run(inner, clock, Some(socket), listener)?;
        tracing::info!(
            "core {} listening, {install:?} install",
            env!("CARGO_PKG_VERSION")
        );
        Ok(Arc::new(core))
    }

    /// Something macOS observed: a signal, which nobody waits on.
    pub fn report(&self, fact: Fact) {
        self.post(Input::Fact(fact));
    }

    /// A command from the menu, Settings, onboarding or the shortcut, decided as the CLI's are.
    /// Returns at once; its outcome carries `ticket` back with the reply, after its effects.
    #[uniffi::method(default(ticket = None))]
    pub fn send(&self, command: Command, ticket: Option<u64>) {
        self.post(Input::Command(command, ReplyTo::App(ticket)));
    }

    /// A command only the app sends (ADR-0016), replied to the same way.
    #[uniffi::method(default(ticket = None))]
    pub fn send_app(&self, command: AppCommand, ticket: Option<u64>) {
        self.post(Input::AppCommand(command, ticket));
    }

    /// Releases every force as the app quits (ADR-0008), once everything before it is decided,
    /// and returns what the shell must carry out before it exits. The core decides nothing
    /// after, so the snapshot it publishes is the one quitting left.
    pub fn stop(&self) -> Outcome {
        match &self.runs {
            Runs::Thread(mailbox) => {
                let (reply, released) = mpsc::channel();
                match mailbox.post(Mail::Stop { reply }) {
                    Ok(()) => released.recv().unwrap_or_else(|_| {
                        tracing::error!("the core's thread ended before releasing every force");
                        Outcome::new(Cause::Quit)
                    }),
                    Err(error) => {
                        tracing::warn!("asked to stop again: {error}");
                        Outcome::new(Cause::Quit)
                    }
                }
            }
            #[cfg(any(test, debug_assertions))]
            Runs::Inline(inline) => {
                inline.decide(|inner, now| react(inner, Input::Terminating, now).0)
            }
        }
    }

    /// The snapshot the core published after its last input, which every query and the export
    /// read from, so one draw's answers all come from the same input (ADR-0002). It can lag one
    /// input behind; the outcome that follows carries the snapshot its input left.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.published.latest()
    }
}

impl Core {
    /// A core deciding from `inner` on its own thread, by `clock`, which tells `listener` each
    /// outcome, with `socket`'s thread putting the CLI's requests into its mailbox.
    pub(crate) fn run(
        inner: Inner,
        clock: Arc<dyn Clock>,
        socket: Option<Socket>,
        listener: Arc<dyn Listener>,
    ) -> Result<Self, CoreError> {
        let published = Arc::new(Published::new(Snapshot::of(&inner, clock.now())));
        let path = socket.as_ref().map(|socket| socket.path.clone());
        let mailbox = server::spawn(inner, clock, path, published.clone(), listener)?;
        if let Some(socket) = socket {
            ipc::serve(socket.listener, mailbox.clone(), published.clone())?;
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
            Runs::Inline(inline) => _ = inline.decide(|inner, now| react(inner, input, now)),
        }
    }
}

/// Why the core couldn't start. An I/O failure carries its kind, and the core logs macOS's own
/// reason as it finds it.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{bundle_id} isn't a Nightcap bundle ID")]
    UnknownBundleId { bundle_id: String },
    #[error("another Nightcap is already running at {path}")]
    AlreadyRunning { path: String },
    #[error("{iana_id} isn't a time zone this Mac knows")]
    UnknownTimeZone { iana_id: String },
    #[error("can't read the time zone database")]
    NoTimeZoneDatabase,
    /// There's no home folder, or no XDG state directory, for the socket and the files.
    #[error("can't find the home folder")]
    NoHome,
    #[error("the socket path {path} is longer than macOS allows")]
    SocketPathTooLong { path: String },
    /// macOS refused the socket, or its folder, at `path`.
    #[error("couldn't take {path}: {failure:?}")]
    Io { failure: IoFailure, path: String },
    /// One of the core's threads couldn't start.
    #[error("couldn't start a thread: {failure:?}")]
    Thread { failure: IoFailure },
}

impl From<PathError> for CoreError {
    fn from(error: PathError) -> Self {
        match error {
            PathError::NoHome => Self::NoHome,
            PathError::SocketPathTooLong(path) => Self::SocketPathTooLong {
                path: path.display().to_string(),
            },
        }
    }
}
