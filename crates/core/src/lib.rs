uniffi::setup_scaffolding!();

mod ipc;
mod logging;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

use nightcap_protocol::{Instance, Payload, Permissions, Query, Request, Response, Status};

/// Lets the core's own threads ask the shell for a `tick()` on the main thread (ADR-0002).
#[uniffi::export(foreign)]
pub trait Waker: Send + Sync {
    fn wake(&self);
}

#[derive(Debug, uniffi::Enum)]
pub enum Event {
    PermissionsChanged { accessibility: bool },
    Terminating,
}

#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum Effect {
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
    socket: PathBuf,
    requests: Mutex<mpsc::Receiver<ipc::Pending>>,
    state: Mutex<State>,
}

/// What the core believes about the system, as last reported by the shell.
#[derive(Default)]
struct State {
    accessibility: bool,
}

#[uniffi::export]
impl Core {
    /// Starts the core for the app with this bundle ID, which picks the instance: its
    /// subsystem for logging and its socket. Fails if that instance is already running.
    #[uniffi::constructor]
    pub fn new(bundle_id: String, waker: Arc<dyn Waker>) -> Result<Arc<Self>, CoreError> {
        let instance =
            Instance::from_bundle_id(&bundle_id).ok_or_else(|| CoreError::UnknownBundleId {
                bundle_id: bundle_id.clone(),
            })?;
        logging::init(&bundle_id);
        let socket = instance.socket_path().map_err(|error| CoreError::Io {
            message: error.to_string(),
        })?;
        let core = Self::listening_at(socket, waker)?;
        tracing::info!("core {} listening", env!("CARGO_PKG_VERSION"));
        Ok(Arc::new(core))
    }

    pub fn handle(&self, event: Event) -> Vec<Effect> {
        match event {
            Event::PermissionsChanged { accessibility } => {
                tracing::info!("accessibility permission: {accessibility}");
                self.state().accessibility = accessibility;
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
        for pending in requests.try_iter() {
            let response = self.answer(&pending.request);
            pending.reply(response);
        }
        Vec::new()
    }
}

impl Core {
    fn listening_at(socket: PathBuf, waker: Arc<dyn Waker>) -> Result<Self, CoreError> {
        let requests = ipc::serve(&socket, waker)?;
        Ok(Self {
            socket,
            requests: Mutex::new(requests),
            state: Mutex::default(),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("no panic happens while it's held")
    }

    fn answer(&self, request: &Request) -> Response {
        match request {
            Request::Query(Query::Status) => Response::Ok(Payload::Status(Status {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                permissions: Permissions {
                    accessibility: self.state().accessibility,
                },
            })),
        }
    }
}

#[cfg(test)]
mod tests;
