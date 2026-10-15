uniffi::setup_scaffolding!();

mod ipc;
mod logging;
mod state;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};

use etcetera::BaseStrategy;

use nightcap_protocol::{
    Command, ErrorCode, ErrorReply, Install, Instance, ManualState, NightShift,
    NotificationPermission, Payload, Permissions, Query, Request, Response, Status,
};

#[uniffi::remote(Enum)]
pub enum NotificationPermission {
    NotRequested,
    Denied,
    Granted,
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
    /// Night Shift as CoreBrightness reports it.
    StatusChanged {
        enabled: bool,
    },
    Terminating,
}

#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum Effect {
    RegisterLoginItem,
    SetNightShift { enabled: bool },
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{bundle_id} isn't a Nightcap bundle ID")]
    UnknownBundleId { bundle_id: String },
    #[error("another Nightcap is already running at {path}")]
    AlreadyRunning { path: String },
    #[error("{message}")]
    Io { message: String },
}

#[derive(uniffi::Object)]
pub struct Core {
    instance: Instance,
    install: Install,
    socket: PathBuf,
    store: state::Store,
    requests: Mutex<mpsc::Receiver<ipc::Pending>>,
    state: Mutex<State>,
}

/// Where a core lives and what kind of install it serves.
struct Setup {
    instance: Instance,
    install: Install,
    socket: PathBuf,
    state_file: PathBuf,
}

/// What the core believes about the system, as last reported by the shell, plus what it
/// keeps across restarts.
struct State {
    night_shift: NightShift,
    manual_state: Option<ManualState>,
    accessibility: bool,
    notifications: NotificationPermission,
    persisted: state::Persisted,
}

#[uniffi::export]
impl Core {
    /// Starts the core for the app with this bundle ID, which picks the instance: its
    /// subsystem for logging and its socket. Fails if that instance is already running.
    /// `bundle_writable` says whether the user can replace the app bundle, which makes it a
    /// self-managed install.
    #[uniffi::constructor]
    pub fn new(
        bundle_id: String,
        bundle_writable: bool,
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
            Event::StatusChanged { enabled } => {
                self.state().night_shift = if enabled {
                    NightShift::On
                } else {
                    NightShift::Off
                };
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

    pub fn tick(&self) -> Vec<Effect> {
        let requests = self
            .requests
            .lock()
            .expect("no panic happens while it's held");
        let mut effects = Vec::new();
        for pending in requests.try_iter() {
            let response = self.answer(&pending.request, &mut effects);
            pending.reply(response);
        }
        effects
    }
}

impl Core {
    fn start(setup: Setup, waker: Arc<dyn Waker>) -> Result<Self, CoreError> {
        let requests = ipc::serve(&setup.socket, waker)?;
        let store = state::Store::new(setup.state_file);
        let persisted = store.load();
        Ok(Self {
            instance: setup.instance,
            install: setup.install,
            socket: setup.socket,
            store,
            requests: Mutex::new(requests),
            state: Mutex::new(State {
                night_shift: NightShift::Unavailable,
                manual_state: None,
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

    /// Writes Night Shift only when it differs from what was last observed (ADR-0005).
    fn set_manual_state(&self, enabled: bool, effects: &mut Vec<Effect>) {
        let mut state = self.state();
        state.manual_state = Some(if enabled {
            ManualState::On
        } else {
            ManualState::Off
        });
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
            permissions: Permissions {
                accessibility: state.accessibility,
                notifications: state.notifications,
            },
        }))
    }
}

#[cfg(test)]
mod tests;
