//! The connection to the daemon: its socket, and the Hello before each request.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use nightcap_protocol::{Hello, HelloMessage, HelloReply, Instance, Payload, Request, Response};

use crate::Failure;

/// How long the daemon may take to answer, including the core's turn at a command.
const TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn request(
    instance: Instance,
    request: &Request,
    wait: Option<Duration>,
) -> Result<Payload, Failure> {
    let socket = instance
        .socket_path()
        .context("couldn't locate the socket")?;
    let stream = connect(&socket, wait)?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .context("couldn't set a timeout on the socket")?;
    let mut reader = BufReader::new(stream.try_clone().context("couldn't read the socket")?);
    let mut writer = stream;

    send(
        &mut writer,
        &HelloMessage {
            hello: Hello::current(),
        },
    )?;
    if let HelloReply::Error(error) = receive(&mut reader)? {
        return Err(Failure::Rejected(error));
    }
    send(&mut writer, request)?;
    match receive(&mut reader)? {
        Response::Ok(payload) => Ok(payload),
        Response::Error(error) => Err(Failure::Rejected(error)),
    }
}

/// Connects to the daemon, retrying for up to `wait` while it starts.
fn connect(socket: &Path, wait: Option<Duration>) -> anyhow::Result<UnixStream> {
    let wait = wait.unwrap_or_default();
    let deadline = Instant::now()
        .checked_add(wait)
        .with_context(|| format!("can't wait {wait:?} for Nightcap: that's too long"))?;
    loop {
        match UnixStream::connect(socket) {
            Ok(stream) => return Ok(stream),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "Nightcap isn't running (nothing is listening at {})",
                        socket.display()
                    )
                });
            }
        }
    }
}

fn send(writer: &mut impl Write, message: &impl serde::Serialize) -> anyhow::Result<()> {
    serde_json::to_writer(&mut *writer, message).context("couldn't send to Nightcap")?;
    writer
        .write_all(b"\n")
        .and_then(|()| writer.flush())
        .context("couldn't send to Nightcap")
}

fn receive<T: serde::de::DeserializeOwned>(reader: &mut impl BufRead) -> anyhow::Result<T> {
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .context("Nightcap didn't answer in time")?;
    anyhow::ensure!(read > 0, "Nightcap closed the connection without answering");
    serde_json::from_str(&line).context("Nightcap sent a message this CLI doesn't understand")
}
