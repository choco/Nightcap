use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::{Parser, Subcommand};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, Zoned};
use nightcap_protocol::{
    self as protocol, BundleId, CheckKind, CheckState, Decision, Domain, ErrorReply, Hello,
    HelloMessage, HelloReply, Install, Instance, Layer, Length, NightShift, NotificationPermission,
    OnOff, Payload, Place, PlaceLayers, Query, Reason, Request, Response, Rule, RuleIdentity,
    Schedule, Status, TimeOfDay,
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
    /// Turn Night Shift on. Alone, it's the switch: on everywhere, with the schedule if the
    /// schedule has it on, else until you turn it off. With a length, a time or --until-schedule,
    /// it's on that long, everywhere or in one app or on one site, over any rule there.
    On(Change),
    /// Turn Night Shift off, as `on` turns it on.
    Off(Change),
    /// Turn Night Shift the other way everywhere, until you turn it back.
    Toggle,
    /// Take off what you set for a place, everywhere unless you name an app or a site: its rule
    /// comes back, or for everywhere the schedule.
    Reset {
        #[command(flatten)]
        place: Where,
    },
    /// Show how warm Night Shift tints the display, or set it, from 0 to 100.
    Strength {
        /// 0 is the least warm, 100 the warmest.
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        value: Option<u8>,
    },
    /// Check for an update now; Nightcap shows what it finds and offers to install it.
    Update,
    /// Turn website checking on or off: domain rules need it, and it needs Accessibility.
    Websites {
        #[arg(value_enum)]
        state: Switch,
    },
    /// List, add or remove rules, or export the ad-hoc ones for the config.
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },
    /// Read the config again, after changing it.
    Reload {
        /// Keep trying this long for Nightcap to start, e.g. 5s.
        #[arg(long, value_parser = parse_duration)]
        wait: Option<Duration>,
    },
}

/// How long, and where: `on` or `off` alone is the switch, everywhere until turned back.
#[derive(clap::Args)]
struct Change {
    /// For this long, e.g. 30m, 1h or 1h30m.
    #[arg(long = "for", value_parser = parse_length, group = "how_long")]
    length: Option<SignedDuration>,
    /// Until this time of day, e.g. 23:30: today, or tomorrow once it has passed.
    #[arg(long, value_parser = parse_time_of_day, group = "how_long")]
    until: Option<jiff::civil::Time>,
    /// Until the schedule next turns it back: when it ends for an on, when it starts for an off.
    #[arg(long, group = "how_long")]
    until_schedule: bool,
    #[command(flatten)]
    place: Where,
}

/// An app or a site; everywhere when neither is given.
#[derive(clap::Args)]
struct Where {
    /// An app, by its bundle ID, e.g. com.figma.Desktop.
    #[arg(long, group = "where")]
    app: Option<BundleId>,
    /// A site and its subdomains, e.g. figma.com.
    #[arg(long, group = "where")]
    domain: Option<Domain>,
}

impl Where {
    fn place(&self) -> Place {
        match (&self.app, &self.domain) {
            (Some(bundle_id), _) => Place::App(bundle_id.clone()),
            (None, Some(domain)) => Place::Domain(domain.clone()),
            (None, None) => Place::Everywhere,
        }
    }
}

/// The command `on` or `off` sends, with the time read now in `zone`.
fn change(state: OnOff, change: &Change, now: &Zoned) -> Result<protocol::Command, String> {
    let place = change.place.place();
    let length = match (change.length, change.until, change.until_schedule) {
        (Some(length), _, _) => Length::For(length),
        (None, Some(time), _) => Length::Until(next_occurrence(time, now)?),
        (None, None, true) => Length::UntilScheduleTurns,
        (None, None, false) if place == Place::Everywhere => {
            return Ok(match state {
                OnOff::On => protocol::Command::On,
                OnOff::Off => protocol::Command::Off,
            });
        }
        (None, None, false) => {
            return Err(format!(
                "an app or a site needs a length: --for, --until or --until-schedule; for good, add a rule with `nightcap rules add {place}`"
            ));
        }
    };
    Ok(protocol::Command::Set {
        place,
        state,
        length,
    })
}

