//! The CLI's connection (ADR-0010): the socket's thread, which never touches the core's state: a
//! request's queries are all answered from the one snapshot the core last published, and a
//! command goes into the core's mailbox, its answer back over the socket once the core's thread
//! has decided it.

use std::fs::{self, DirBuilder, Permissions};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use nightcap_protocol::{
    Hello, HelloMessage, HelloReply, IoFailure, PROTOCOL_VERSION, Request, Response,
};
use serde::{Serialize, de::DeserializeOwned};

use crate::CoreError;
use crate::server::mailbox::{Mail, Mailbox};
use crate::server::snapshot::Published;

/// How long a client may take to send a line, and how long it waits for the core's thread.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Takes the socket at `path`, failing if another instance holds it.
pub(crate) fn bind(path: &Path) -> Result<UnixListener, CoreError> {
    let Some(folder) = path.parent() else {
        tracing::error!("{} has no folder", path.display());
        return Err(CoreError::Io {
            failure: IoFailure::Other,
            path: path.display().to_string(),
        });
    };
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
        .and_then(|()| fs::set_permissions(folder, Permissions::from_mode(0o700)))
        .map_err(|error| io_error("couldn't prepare", folder, &error))?;
    if fs::symlink_metadata(path).is_ok() {
        if UnixStream::connect(path).is_ok() {
            return Err(CoreError::AlreadyRunning {
                path: path.display().to_string(),
            });
        }
        fs::remove_file(path).map_err(|error| io_error("couldn't remove stale", path, &error))?;
    }
    UnixListener::bind(path).map_err(|error| io_error("couldn't listen at", path, &error))
}

/// Serves `listener` on a thread of its own.
pub(crate) fn serve(
    listener: UnixListener,
    mailbox: Mailbox,
    published: Arc<Published>,
) -> Result<(), CoreError> {
    thread::Builder::new()
        .name("nightcap-ipc".to_owned())
        .spawn(move || accept(&listener, &mailbox, &published))
        .map_err(|error| {
            tracing::error!("couldn't start the IPC thread: {error}");
            CoreError::Thread {
                failure: error.kind().into(),
            }
        })?;
    Ok(())
}

fn accept(listener: &UnixListener, mailbox: &Mailbox, published: &Published) {
    for stream in listener.incoming() {
        match stream.and_then(|stream| converse(stream, mailbox, published)) {
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

fn converse(stream: UnixStream, mailbox: &Mailbox, published: &Published) -> io::Result<()> {
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
            let message = error.to_string();
            return write_line(&mut writer, &HelloReply::Malformed { message });
        }
    };
    if hello.protocol != PROTOCOL_VERSION {
        return write_line(&mut writer, &HelloReply::Mismatch(Hello::current()));
    }
    write_line(&mut writer, &HelloReply::Hello(Hello::current()))?;

    let request = match read_line::<Request>(&mut reader)? {
        Ok(request) => request,
        Err(error) => {
            let message = error.to_string();
            return write_line(&mut writer, &Response::Malformed { message });
        }
    };
    let response = match request {
        Request::Queries(queries) => published.latest().answer(&queries),
        Request::Command(command) => {
            let (reply, answer) = mpsc::channel();
            mailbox
                .post(Mail::Command { command, reply })
                .map_err(io::Error::other)?;
            Response::from(answer.recv_timeout(TIMEOUT).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the core's thread didn't answer in time",
                )
            })?)
        }
    };
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

