//! The connection to the server: its socket, the Hello before each request, and the queries
//! the CLI asks.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use nightcap_protocol::{
    About, Answer, Checks, ConfigState, Front, Hello, HelloMessage, HelloReply, Instance,
    ListedRule, NightShiftState, Payload, Preferences, Query, Request, Response, Rules, TrialState,
};
use serde::Serialize;

use crate::{Failure, unexpected_reply};

/// How long the server may take to answer, including the core's turn at a command.
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
    match serde_json::from_str(&receive(&mut reader)?) {
        Ok(HelloReply::Hello(_)) => {}
        Ok(HelloReply::Mismatch(server)) => return Err(Failure::Mismatch(Some(server))),
        Ok(HelloReply::Malformed { message }) => return Err(Failure::Malformed(message)),
        // A Nightcap that speaks another protocol can answer in a shape this CLI can't read.
        Err(_) => return Err(Failure::Mismatch(None)),
    }
    send(&mut writer, request)?;
    let response = serde_json::from_str(&receive(&mut reader)?)
        .context("Nightcap sent a message this CLI doesn't understand")?;
    match response {
        Response::Ok(payload) => Ok(payload),
        Response::Refused(refusal) => Err(Failure::Refused(refusal)),
        Response::Malformed { message } => Err(Failure::Malformed(message)),
    }
}

/// Asks `query` alone, in the one request `send` sends, and gives its answer.
pub(crate) fn ask(
    send: impl FnOnce(&Request) -> Result<Payload, Failure>,
    query: Query,
) -> Result<Answer, Failure> {
    let Payload::Answers(answers) = send(&Request::Queries(vec![query]))? else {
        return Err(unexpected_reply());
    };
    let Ok([answer]) = <[Answer; 1]>::try_from(answers) else {
        return Err(unexpected_reply());
    };
    Ok(answer)
}

/// The apps' and sites' rules, each with its snooze, in the one request `send` sends.
pub(crate) fn rules(
    send: impl FnOnce(&Request) -> Result<Payload, Failure>,
) -> Result<Vec<ListedRule>, Failure> {
    let Answer::Rules(rules) = ask(send, Query::Rules)? else {
        return Err(unexpected_reply());
    };
    Ok(rules.rules)
}

/// What `nightcap status` shows, from the queries it asks in one request, so every answer is
/// as of the same input (ADR-0010).
#[derive(Debug)]
pub(crate) struct Report {
    pub(crate) about: About,
    pub(crate) night_shift: NightShiftState,
    pub(crate) front: Front,
    pub(crate) rules: Rules,
    pub(crate) config: ConfigState,
    pub(crate) preferences: Preferences,
    pub(crate) checks: Checks,
    pub(crate) trial: Option<TrialState>,
}

/// Asks what `nightcap status` shows, in the one request `ask` sends to the server.
pub(crate) fn report(
    ask: impl FnOnce(&Request) -> Result<Payload, Failure>,
) -> Result<Report, Failure> {
    let request = Request::Queries(vec![
        Query::About,
        Query::NightShift,
        Query::Front,
        Query::Rules,
        Query::Config,
        Query::Preferences,
        Query::Checks,
        Query::Trial,
    ]);
    let Payload::Answers(answers) = ask(&request)? else {
        return Err(unexpected_reply());
    };
    let Ok(
        [
            Answer::About(about),
            Answer::NightShift(night_shift),
            Answer::Front(front),
            Answer::Rules(rules),
            Answer::Config(config),
            Answer::Preferences(preferences),
            Answer::Checks(checks),
            Answer::Trial(trial),
        ],
    ) = <[Answer; 8]>::try_from(answers)
    else {
        return Err(unexpected_reply());
    };
    Ok(Report {
        about,
        night_shift,
        front: *front,
        rules,
        config: *config,
        preferences,
        checks,
        trial,
    })
}

/// Connects to the server, retrying for up to `wait` while it starts.
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

/// The next line Nightcap sends.
fn receive(reader: &mut impl BufRead) -> anyhow::Result<String> {
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .context("Nightcap didn't answer in time")?;
    anyhow::ensure!(read > 0, "Nightcap closed the connection without answering");
    Ok(line)
}

// Staged cores exist only in debug builds.
#[cfg(test)]
#[cfg(debug_assertions)]
pub(crate) mod tests {
    use nightcap_core::Scenario;

    use super::*;

    /// What `report` reads from `scenario`'s core, staged in memory, with each request carried
    /// as the socket carries it, from Nightcap 1.2.0.
    pub(crate) fn staged_report(scenario: Scenario) -> Report {
        let staged = scenario.stage().unwrap();
        let Ok(mut report) = report(|request| match staged.request(request.clone()) {
            (_, Response::Ok(payload)) => Ok(payload),
            (_, response) => panic!("expected answers, got {response:?}"),
        }) else {
            panic!("the status reads its answers");
        };
        report.about.version = "1.2.0".to_owned();
        report
    }
}