/// The next time the clock reads `time` after `now`: today, or tomorrow once it has passed.
fn next_occurrence(time: jiff::civil::Time, now: &Zoned) -> Result<Timestamp, String> {
    let today = now
        .with()
        .time(time)
        .build()
        .map_err(|error| error.to_string())?;
    let next = if today > *now {
        today
    } else {
        today
            .tomorrow()
            .map_err(|error| error.to_string())?
            .with()
            .time(time)
            .build()
            .map_err(|error| error.to_string())?
    };
    Ok(next.timestamp())
}

#[derive(Subcommand)]
enum RulesAction {
    /// Show every rule and where it comes from.
    List {
        /// Print the rules as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Add an ad-hoc rule, e.g. app:com.figma.Desktop or domain:figma.com, that turns Night Shift
    /// off there, or on with --on. It replaces your own rule for the same app or site.
    Add {
        identity: RuleIdentity,
        /// Keep Night Shift on there, even by day.
        #[arg(long)]
        on: bool,
    },
    /// Remove an ad-hoc rule; a config rule changes only in the config.
    Remove { identity: RuleIdentity },
    /// Print the ad-hoc rules, ready to paste into the config.
    Export {
        #[arg(long, value_enum)]
        format: Format,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Switch {
    On,
    Off,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Format {
    Nix,
    Toml,
}

/// A length such as 30m, 1h or 1h30m, longer than zero.
fn parse_length(text: &str) -> Result<SignedDuration, String> {
    let length: SignedDuration = text.parse().map_err(|error| format!("{error}"))?;
    if length <= SignedDuration::ZERO {
        return Err("the length must be longer than zero".to_owned());
    }
    Ok(length)
}

/// A time of day such as 23:30.
fn parse_time_of_day(text: &str) -> Result<jiff::civil::Time, String> {
    jiff::civil::Time::strptime("%H:%M", text)
        .map_err(|_| format!("{text:?} isn't a time of day like 23:30"))
}

fn parse_duration(text: &str) -> Result<Duration, String> {
    let duration: SignedDuration = text.parse().map_err(|error| format!("{error}"))?;
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
    // A reader that stops early, as `nightcap status | head -1` does, ends the CLI quietly, as it
    // would a C tool, rather than with a panic from `println!`.
    #[expect(
        unsafe_code,
        reason = "libc is the only way to restore SIGPIPE's default"
    )]
    // SAFETY: restoring a signal's default action, before any other thread exists.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
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
        Command::Rules { action } => return run_rules(instance, action),
        Command::Strength { value: None } => {
            let Payload::Status(status) = request(instance, &Request::Query(Query::Status), None)?
            else {
                return Err(unexpected_reply());
            };
            let Some(strength) = status.strength else {
                return Err(Failure::Rejected(ErrorReply {
                    code: protocol::ErrorCode::StrengthUnavailable,
                    message: "Night Shift's strength can't be read on this Mac".to_owned(),
                }));
            };
            println!("Strength: {strength}");
            return Ok(());
        }
        Command::Strength { value: Some(value) } => {
            let command = protocol::Command::Strength(value);
            let Payload::Decision(_) = request(instance, &Request::Command(command), None)? else {
                return Err(unexpected_reply());
            };
            println!("Strength set to {value}.");
            return Ok(());
        }
        Command::Update => {
            let Payload::UpdateCheckStarted = request(instance, &Request::Update, None)? else {
                return Err(unexpected_reply());
            };
            println!("Checking for updates; Nightcap shows what it finds.");
            return Ok(());
        }
        Command::On(details) => change(OnOff::On, &details, &Zoned::now()).map_err(rejected)?,
        Command::Off(details) => change(OnOff::Off, &details, &Zoned::now()).map_err(rejected)?,
        Command::Toggle => protocol::Command::Toggle,
        Command::Reset { place } => protocol::Command::Reset(place.place()),
        Command::Websites { state } => {
            let state = match state {
                Switch::On => OnOff::On,
                Switch::Off => OnOff::Off,
            };
            let command = protocol::Command::Websites(state);
            let Payload::Decision(_) = request(instance, &Request::Command(command), None)? else {
                return Err(unexpected_reply());
            };
            println!("Website checking is {state}.");
            return Ok(());
        }
    };
    let Payload::Decision(decision) = request(instance, &Request::Command(command.clone()), None)?
    else {
        return Err(unexpected_reply());
    };
    let zone = TimeZone::system();
    if let Some(line) = set_elsewhere(&command, &decision, &zone) {
        println!("{line}");
    }
    // The reply comes before the shell applies the change, so it names what the core decided
    // rather than Night Shift as last observed.
    println!("Night Shift {}", decision.describe(&zone));
    Ok(())
}

/// What a `Set` to a place that doesn't decide says it set: the decision is the front's.
fn set_elsewhere(
    command: &protocol::Command,
    decision: &Decision,
    zone: &TimeZone,
) -> Option<String> {
    match command {
        protocol::Command::Set {
            place,
            state,
            length,
        } if *place != decision.place => Some(format!(
            "Set {place} {state} {}.",
            length_text(length, zone)
        )),
        _ => None,
    }
}

/// A mistake in what was asked, said before reaching Nightcap: exit 2, as Nightcap's own
/// refusals do.
fn rejected(message: String) -> Failure {
    Failure::Rejected(ErrorReply {
        code: protocol::ErrorCode::BadRequest,
        message,
    })
}

fn run_rules(instance: Instance, action: RulesAction) -> Result<(), Failure> {
    let (command, done) = match action {
        RulesAction::List { json } => {
            let Payload::Rules(rules) = request(instance, &Request::Query(Query::Rules), None)?
            else {
                return Err(unexpected_reply());
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&rules).context("couldn't encode the rules")?
                );
            } else if rules.is_empty() {
                println!("No rules.");
            } else {
                for rule in &rules {
                    println!("{rule}");
                }
            }
            return Ok(());
        }
        RulesAction::Export { format } => {
            let format = match format {
                Format::Nix => protocol::ExportFormat::Nix,
                Format::Toml => protocol::ExportFormat::Toml,
            };
            let Payload::Export(text) =
                request(instance, &Request::Query(Query::Export(format)), None)?
            else {
                return Err(unexpected_reply());
            };
            print!("{text}");
            return Ok(());
        }
        RulesAction::Add { identity, on } => {
            let says = OnOff::from(on);
            let done = format!("Added {identity}, which turns Night Shift {says}.");
            (protocol::Command::AddRule { identity, says }, done)
        }
        RulesAction::Remove { identity } => {
            let done = format!("Removed {identity}.");
            (protocol::Command::RemoveRule(identity), done)
        }
    };
    let Payload::Decision(decision) = request(instance, &Request::Command(command), None)? else {
        return Err(unexpected_reply());
    };
    println!(
        "{done} Night Shift {}.",
        decision.describe(&TimeZone::system())
    );
    Ok(())
}

