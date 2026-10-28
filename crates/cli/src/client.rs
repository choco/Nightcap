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
    ListedRule, NightShiftState, Payload, Preferences, Query, Request, Response, TrialState,
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

/// Every rule with its snooze, in the one request `send` sends.
pub(crate) fn rules(
    send: impl FnOnce(&Request) -> Result<Payload, Failure>,
) -> Result<Vec<ListedRule>, Failure> {
    let Answer::Rules(rules) = ask(send, Query::Rules)? else {
        return Err(unexpected_reply());
    };
    Ok(rules)
}

/// What `nightcap status` shows, from the queries it asks in one request, so every answer is
/// as of the same input (ADR-0010).
#[derive(Debug)]
pub(crate) struct Report {
    pub(crate) about: About,
    pub(crate) night_shift: NightShiftState,
    pub(crate) front: Front,
    pub(crate) rules: Vec<ListedRule>,
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

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value};

    use super::*;

    /// The answers to what `nightcap status` asks, each under its query's name.
    const ANSWERS: &str = concat!(
        r#"{"about":{"version":"1.2.0","install":"self_managed","manages_itself":true},"#,
        r#""night_shift":{"state":"on","strength":60,"schedule":{"custom":{"from":{"hour":22,"minute":0},"to":{"hour":7,"minute":0}}},"set_aside":false,"scheduled":"off","next_on":"2026-10-16T20:00:00Z","next_off":"2026-10-17T05:00:00Z"},"#,
        r#""front":{"app":"com.apple.Safari","watching":true,"host":"www.figma.com","site":"figma.com","decision":{"state":"off","place":{"domain":"figma.com"},"reasons":[{"rule":{"place":{"domain":"figma.com"},"says":"off","length":"indefinite","set_by":"config"}}]},"places":[],"everywhere":{"place":"everywhere","gives":"on","rules":[],"base":null,"inherits":null}},"#,
        r#""rules":[{"place":{"domain":"figma.com"},"says":"off","length":"indefinite","set_by":"config","snooze":null}],"#,
        r#""config":{"path":"/Users/me/.config/nightcap/config.toml","found":true,"error":null,"strength":40,"schedule":null,"websites":"on","lengths":["PT30M","PT1H30M"],"ad_hoc_error":null},"#,
        r#""preferences":{"website_checking":{"state":"on","from_config":true},"notifications_wanted":true,"open_at_login":false},"#,
        r#""checks":{"checks":[{"kind":"accessibility","detail":"not_allowed","state":"drifted"}],"permissions":{"accessibility":false,"notifications":"granted","login_item":"requires_approval"}},"#,
        r#""trial":{"stands_in":true}}"#,
    );

    #[test]
    fn status_asks_every_query_in_one_request_and_keeps_each_answer() {
        let answers: Value = serde_json::from_str(ANSWERS).unwrap();
        // The server's answer to a query, as `ANSWERS` has it under the query's name.
        let answer = |query: &Query| {
            let Value::String(name) = serde_json::to_value(query).unwrap() else {
                panic!("status asks no query with an argument, but asked {query:?}");
            };
            let answered = answers[name.as_str()].clone();
            serde_json::from_value::<Answer>(Value::Object(Map::from_iter([(name, answered)])))
                .unwrap()
        };
        let mut asked = Vec::new();

        let Ok(report) = report(|request| {
            asked.push(request.clone());
            let Request::Queries(queries) = request else {
                panic!("expected queries, got {request:?}");
            };
            Ok(Payload::Answers(queries.iter().map(answer).collect()))
        }) else {
            panic!("the status reads its answers");
        };

        assert_eq!(
            asked,
            vec![Request::Queries(vec![
                Query::About,
                Query::NightShift,
                Query::Front,
                Query::Rules,
                Query::Config,
                Query::Preferences,
                Query::Checks,
                Query::Trial,
            ])]
        );
        let Report {
            about,
            night_shift,
            front,
            rules,
            config,
            preferences,
            checks,
            trial,
        } = report;
        for (name, kept) in [
            ("about", serde_json::to_value(about)),
            ("night_shift", serde_json::to_value(night_shift)),
            ("front", serde_json::to_value(front)),
            ("rules", serde_json::to_value(rules)),
            ("config", serde_json::to_value(config)),
            ("preferences", serde_json::to_value(preferences)),
            ("checks", serde_json::to_value(checks)),
            ("trial", serde_json::to_value(trial)),
        ] {
            assert_eq!(kept.unwrap(), answers[name], "{name}");
        }
    }
}
