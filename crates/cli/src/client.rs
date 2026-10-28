//! The connection to the daemon: its socket, the Hello before each request, and the queries
//! `nightcap status` asks.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use nightcap_protocol::{
    About, Checks, ConfigState, Front, Hello, HelloMessage, HelloReply, Instance, NightShiftState,
    Payload, Preferences, Query, Request, Response, Rule,
};
use serde::Serialize;

use crate::{Failure, unexpected_reply};

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

/// What `nightcap status` shows, from the queries it asks, one per connection; `--json` prints
/// it as it is.
#[derive(Serialize)]
pub(crate) struct Report {
    pub(crate) about: About,
    pub(crate) night_shift: NightShiftState,
    pub(crate) front: Front,
    pub(crate) rules: Vec<Rule>,
    pub(crate) config: ConfigState,
    pub(crate) preferences: Preferences,
    pub(crate) checks: Checks,
}

/// Asks the daemon what `nightcap status` shows.
pub(crate) fn report(instance: Instance) -> Result<Report, Failure> {
    let ask = |query| request(instance, &Request::Query(query), None);
    let Payload::About(about) = ask(Query::About)? else {
        return Err(unexpected_reply());
    };
    let Payload::NightShift(night_shift) = ask(Query::NightShift)? else {
        return Err(unexpected_reply());
    };
    let Payload::Front(front) = ask(Query::Front)? else {
        return Err(unexpected_reply());
    };
    let Payload::Rules(rules) = ask(Query::Rules)? else {
        return Err(unexpected_reply());
    };
    let Payload::Config(config) = ask(Query::Config)? else {
        return Err(unexpected_reply());
    };
    let Payload::Preferences(preferences) = ask(Query::Preferences)? else {
        return Err(unexpected_reply());
    };
    let Payload::Checks(checks) = ask(Query::Checks)? else {
        return Err(unexpected_reply());
    };
    Ok(Report {
        about,
        night_shift,
        front: *front,
        rules,
        config,
        preferences,
        checks,
    })
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

fn send(writer: &mut impl Write, message: &impl Serialize) -> anyhow::Result<()> {
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