fn unexpected_reply() -> Failure {
    Failure::Unreachable(anyhow::anyhow!(
        "Nightcap sent a reply this CLI doesn't expect"
    ))
}

fn schedule_text(schedule: Schedule) -> String {
    match schedule {
        Schedule::Off => "off".to_owned(),
        Schedule::SunsetToSunrise => "sunset to sunrise".to_owned(),
        Schedule::Custom { from, to } => format!("{} to {}", time_of_day(from), time_of_day(to)),
    }
}

/// The config's value, when it sets one that differs from the Mac's (ADR-0014).
fn from_config<T: PartialEq + Copy>(
    config: Option<T>,
    mac: Option<T>,
    text: impl Fn(T) -> String,
) -> String {
    match config {
        Some(value) if Some(value) != mac => format!(" (the config says {})", text(value)),
        _ => String::new(),
    }
}

/// How long a change lasts, as `on` and `off` were told.
fn length_text(length: &Length, zone: &TimeZone) -> String {
    match length {
        Length::For(duration) => format!("for {duration:#}"),
        Length::Until(end) => format!("until {}", end.to_zoned(zone.clone()).strftime("%H:%M")),
        Length::UntilScheduleTurns => "until the schedule turns it back".to_owned(),
        Length::UntilTurnedBack => "until you turn it back".to_owned(),
    }
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
    let schedule = status.schedule.map_or("unknown".to_owned(), schedule_text);
    let scheduled = match (status.schedule, status.scheduled_state) {
        (Some(Schedule::Off), _) | (None, _) => String::new(),
        (Some(_), None) => ", waiting for sun times".to_owned(),
        (Some(_), Some(scheduled)) => {
            format!(", {}{}", scheduled, until(status.next_transition))
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
    let decided = status.decision.describe(&TimeZone::system());
    // A decision on a layer or a rule starts with the state it asks for, which needs saying again
    // only while Night Shift isn't there yet.
    let says_state = !matches!(
        status.decision.reasons.first(),
        None | Some(Reason::Schedule { .. })
    );
    let observed = match status.night_shift {
        NightShift::Unavailable => None,
        NightShift::On => Some(OnOff::On),
        NightShift::Off => Some(OnOff::Off),
    };
    if says_state
        && status
            .decision
            .state
            .is_some_and(|state| Some(state) == observed)
    {
        println!("Night Shift: {decided}");
    } else {
        println!("Night Shift: {night_shift}, {decided}");
    }
    println!(
        "Schedule: {schedule}{scheduled}{}",
        from_config(status.config_schedule, status.schedule, schedule_text)
    );
    if let Some(strength) = status.strength {
        println!(
            "Strength: {strength}{}",
            from_config(status.config_strength, status.strength, |value| value
                .to_string())
        );
    }
    let host = match (&status.host, status.watching) {
        (Some(host), _) => format!(", on {host}"),
        (None, true) => ", host unknown".to_owned(),
        (None, false) => String::new(),
    };
    println!(
        "Frontmost app: {}{host}",
        status.frontmost_app.as_deref().unwrap_or("unknown")
    );
    println!("Rules: {}", rules(&status.rules));
    let website_checking = status.website_checking;
    println!(
        "Website checking: {}{}",
        website_checking.state,
        if website_checking.from_config {
            " (set in the config)"
        } else {
            ""
        }
    );
    for layers in &status.layers {
        println!("{}", layered(layers));
    }
    println!("Accessibility: {accessibility}");
    println!("Notifications: {notifications}");
    let problems = needs_attention(status);
    if !problems.is_empty() {
        println!("Needs attention:");
        for problem in problems {
            println!("  {problem}");
        }
    }
}

/// Each drifted or broken check's line. Settings shows the config's path above its check, so
/// here the config's problem is its error, which names the file.
fn needs_attention(status: &Status) -> Vec<String> {
    status
        .checks
        .iter()
        .filter(|check| matches!(check.state, CheckState::Drifted | CheckState::Broken))
        .map(|check| match (check.kind, &status.config_error) {
            (CheckKind::ConfigFile, Some(error)) => format!("The config file is invalid: {error}"),
            _ => check.text.clone(),
        })
        .collect()
}

fn rules(rules: &[Rule]) -> String {
    if rules.is_empty() {
        return "none".to_owned();
    }
    rules
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A place's timer and hold, top first: "app:com.figma.Desktop: on until 07:00".
fn layered(layers: &PlaceLayers) -> String {
    let layer = |layer: &Layer| match layer.until {
        Some(end) => format!("{}{}", layer.state, until(Some(end))),
        None => format!("{} until you turn it {}", layer.state, layer.state.other()),
    };
    let parts: Vec<_> = [&layers.timer, &layers.hold]
        .into_iter()
        .flatten()
        .map(layer)
        .collect();
    let place = match &layers.place {
        Place::Everywhere => "Everywhere".to_owned(),
        place => place.to_string(),
    };
    format!("{place}: {}", parts.join(", then "))
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

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    /// The command a line of arguments sends, at 21:00 on 16 October in Zurich.
    fn sends(args: &[&str]) -> Result<protocol::Command, String> {
        let cli = Cli::try_parse_from(["nightcap"].iter().chain(args))
            .map_err(|error| error.to_string())?;
        let now = "2026-10-16T21:00:00+02:00[Europe/Zurich]".parse().unwrap();
        match cli.command {
            Command::On(details) => change(OnOff::On, &details, &now),
            Command::Off(details) => change(OnOff::Off, &details, &now),
            _ => panic!("expected on or off"),
        }
    }

    fn set(place: Place, state: OnOff, length: Length) -> Result<protocol::Command, String> {
        Ok(protocol::Command::Set {
            place,
            state,
            length,
        })
    }

    #[test]
    fn on_and_off_take_a_length_and_a_place() {
        assert_eq!(sends(&["on"]), Ok(protocol::Command::On));
        assert_eq!(sends(&["off"]), Ok(protocol::Command::Off));
        assert_eq!(
            sends(&["off", "--for", "45m"]),
            set(
                Place::Everywhere,
                OnOff::Off,
                Length::For(SignedDuration::from_mins(45))
            )
        );
        assert_eq!(
            sends(&["on", "--app", "com.figma.Desktop", "--until-schedule"]),
            set(
                Place::App("com.figma.Desktop".parse().unwrap()),
                OnOff::On,
                Length::UntilScheduleTurns
            )
        );
        assert_eq!(
            sends(&["on", "--domain", "figma.com", "--for", "1h"]),
            set(
                Place::Domain("figma.com".parse().unwrap()),
                OnOff::On,
                Length::For(SignedDuration::from_hours(1))
            )
        );
        assert!(sends(&["on", "--app", "com.figma.Desktop"]).is_err());
        assert!(sends(&["on", "--for", "1h", "--until-schedule"]).is_err());
        assert!(sends(&["on", "--app", "a", "--domain", "b", "--for", "1h"]).is_err());
        assert!(sends(&["on", "--app", "com.figma.Desktop ", "--for", "1h"]).is_err());
        assert!(sends(&["on", "--domain", "figma.com/file", "--for", "1h"]).is_err());
    }

    #[test]
    fn a_change_where_the_decision_is_says_only_the_decision_in_any_spelling() {
        let zone = TimeZone::get("Europe/Zurich").unwrap();
        let decided_at = |place: &str| Decision {
            state: Some(OnOff::On),
            place: serde_json::from_str(place).unwrap(),
            reasons: Vec::new(),
        };
        let figma = decided_at(r#"{"app":"com.figma.Desktop"}"#);
        let site = decided_at(r#"{"domain":"www.figma.com"}"#);
        let change = |args: &[&str]| sends(args).unwrap();

        let app = change(&["on", "--app", "com.figma.desktop", "--for", "1h"]);
        assert_eq!(set_elsewhere(&app, &figma, &zone), None);
        let domain = change(&["on", "--domain", "WWW.Figma.com", "--for", "1h"]);
        assert_eq!(set_elsewhere(&domain, &site, &zone), None);
        assert_eq!(
            set_elsewhere(&domain, &figma, &zone),
            Some("Set domain:www.figma.com on for 1h.".to_owned())
        );
    }

    #[test]
    fn until_a_time_of_day_means_its_next_occurrence() {
        let until = |time: &str| {
            sends(&["off", "--until", time]).map(|command| match command {
                protocol::Command::Set {
                    length: Length::Until(end),
                    ..
                } => end,
                other => panic!("expected a time, got {other:?}"),
            })
        };

        assert_eq!(
            until("23:30"),
            Ok("2026-10-16T23:30:00+02:00".parse().unwrap())
        );
        assert_eq!(
            until("07:00"),
            Ok("2026-10-17T07:00:00+02:00".parse().unwrap())
        );
        assert_eq!(
            until("21:00"),
            Ok("2026-10-17T21:00:00+02:00".parse().unwrap())
        );
        assert!(until("7am").is_err());
    }

    #[test]
    fn lengths_read_as_30m_1h_or_1h30m() {
        assert_eq!(parse_length("30m"), Ok(SignedDuration::from_mins(30)));
        assert_eq!(parse_length("1h"), Ok(SignedDuration::from_hours(1)));
        assert_eq!(parse_length("1h30m"), Ok(SignedDuration::from_mins(90)));
        assert!(parse_length("0m").is_err());
        assert!(parse_length("soon").is_err());
    }

    #[test]
    fn the_configs_problem_names_its_file_where_settings_shows_it_above() {
        let status: Status = serde_json::from_str(
            r#"{"version":"1.2.0","install":"self_managed","night_shift":"on","strength":60,"config_strength":null,"decision":{"state":"on","place":"everywhere","reasons":[]},"schedule":null,"config_schedule":null,"scheduled_state":null,"next_transition":null,"frontmost_app":null,"watching":false,"host":null,"rules":[],"config_error":"/Users/me/.config/nightcap/config.toml: line 4: unknown field `domian`, expected `app` or `domain`","ad_hoc_error":null,"website_checking":{"state":"on","from_config":false},"layers":[],"permissions":{"accessibility":false,"notifications":"granted"},"checks":[{"kind":"accessibility","state":"drifted","text":"Accessibility isn’t allowed."},{"kind":"notifications","state":"ok","text":"Allowed."},{"kind":"config_file","state":"broken","text":"Line 4: unknown field `domian`, expected `app` or `domain`."}]}"#,
        )
        .unwrap();

        assert_eq!(
            needs_attention(&status),
            [
                "Accessibility isn’t allowed.",
                "The config file is invalid: /Users/me/.config/nightcap/config.toml: line 4: unknown field `domian`, expected `app` or `domain`",
            ]
        );
    }
}
