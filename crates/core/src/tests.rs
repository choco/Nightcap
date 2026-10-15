use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use nightcap_protocol::{
    ErrorCode, Hello, HelloMessage, HelloReply, Payload, Query, Request, Response, Status,
};

use crate::{Core, CoreError, Event, Waker};

/// Stands in for the shell: every wake is reported on a channel the test can wait on.
struct ChannelWaker(Mutex<mpsc::Sender<()>>);

impl Waker for ChannelWaker {
    fn wake(&self) {
        self.0.lock().unwrap().send(()).unwrap();
    }
}

fn start(socket: &Path) -> (Core, mpsc::Receiver<()>) {
    let (sender, wakes) = mpsc::channel();
    let waker = Arc::new(ChannelWaker(Mutex::new(sender)));
    let core = Core::listening_at(socket.to_owned(), waker).unwrap();
    (core, wakes)
}

fn socket_in(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("instance").join("nightcap.sock")
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

#[test]
fn a_status_query_is_answered_on_the_next_tick() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    let client = thread::spawn(move || {
        let mut client = Client::connect(&socket);
        assert_eq!(client.hello(1), HelloReply::Hello(Hello::current()));
        client.send(&serde_json::to_string(&Request::Query(Query::Status)).unwrap());
        client.receive::<Response>()
    });

    wakes
        .recv_timeout(Duration::from_secs(5))
        .expect("the IPC thread asks for a tick");
    assert_eq!(core.tick(), Vec::new());
    assert_eq!(
        client.join().unwrap(),
        Response::Ok(Payload::Status(Status {
            version: env!("CARGO_PKG_VERSION").to_owned()
        }))
    );
}

#[test]
fn a_client_that_hangs_up_at_once_leaves_the_server_answering() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_core, _wakes) = start(&socket);

    drop(UnixStream::connect(&socket).unwrap());

    assert_eq!(
        Client::connect(&socket).hello(1),
        HelloReply::Hello(Hello::current())
    );
}

#[test]
fn a_protocol_mismatch_is_refused_at_hello() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_core, wakes) = start(&socket);

    let reply = Client::connect(&socket).hello(99);

    let HelloReply::Error(error) = reply else {
        panic!("expected an error, got {reply:?}");
    };
    assert_eq!(error.code, ErrorCode::ProtocolMismatch);
    assert!(
        wakes.try_recv().is_err(),
        "a refused client never reaches the main thread"
    );
}

#[test]
fn a_malformed_request_is_answered_without_a_tick() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_core, wakes) = start(&socket);

    let mut client = Client::connect(&socket);
    client.hello(1);
    client.send(r#"{"query":"weather"}"#);
    let response: Response = client.receive();

    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(wakes.try_recv().is_err());
}

#[test]
fn a_second_instance_on_the_same_socket_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_first, _wakes) = start(&socket);

    let (sender, _) = mpsc::channel();
    let second = Core::listening_at(socket.clone(), Arc::new(ChannelWaker(Mutex::new(sender))));

    assert!(matches!(second, Err(CoreError::AlreadyRunning { .. })));
}

#[test]
fn a_stale_socket_left_by_a_crash_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    drop(UnixListener::bind(&socket).unwrap());

    let (_core, _wakes) = start(&socket);

    assert!(UnixStream::connect(&socket).is_ok());
}

#[test]
fn the_socket_folder_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_core, _wakes) = start(&socket);

    let mode = std::fs::metadata(socket.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn terminating_removes_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes) = start(&socket);

    assert_eq!(core.handle(Event::Terminating), Vec::new());

    assert!(!socket.exists());
}
