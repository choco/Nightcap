//! The IPC server thread (ADR-0010). It never touches the core's state: each request is
//! queued, the shell is asked for a tick, and the answer comes back from the main thread.

use std::fs::{self, DirBuilder, Permissions};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use nightcap_protocol::{
    ErrorCode, ErrorReply, Hello, HelloMessage, HelloReply, PROTOCOL_VERSION, Request, Response,
};
use serde::{Serialize, de::DeserializeOwned};

use crate::{CoreError, Waker};

/// How long a client may take to send a line, and how long it waits for the main thread.
const TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct Pending {
    pub(crate) request: Request,
    reply: mpsc::Sender<Response>,
}

impl Pending {
    /// A request, and where its response arrives.
    pub(crate) fn new(request: Request) -> (Self, mpsc::Receiver<Response>) {
        let (reply, response) = mpsc::channel();
        (Self { request, reply }, response)
    }

    pub(crate) fn reply(self, response: Response) {
        // The receiver is gone only if the client already hung up, and then no one is waiting.
        let _ = self.reply.send(response);
    }
}

pub(crate) fn serve(
    path: &Path,
    waker: Arc<dyn Waker>,
) -> Result<mpsc::Receiver<Pending>, CoreError> {
    let listener = bind(path)?;
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("nightcap-ipc".to_owned())
        .spawn(move || accept(&listener, &sender, waker.as_ref()))
        .map_err(|error| io_error("couldn't start the IPC thread", &error))?;
    Ok(receiver)
}

fn bind(path: &Path) -> Result<UnixListener, CoreError> {
    let folder = path.parent().ok_or_else(|| CoreError::Io {
        message: format!("{} has no folder", path.display()),
    })?;
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
        .and_then(|()| fs::set_permissions(folder, Permissions::from_mode(0o700)))
        .map_err(|error| io_error(&format!("couldn't prepare {}", folder.display()), &error))?;
    if fs::symlink_metadata(path).is_ok() {
        if UnixStream::connect(path).is_ok() {
            return Err(CoreError::AlreadyRunning {
                path: path.display().to_string(),
            });
        }
        fs::remove_file(path).map_err(|error| {
            io_error(&format!("couldn't remove stale {}", path.display()), &error)
        })?;
    }
    UnixListener::bind(path)
        .map_err(|error| io_error(&format!("couldn't listen at {}", path.display()), &error))
}

fn accept(listener: &UnixListener, sender: &mpsc::Sender<Pending>, waker: &dyn Waker) {
    for stream in listener.incoming() {
        match stream.and_then(|stream| converse(stream, sender, waker)) {
            Ok(()) => {}
            // A client that leaves before saying anything, such as another instance checking
            // whether this one is alive.
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                tracing::debug!("IPC client hung up before answering");
            }
            Err(error) => tracing::warn!("IPC connection failed: {error}"),
        }
    }
}

fn converse(
    stream: UnixStream,
    sender: &mpsc::Sender<Pending>,
    waker: &dyn Waker,
) -> io::Result<()> {
    // macOS refuses socket options once the peer has closed its end.
    stream.set_read_timeout(Some(TIMEOUT)).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidInput {
            io::Error::new(io::ErrorKind::UnexpectedEof, "the client hung up")
        } else {
            error
        }
    })?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    let hello = match read_line::<HelloMessage>(&mut reader)? {
        Ok(message) => message.hello,
        Err(error) => {
            return write_line(&mut writer, &HelloReply::Error(bad_request(&error)));
        }
    };
    if hello.protocol != PROTOCOL_VERSION {
        let ours = Hello::current();
        let message = format!(
            "the daemon is Nightcap {} (protocol {}) and this CLI speaks protocol {}; restart Nightcap",
            ours.version, ours.protocol, hello.protocol
        );
        return write_line(
            &mut writer,
            &HelloReply::Error(ErrorReply {
                code: ErrorCode::ProtocolMismatch,
                message,
            }),
        );
    }
    write_line(&mut writer, &HelloReply::Hello(Hello::current()))?;

    let request = match read_line::<Request>(&mut reader)? {
        Ok(request) => request,
        Err(error) => return write_line(&mut writer, &Response::Error(bad_request(&error))),
    };
    let (pending, response) = Pending::new(request);
    sender
        .send(pending)
        .map_err(|_| io::Error::other("the core has shut down"))?;
    waker.wake();
    let response = response.recv_timeout(TIMEOUT).map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "the main thread didn't tick in time",
        )
    })?;
    write_line(&mut writer, &response)
}

