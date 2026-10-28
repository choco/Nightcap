mod args;
mod client;
mod json;
mod wording;

use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use nightcap_protocol::{
    self as protocol, Answer, Decision, Hello, Instance, Lasting, OnOff, Payload, Place, Query,
    Refusal, Request,
};

use args::{Change, Cli, Command, Format, RulesAction, Switch};
use client::{ask, request};

/// The command `on` or `off` sends, with the time read now in `zone`.
fn change(state: OnOff, change: &Change, now: &Zoned) -> Result<protocol::Command, String> {
    let place = change.place.place();
    let length = match (change.length, change.until, change.until_schedule) {
        (Some(length), _, _) => Lasting::For(length),
        (None, Some(time), _) => Lasting::Until(next_occurrence(time, now)?),
        (None, None, true) => Lasting::UntilScheduleTurns,
        (None, None, false) if place == Place::Everywhere => {
            return Ok(match state {
                OnOff::On => protocol::Command::On,
                OnOff::Off => protocol::Command::Off,
            });
        }
        (None, None, false) => return Err(wording::needs_a_length(&place)),
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

/// Why a command failed, which decides the exit code: 1 where Nightcap couldn't be reached, else
/// 2.
enum Failure {
    /// The server couldn't be reached or didn't answer.
    Unreachable(anyhow::Error),
    /// The server answered and refused the request.
    Refused(Refusal),
    /// The server speaks another protocol: its hello, where this CLI could read it.
    Mismatch(Option<Hello>),
    /// The server couldn't read what this CLI sent: serde's reason.
    Malformed(String),
    /// A mistake in what was asked, found before reaching Nightcap.
    Mistaken(String),
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
        Err(Failure::Refused(refusal)) => refused(&wording::refusal(&refusal)),
        Err(Failure::Mismatch(server)) => refused(&wording::mismatch(server.as_ref())),
        Err(Failure::Malformed(message)) => refused(&wording::malformed(&message)),
        Err(Failure::Mistaken(message)) => refused(&message),
    }
}

/// Says why Nightcap, or this CLI before it, refused what was asked: exit 2.
fn refused(why: &str) -> ExitCode {
    eprintln!("error: {why}");
    ExitCode::from(2)
}

fn run(instance: Instance, command: Command) -> Result<(), Failure> {
    let send = |sent: &Request| request(instance, sent, None);
    let command = match command {
        Command::Status { json: as_json } => {
            let report = client::report(send)?;
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string(&json::Status::from(&report))
                        .context("couldn't encode the status")?
                );
            } else {
                for line in wording::status(&report, &TimeZone::system()) {
                    println!("{line}");
                }
            }
            return Ok(());
        }
        Command::Reload { wait } => {
            let reload = Request::Command(protocol::Command::Reload);
            let Payload::Rules(rules) = request(instance, &reload, wait)? else {
                return Err(unexpected_reply());
            };
            println!("{}", wording::reloaded(&rules, &TimeZone::system()));
            return Ok(());
        }
        Command::Rules {
            action: RulesAction::List { json: as_json },
        } => {
            let rules = client::rules(send)?;
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string(&json::Rules::from(&rules[..]))
                        .context("couldn't encode the rules")?
                );
                return Ok(());
            }
            if rules.is_empty() {
                println!("{}", wording::no_rules());
            } else {
                let zone = TimeZone::system();
                for rule in &rules {
                    println!("{}", wording::rule(rule, &zone));
                }
            }
            return Ok(());
        }
        Command::Rules {
            action: RulesAction::Export { format },
        } => {
            let format = match format {
                Format::Nix => protocol::ExportFormat::Nix,
                Format::Toml => protocol::ExportFormat::Toml,
            };
            let Answer::Export(text) = ask(send, Query::Export(format))? else {
                return Err(unexpected_reply());
            };
            print!("{text}");
            return Ok(());
        }
        Command::Rules {
            action: RulesAction::Add { identity, on },
        } => protocol::Command::AddRule {
            identity,
            says: OnOff::from(on),
        },
        Command::Rules {
            action: RulesAction::Remove { identity },
        } => protocol::Command::RemoveRule(identity),
        Command::Strength { value: None } => {
            let Answer::NightShift(night_shift) = ask(send, Query::NightShift)? else {
                return Err(unexpected_reply());
            };
            let Some(strength) = night_shift.strength else {
                return Err(Failure::Refused(Refusal::StrengthUnavailable));
            };
            println!("{}", wording::strength(strength));
            return Ok(());
        }
        Command::Strength { value: Some(value) } => {
            let command = protocol::Command::Strength(value);
            let Payload::Decision(_) = send(&Request::Command(command))? else {
                return Err(unexpected_reply());
            };
            println!("{}", wording::strength_set(value));
            return Ok(());
        }
        Command::Update => {
            let check = Request::Command(protocol::Command::CheckForUpdates);
            let Payload::UpdateCheckStarted = send(&check)? else {
                return Err(unexpected_reply());
            };
            println!("{}", wording::checking_for_updates());
            return Ok(());
        }
        Command::On(details) => {
            change(OnOff::On, &details, &Zoned::now()).map_err(Failure::Mistaken)?
        }
        Command::Off(details) => {
            change(OnOff::Off, &details, &Zoned::now()).map_err(Failure::Mistaken)?
        }
        Command::Toggle => protocol::Command::Toggle,
        Command::Reset { place } => protocol::Command::Reset(place.place()),
        Command::Websites { state } => {
            let state = match state {
                Switch::On => OnOff::On,
                Switch::Off => OnOff::Off,
            };
            let command = protocol::Command::Websites(state);
            let Payload::Decision(_) = send(&Request::Command(command))? else {
                return Err(unexpected_reply());
            };
            println!("{}", wording::website_checking_set(state));
            return Ok(());
        }
    };
    for line in commanded(&command, send, &TimeZone::system())? {
        println!("{line}");
    }
    Ok(())
}

