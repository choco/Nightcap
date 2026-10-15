//! The IPC server thread (ADR-0013). It never touches the core's state: each request is
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
    let folder = path.parent().expect("the socket path has a folder");
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
    let (reply, response) = mpsc::channel();
    sender
        .send(Pending { request, reply })
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
