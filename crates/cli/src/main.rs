use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::{Parser, Subcommand};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{
    self as protocol, Decision, ErrorReply, Hello, HelloMessage, HelloReply, Install, Instance,
    NightShift, NotificationPermission, OnOff, Payload, Query, Reason, Request, Response, Rule,
    RuleSource, Schedule, Status, TimeOfDay, Verdict,
};

/// How long the daemon may take to answer, including its main thread's tick.
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Parser)]
#[command(
    name = "nightcap",
    version,
    about = "Control Nightcap from the command line"
)]
struct Cli {
    /// Talk to the dev build's daemon instead of the installed one.
    #[arg(long, global = true)]
    dev: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show what Nightcap is doing and why.
    Status {
        /// Print the status as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Turn Night Shift on until the next schedule transition.
    On,
    /// Turn Night Shift off until the next schedule transition.
    Off,
    /// Turn Night Shift on if it's off, or off if it's on, until the next schedule transition.
    Toggle,
    /// Read the config again, after changing it.
    Reload {
        /// Keep trying this long for Nightcap to start, e.g. 5s.
        #[arg(long, value_parser = parse_duration)]
        wait: Option<Duration>,
    },
}

fn parse_duration(text: &str) -> Result<Duration, String> {
    let duration: jiff::SignedDuration = text.parse().map_err(|error| format!("{error}"))?;
    Duration::try_from(duration).map_err(|error| format!("{error}"))
}

/// Why a command failed, which decides the exit code.
enum Failure {
    /// The daemon couldn't be reached or didn't answer: exit 1.
    Unreachable(anyhow::Error),
    /// The daemon answered and refused the request: exit 2.
    Rejected(ErrorReply),
}

impl From<anyhow::Error> for Failure {
    fn from(error: anyhow::Error) -> Self {
        Self::Unreachable(error)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let instance = if cli.dev {
        Instance::Dev
    } else {
        Instance::Release
    };
    match run(instance, cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Unreachable(error)) => {
            eprintln!("error: {error:#}");
            ExitCode::from(1)
        }
        Err(Failure::Rejected(error)) => {
            eprintln!("error: {}", error.message);
            ExitCode::from(2)
        }
    }
}

fn run(instance: Instance, command: Command) -> Result<(), Failure> {
    let command = match command {
        Command::Status { json } => {
            let Payload::Status(status) = request(instance, &Request::Query(Query::Status), None)?
            else {
                return Err(unexpected_reply());
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&status).context("couldn't encode the status")?
                );
            } else {
                print_status(&status);
            }
            return Ok(());
        }
        Command::Reload { wait } => {
            let Payload::Status(status) = request(instance, &Request::Reload, wait)? else {
                return Err(unexpected_reply());
            };
            println!("Reloaded the config. Rules: {}", rules(&status.rules));
            return Ok(());
        }
        Command::On => protocol::Command::On,
        Command::Off => protocol::Command::Off,
        Command::Toggle => protocol::Command::Toggle,
    };
    let Payload::Decision(decision) = request(instance, &Request::Command(command), None)? else {
        return Err(unexpected_reply());
    };
    // The reply comes before the shell applies the change, so it names what the core decided
    // rather than Night Shift as last observed.
    println!("Night Shift {}", describe(&decision));
    Ok(())
}

fn unexpected_reply() -> Failure {
    Failure::Unreachable(anyhow::anyhow!(
        "Nightcap sent a reply this CLI doesn't expect"
    ))
}

fn print_status(status: &Status) {
    let install = match status.install {
        Install::Managed => "managed by its installer",
        Install::SelfManaged => "self-managed",
    };
    let night_shift = match status.night_shift {
        NightShift::Unavailable => "unavailable",
        NightShift::On => "on",
        NightShift::Off => "off",
    };
    let schedule = match status.schedule {
        None => "unknown".to_owned(),
        Some(Schedule::Off) => "off".to_owned(),
        Some(Schedule::SunsetToSunrise) => "sunset to sunrise".to_owned(),
        Some(Schedule::Custom { from, to }) => {
            format!("{} to {}", time_of_day(from), time_of_day(to))
        }
    };
    let scheduled = match (status.schedule, status.scheduled_state) {
        (Some(Schedule::Off), _) | (None, _) => String::new(),
        (Some(_), None) => ", waiting for sun times".to_owned(),
        (Some(_), Some(scheduled)) => {
            format!(", {}{}", on_off(scheduled), until(status.next_transition))
        }
    };
    let accessibility = if status.permissions.accessibility {
        "granted"
    } else {
        "not granted"
    };
    let notifications = match status.permissions.notifications {
        NotificationPermission::NotRequested => "not requested",
        NotificationPermission::Denied => "denied",
        NotificationPermission::Granted => "granted",
    };
    println!("Nightcap {}, {install}", status.version);
    println!("Night Shift: {night_shift}, {}", describe(&status.decision));
    println!("Schedule: {schedule}{scheduled}");
    println!(
        "Frontmost app: {}",
        status.frontmost_app.as_deref().unwrap_or("unknown")
    );
    println!("Rules: {}", rules(&status.rules));
    if let Some(error) = &status.config_error {
        println!("Config: invalid. {error}");
    }
    println!("Accessibility: {accessibility}");
    println!("Notifications: {notifications}");
}

/// What the core decided and why, as it reads after "Night Shift".
fn describe(decision: &Decision) -> String {
    let forcing: Vec<_> = decision
        .reasons
        .iter()
        .filter_map(|reason| match reason {
            Reason::Rule(rule) => Some(rule_name(rule)),
            _ => None,
        })
        .collect();
    if decision.verdict == Verdict::ForceOff {
        return format!("forced off by {}", forcing.join(", "));
    }
    match decision.reasons.first() {
        Some(Reason::ManualState { state, until: end }) => {
            format!("turned {} by hand{}", on_off(*state), until(*end))
        }
        Some(Reason::Schedule { .. }) => "following the schedule".to_owned(),
        Some(Reason::Rule(_)) | None => "left alone".to_owned(),
    }
}

fn rules(rules: &[Rule]) -> String {
    if rules.is_empty() {
        return "none".to_owned();
    }
    rules.iter().map(rule_name).collect::<Vec<_>>().join(", ")
}

fn rule_name(rule: &Rule) -> String {
    let source = match rule.source {
        RuleSource::Config => "config",
    };
    format!("{} ({source} rule)", rule.identity)
}

fn on_off(state: OnOff) -> &'static str {
    match state {
        OnOff::On => "on",
        OnOff::Off => "off",
    }
}

/// " until 22:00" in local time, or nothing when no schedule transition is coming.
fn until(transition: Option<Timestamp>) -> String {
    transition.map_or_else(String::new, |transition| {
        format!(
            " until {}",
            transition.to_zoned(TimeZone::system()).strftime("%H:%M")
        )
    })
}

fn time_of_day(time: TimeOfDay) -> String {
    format!("{:02}:{:02}", time.hour, time.minute)
}

fn request(
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
    let deadline = Instant::now() + wait.unwrap_or_default();
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