/// `error` at `path` as the shell gets it: its kind. The log gets macOS's own reason, after what
/// start-up was `doing` there.
fn io_error(doing: &str, path: &Path, error: &io::Error) -> CoreError {
    tracing::error!("{doing} {}: {error}", path.display());
    CoreError::Io {
        failure: error.kind().into(),
        path: path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::Duration;

    use nightcap_protocol::{
        Answer, Command, Decision, Front, Hello, HelloMessage, HelloReply, IoFailure, NightShift,
        OnOff, PROTOCOL_VERSION, Payload, Place, PlaceInFront, Query, Request, Response,
        RuleIdentity,
    };

    use super::{bind, serve};
    use crate::server::mailbox::{Mail, Mailbox};
    use crate::server::snapshot::{Published, Snapshot};
    use crate::server::tests::{ChannelListener, Heard, noon};
    use crate::{Cause, Core, CoreError, Outcome};

    /// The socket, in a folder that serving creates.
    fn socket_in(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("instance").join("nightcap.sock")
    }

    /// Serves `socket` with no core's thread behind it: what reaches the mailbox waits there,
    /// and queries read a snapshot of a core at noon with nothing reported.
    fn served(socket: &Path) -> mpsc::Receiver<Mail> {
        let (mailbox, inbox) = Mailbox::new();
        let published = Arc::new(Published::new(Snapshot::of(&noon(None))));
        serve(bind(socket).unwrap(), mailbox, published).unwrap();
        inbox
    }

    /// A core on its own thread at noon, serving `socket`, with the outcomes it tells its
    /// listener.
    fn served_core(socket: &Path) -> (Core, mpsc::Receiver<Heard>) {
        let (listener, outcomes) = ChannelListener::new();
        let core = Core::run(
            noon(Some(socket)),
            Some(bind(socket).unwrap()),
            Arc::new(listener),
        )
        .unwrap();
        (core, outcomes)
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
        let inbox = served(&socket);

        drop(UnixStream::connect(&socket).unwrap());

        let response = ask(&socket, Request::Command(Command::Reload));
        let Ok(Mail::Command { command, reply }) = inbox.recv_timeout(Duration::from_secs(5))
        else {
            panic!("the reload reaches the mailbox");
        };
        assert_eq!(command, Command::Reload);
        reply.send(Ok(Payload::Rules(Vec::new()))).unwrap();
        assert_eq!(
            response.join().unwrap(),
            Response::Ok(Payload::Rules(Vec::new()))
        );
    }

    #[test]
    fn a_protocol_mismatch_is_refused_at_hello() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let inbox = served(&socket);

        let reply = Client::connect(&socket).hello(99);

        assert_eq!(reply, HelloReply::Mismatch(Hello::current()));
        assert!(
            inbox.try_recv().is_err(),
            "a refused client never reaches the core"
        );
    }

    #[test]
    fn a_malformed_request_is_refused_before_the_core() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let inbox = served(&socket);

        let mut client = Client::connect(&socket);
        client.hello(PROTOCOL_VERSION);
        client.send(r#"{"queries":["weather"]}"#);
        let response: Response = client.receive();

        assert!(
            matches!(&response, Response::Malformed { message } if message.contains("weather")),
            "{response:?}"
        );
        assert!(inbox.try_recv().is_err());
    }

    #[test]
    fn a_bad_identity_is_refused_before_the_core() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let inbox = served(&socket);

        for bad in ["domain:https://figma.com", "app:com.figma.Desktop/", "app:"] {
            let mut client = Client::connect(&socket);
            client.hello(PROTOCOL_VERSION);
            client.send(&format!(
                r#"{{"command":{{"add_rule":{{"identity":"{bad}","says":"off"}}}}}}"#
            ));
            let response: Response = client.receive();
            assert!(
                matches!(response, Response::Malformed { .. }),
                "{bad} was passed on: {response:?}"
            );
        }
        assert!(inbox.try_recv().is_err());
    }

    #[test]
    fn a_second_instance_on_the_same_socket_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let _first = served(&socket);

        let second = bind(&socket);

        assert!(matches!(second, Err(CoreError::AlreadyRunning { .. })));
    }

    #[test]
    fn a_socket_whose_folder_cant_be_made_fails_by_kind_with_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a file"), "").unwrap();
        let folder = dir.path().join("a file/Nightcap");

        let bound = bind(&folder.join("nightcap.sock"));

        assert!(
            matches!(
                &bound,
                Err(CoreError::Io { failure: IoFailure::NotAFolder, path })
                    if *path == folder.display().to_string()
            ),
            "{bound:?}"
        );
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
    fn queries_are_answered_in_the_order_asked_from_the_snapshot_without_the_mailbox() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (core, outcomes) = served_core(&socket);

        let response = ask(
            &socket,
            Request::Queries(vec![Query::Front, Query::NightShift]),
        );

        let Response::Ok(Payload::Answers(answers)) = response.join().unwrap() else {
            panic!("expected answers");
        };
        let [Answer::Front(front), Answer::NightShift(night_shift)] = answers.as_slice() else {
            panic!("expected what's in front, then Night Shift, got {answers:?}");
        };
        assert_eq!(
            **front,
            Front {
                app: None,
                watching: false,
                host: None,
                site: None,
                decision: Decision {
                    state: None,
                    place: Place::Everywhere,
                    reasons: Vec::new(),
                },
                places: Vec::new(),
                everywhere: PlaceInFront {
                    place: Place::Everywhere,
                    gives: OnOff::Off,
                    rules: Vec::new(),
                    base: None,
                    inherits: None,
                },
            }
        );
        assert_eq!(night_shift.state, NightShift::Unavailable);
        assert_eq!(*night_shift, core.snapshot().night_shift());
        assert!(
            outcomes.try_recv().is_err(),
            "a query reaches neither the mailbox nor the shell"
        );
    }

    #[test]
    fn a_snapshot_and_a_request_from_before_an_input_answer_as_it_was_and_after_it_the_change() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (core, outcomes) = served_core(&socket);
        let queries = || Request::Queries(vec![Query::Rules, Query::Checks]);
        let before = core.snapshot();
        let asked_before = ask(&socket, queries()).join().unwrap();

        core.send(Command::AddRule {
            identity: RuleIdentity::App("com.figma.Desktop".parse().unwrap()),
            says: OnOff::Off,
        });
        outcomes.recv_timeout(Duration::from_secs(5)).unwrap();
        let after = core.snapshot();
        let asked_after = ask(&socket, queries()).join().unwrap();

        assert_eq!(before.rules(), Vec::new());
        assert_eq!(after.rules().len(), 1);
        assert_ne!(before.checks(), after.checks());
        let answers = |snapshot: &Snapshot| {
            Response::Ok(Payload::Answers(vec![
                Answer::Rules(snapshot.rules()),
                Answer::Checks(snapshot.checks()),
            ]))
        };
        assert_eq!(asked_before, answers(&before));
        assert_eq!(asked_after, answers(&after));
    }

    #[test]
    fn a_cli_command_is_answered_once_decided_and_its_outcome_reaches_the_shell() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (core, outcomes) = served_core(&socket);

        let response = ask(&socket, Request::Command(Command::Websites(OnOff::On)));

        assert!(matches!(
            response.join().unwrap(),
            Response::Ok(Payload::Decision(_))
        ));
        assert_eq!(
            outcomes.recv_timeout(Duration::from_secs(5)).unwrap().0,
            Outcome::new(Cause::Command)
        );
        assert_eq!(
            core.snapshot().preferences().website_checking.state,
            OnOff::On
        );
    }

    #[test]
    fn terminating_removes_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (core, _outcomes) = served_core(&socket);

        assert_eq!(core.stop(), Outcome::new(Cause::Quit));

        assert!(!socket.exists());
    }
}