/// Reads one line; the outer error is the connection's, the inner one the message's.
fn read_line<T: DeserializeOwned>(
    reader: &mut impl BufRead,
) -> io::Result<Result<T, serde_json::Error>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the client hung up",
        ));
    }
    Ok(serde_json::from_str(&line))
}

fn write_line(writer: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, message)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn bad_request(error: &serde_json::Error) -> ErrorReply {
    ErrorReply {
        code: ErrorCode::BadRequest,
        message: error.to_string(),
    }
}

fn io_error(context: &str, error: &io::Error) -> CoreError {
    CoreError::Io {
        message: format!("{context}: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::Duration;

    use jiff::tz::TimeZone;
    use nightcap_protocol::{
        Decision, ErrorCode, Hello, HelloMessage, HelloReply, Install, Instance, NightShift,
        NotificationPermission, OnOff, PROTOCOL_VERSION, Payload, Permissions, Place, Query,
        Request, Response, Status, WebsiteChecking,
    };

    use super::{Pending, serve};
    use crate::file::{Memory, Paths};
    use crate::staging::ManualClock;
    use crate::start::Setup;
    use crate::state::Inner;
    use crate::{Core, CoreError, Outcome, Waker};

    /// Stands in for the shell: every wake is reported on a channel the test can wait on.
    struct ChannelWaker(Mutex<mpsc::Sender<()>>);

    impl Waker for ChannelWaker {
        fn wake(&self) {
            self.0.lock().unwrap().send(()).unwrap();
        }
    }

    fn waker() -> (Arc<dyn Waker>, mpsc::Receiver<()>) {
        let (sender, wakes) = mpsc::channel();
        (Arc::new(ChannelWaker(Mutex::new(sender))), wakes)
    }

    /// The socket, in a folder that serving creates.
    fn socket_in(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("instance").join("nightcap.sock")
    }

    /// Serves `socket` with no core behind it: each request waits in the channel to be answered
    /// by hand.
    fn served(socket: &Path) -> (mpsc::Receiver<Pending>, mpsc::Receiver<()>) {
        let (waker, wakes) = waker();
        (serve(socket, waker).unwrap(), wakes)
    }

    /// A core on files in memory at noon, answering what arrives at `socket` on each tick.
    fn served_core(socket: &Path) -> (Core, mpsc::Receiver<()>) {
        let (requests, wakes) = served(socket);
        let inner = Inner::start(Setup {
            instance: Instance::Release,
            install: Install::Managed,
            socket: Some(socket.to_owned()),
            files: Arc::new(Memory::new(Paths::in_folder(Path::new("/nightcap")))),
            clock: Arc::new(ManualClock::new(
                "2026-10-16T12:00:00+02:00".parse().unwrap(),
            )),
            zone: TimeZone::get("Europe/Zurich").unwrap(),
        });
        (Core::serving(inner, requests), wakes)
    }

    /// Speaks the wire protocol from the client side: a hello, then at most one request.
    struct Client {
        reader: BufReader<UnixStream>,
        writer: UnixStream,
    }

    impl Client {
        fn connect(socket: &Path) -> Self {
            let stream = UnixStream::connect(socket).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            Self {
                reader: BufReader::new(stream.try_clone().unwrap()),
                writer: stream,
            }
        }

        fn send(&mut self, line: &str) {
            writeln!(self.writer, "{line}").unwrap();
        }

        fn receive<T: serde::de::DeserializeOwned>(&mut self) -> T {
            let mut line = String::new();
            self.reader.read_line(&mut line).unwrap();
            serde_json::from_str(&line).unwrap()
        }

        fn hello(&mut self, protocol: u32) -> HelloReply {
            let hello = HelloMessage {
                hello: Hello {
                    protocol,
                    version: "test".to_owned(),
                },
            };
            self.send(&serde_json::to_string(&hello).unwrap());
            self.receive()
        }
    }

    /// Sends `request` from a thread of its own, which returns the response.
    fn ask(socket: &Path, request: Request) -> thread::JoinHandle<Response> {
        let socket = socket.to_owned();
        thread::spawn(move || {
            let mut client = Client::connect(&socket);
            assert_eq!(
                client.hello(PROTOCOL_VERSION),
                HelloReply::Hello(Hello::current())
            );
            client.send(&serde_json::to_string(&request).unwrap());
            client.receive()
        })
    }

    #[test]
    fn a_client_that_hangs_up_at_once_leaves_the_server_answering() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (requests, wakes) = served(&socket);

        drop(UnixStream::connect(&socket).unwrap());

        let response = ask(&socket, Request::Query(Query::Rules));
        wakes
            .recv_timeout(Duration::from_secs(5))
            .expect("the IPC thread asks for a tick");
        let pending = requests.try_recv().unwrap();
        assert_eq!(pending.request, Request::Query(Query::Rules));
        pending.reply(Response::Ok(Payload::Rules(Vec::new())));
        assert_eq!(
            response.join().unwrap(),
            Response::Ok(Payload::Rules(Vec::new()))
        );
    }

    #[test]
    fn a_protocol_mismatch_is_refused_at_hello() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (requests, wakes) = served(&socket);

        let reply = Client::connect(&socket).hello(99);

        let HelloReply::Error(error) = reply else {
            panic!("expected an error, got {reply:?}");
        };
        assert_eq!(error.code, ErrorCode::ProtocolMismatch);
        assert!(
            wakes.try_recv().is_err() && requests.try_recv().is_err(),
            "a refused client never reaches the main thread"
        );
    }

    #[test]
    fn a_malformed_request_is_answered_without_a_tick() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (requests, wakes) = served(&socket);

        let mut client = Client::connect(&socket);
        client.hello(PROTOCOL_VERSION);
        client.send(r#"{"query":"weather"}"#);
        let response: Response = client.receive();

        let Response::Error(error) = response else {
            panic!("expected an error, got {response:?}");
        };
        assert_eq!(error.code, ErrorCode::BadRequest);
        assert!(wakes.try_recv().is_err());
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn a_bad_identity_is_refused_before_the_core() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (requests, wakes) = served(&socket);

        for bad in ["domain:https://figma.com", "app:com.figma.Desktop/", "app:"] {
            let mut client = Client::connect(&socket);
            client.hello(PROTOCOL_VERSION);
            client.send(&format!(
                r#"{{"command":{{"add_rule":{{"identity":"{bad}","says":"off"}}}}}}"#
            ));
            let response: Response = client.receive();
            let Response::Error(error) = response else {
                panic!("{bad} was passed on: {response:?}");
            };
            assert_eq!(error.code, ErrorCode::BadRequest, "{bad}");
        }
        assert!(wakes.try_recv().is_err());
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn a_second_instance_on_the_same_socket_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let _first = served(&socket);

        let second = serve(&socket, waker().0);

        assert!(matches!(second, Err(CoreError::AlreadyRunning { .. })));
    }

    #[test]
    fn a_stale_socket_left_by_a_crash_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
        drop(UnixListener::bind(&socket).unwrap());

        let _served = served(&socket);

        assert!(UnixStream::connect(&socket).is_ok());
    }

    #[test]
    fn the_socket_folder_is_private_to_the_user() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);

        let _served = served(&socket);

        let mode = std::fs::metadata(socket.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn a_status_query_is_answered_on_the_next_tick() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (core, wakes) = served_core(&socket);

        let response = ask(&socket, Request::Query(Query::Status));
        wakes
            .recv_timeout(Duration::from_secs(5))
            .expect("the IPC thread asks for a tick");
        assert_eq!(core.tick(), Outcome::default());

        let Response::Ok(Payload::Status(mut status)) = response.join().unwrap() else {
            panic!("expected a status");
        };
        // Checks have tests of their own.
        status.checks.clear();
        assert_eq!(
            status,
            Box::new(Status {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                install: Install::Managed,
                night_shift: NightShift::Unavailable,
                strength: None,
                config_strength: None,
                decision: Decision {
                    state: None,
                    place: Place::Everywhere,
                    reasons: Vec::new(),
                },
                schedule: None,
                config_schedule: None,
                scheduled_state: None,
                next_transition: None,
                frontmost_app: None,
                watching: false,
                host: None,
                rules: Vec::new(),
                config_error: None,
                ad_hoc_error: None,
                website_checking: WebsiteChecking {
                    state: OnOff::Off,
                    from_config: false,
                },
                layers: Vec::new(),
                permissions: Permissions {
                    accessibility: false,
                    notifications: NotificationPermission::NotRequested,
                },
                checks: Vec::new(),
            })
        );
    }

    #[test]
    fn terminating_removes_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (core, _wakes) = served_core(&socket);

        assert_eq!(core.stop(), Outcome::default());

        assert!(!socket.exists());
    }
}
