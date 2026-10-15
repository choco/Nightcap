use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{
    self as protocol, ErrorReply, Hello, HelloMessage, HelloReply, Install, Instance, NightShift,
    NotificationPermission, OnOff, Payload, Query, Request, Response, Schedule, Status, TimeOfDay,
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
    let (to_send, json) = match command {
        Command::Status { json } => (Request::Query(Query::Status), json),
        Command::On => (Request::Command(protocol::Command::On), false),
        Command::Off => (Request::Command(protocol::Command::Off), false),
        Command::Toggle => (Request::Command(protocol::Command::Toggle), false),
    };
    let is_command = matches!(to_send, Request::Command(_));
    let Payload::Status(status) = request(instance, &to_send)?;
    if json {
        println!(
            "{}",
            serde_json::to_string(&status).context("couldn't encode the status")?
        );
    } else if is_command {
        // The reply comes before the shell applies the change, so it names what was asked
        // rather than Night Shift as last observed.
        match status.manual_state {
            Some(manual) => println!(
                "Night Shift turned {}{}",
                on_off(manual),
                until(status.next_transition)
            ),
            None => print_status(&status),
        }
    } else {
        print_status(&status);
    }
    Ok(())
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
    let manual = match status.manual_state {
        None => String::new(),
        Some(manual) => format!(
            ", turned {} by hand{}",
            on_off(manual),
            until(status.next_transition)
        ),
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
    println!("Night Shift: {night_shift}{manual}");
    println!("Schedule: {schedule}{scheduled}");
    println!("Accessibility: {accessibility}");
    println!("Notifications: {notifications}");
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

fn request(instance: Instance, request: &Request) -> Result<Payload, Failure> {
    let socket = instance
        .socket_path()
        .context("couldn't locate the socket")?;
    let stream = UnixStream::connect(&socket).with_context(|| {
        format!(
            "Nightcap isn't running (nothing is listening at {})",
            socket.display()
        )
    })?;
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