/// What `command`, one that can change what decides, prints once Nightcap has carried it out,
/// with `send` sending each request: what it did, where the decision doesn't say so, then what
/// Night Shift does after it. The reply comes before the shell applies the change, so it names
/// what the core decided rather than Night Shift as last observed. It carries the decision
/// alone, so the deciding rule's snooze comes from the rules, asked in a second request: the core
/// publishes the snapshot a command leaves before it replies, so they answer as of the command
/// or later.
fn commanded(
    command: &protocol::Command,
    mut send: impl FnMut(&Request) -> Result<Payload, Failure>,
    zone: &TimeZone,
) -> Result<Vec<String>, Failure> {
    let (done, decision) = match (command, send(&Request::Command(command.clone()))?) {
        (protocol::Command::AddRule { identity, says }, Payload::RuleAdded { rule, decision }) => {
            (Some(wording::rule_added(identity, *says, rule)), decision)
        }
        (protocol::Command::RemoveRule(identity), Payload::Decision(decision)) => {
            (Some(wording::rule_removed(identity)), decision)
        }
        (protocol::Command::AddRule { .. } | protocol::Command::RemoveRule(_), _) => {
            return Err(unexpected_reply());
        }
        (_, Payload::Decision(decision)) => (None, decision),
        _ => return Err(unexpected_reply()),
    };
    let rules = client::rules(send)?;
    Ok(match done {
        Some(done) => vec![wording::rules_changed(&done, &decision, &rules, zone)],
        None => set_elsewhere(command, &decision, zone)
            .into_iter()
            .chain([wording::decided(&decision, &rules, zone)])
            .collect(),
    })
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
        } if *place != decision.place => Some(wording::set(place, *state, length, zone)),
        _ => None,
    }
}

pub(crate) fn unexpected_reply() -> Failure {
    Failure::Unreachable(anyhow::Error::msg(wording::unexpected_reply()))
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use jiff::SignedDuration;

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

    fn set(place: Place, state: OnOff, length: Lasting) -> Result<protocol::Command, String> {
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
                Lasting::For(SignedDuration::from_mins(45))
            )
        );
        assert_eq!(
            sends(&["on", "--app", "com.figma.Desktop", "--until-schedule"]),
            set(
                Place::App("com.figma.Desktop".parse().unwrap()),
                OnOff::On,
                Lasting::UntilScheduleTurns
            )
        );
        assert_eq!(
            sends(&["on", "--domain", "figma.com", "--for", "1h"]),
            set(
                Place::Domain("figma.com".parse().unwrap()),
                OnOff::On,
                Lasting::For(SignedDuration::from_hours(1))
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
                    length: Lasting::Until(end),
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

    /// What `command` prints, each request carried to `staged` as the socket carries it, with
    /// times in Zurich.
    #[cfg(debug_assertions)]
    fn printed(staged: &nightcap_core::Staged, command: protocol::Command) -> Vec<String> {
        let zone = TimeZone::get("Europe/Zurich").unwrap();
        let send = |sent: &Request| match staged.request(sent.clone()) {
            (_, protocol::Response::Ok(payload)) => Ok(payload),
            (_, response) => panic!("expected an answer, got {response:?}"),
        };
        let Ok(lines) = commanded(&command, send, &zone) else {
            panic!("{command:?} printed nothing");
        };
        lines
    }

    // Staged cores exist only in debug builds.
    #[cfg(debug_assertions)]
    #[test]
    fn a_command_says_the_deciding_rule_is_snoozed_as_the_rules_answer_serves_it() {
        use nightcap_core::{Fact, Scenario};
        use protocol::{Schedule, TimeOfDay};

        let figma = || Place::App("com.figma.Desktop".parse().unwrap());
        let staged = Scenario::at("2026-10-16T23:00:00+02:00".parse().unwrap())
            .config("[rules.off]\napp = [\"com.figma.Desktop\"]\n")
            .then(Fact::StatusChanged {
                enabled: true,
                schedule: Schedule::Custom {
                    from: TimeOfDay {
                        hour: 22,
                        minute: 0,
                    },
                    to: TimeOfDay { hour: 7, minute: 0 },
                },
                sun_schedule_set_aside: false,
            })
            .then(Fact::AppActivated {
                bundle_id: Some("com.figma.Desktop".to_owned()),
                is_browser: false,
            })
            .then(protocol::Command::Set {
                place: figma(),
                state: OnOff::On,
                length: Lasting::For(SignedDuration::from_mins(30)),
            })
            .stage()
            .unwrap();
        let snoozed =
            "Night Shift on at app:com.figma.Desktop until 23:30, its config rule snoozed";

        assert_eq!(printed(&staged, protocol::Command::Off), [snoozed]);
        assert_eq!(printed(&staged, protocol::Command::Toggle), [snoozed]);
        assert_eq!(
            printed(
                &staged,
                protocol::Command::AddRule {
                    identity: "domain:youtube.com".parse().unwrap(),
                    says: OnOff::Off,
                }
            ),
            [format!(
                "Added domain:youtube.com, which turns Night Shift off. {snoozed}."
            )]
        );
        assert_eq!(
            printed(&staged, protocol::Command::Reset(figma())),
            ["Night Shift off by the config rule for app:com.figma.Desktop"]
        );
    }
}
