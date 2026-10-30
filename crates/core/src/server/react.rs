//! What each input means for the core: one turn, deciding its outcome (ADR-0002).

use std::sync::mpsc;
use std::time::SystemTime;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{Command, Done, Refusal};

use crate::schedule::SunTimes;
use crate::server::messages::{Replied, Reply};
use crate::server::state::Inner;
use crate::{AppCommand, Cause, Effect, Fact, Outcome};

/// Where the CLI waits on a command's answer, which goes back over the socket (ADR-0010).
pub(crate) type Waiter = mpsc::Sender<Result<Done, Refusal>>;

/// The answer to a command the CLI sent, with where it waits.
pub(crate) type Answer = (Waiter, Result<Done, Refusal>);

/// Where a command's reply goes: the app's outcome, under the ticket it sent the command with,
/// if any, or the CLI's socket.
#[derive(Debug)]
pub(crate) enum ReplyTo {
    App(Option<u64>),
    Cli(Waiter),
}

/// Something that changes the core: a fact the platform reports, a command from the app or the
/// CLI, or the app quitting.
#[derive(Debug)]
pub(crate) enum Input {
    Fact(Fact),
    /// A command, decided the same whoever sent it, and where its reply goes.
    Command(Command, ReplyTo),
    /// A command only the app sends (ADR-0016), under the ticket it sent it with, if any.
    AppCommand(AppCommand, Option<u64>),
    /// The app is quitting, which releases every force (ADR-0008).
    Terminating,
}

/// The reply to a command of the app's that has a ticket.
fn replied(ticket: Option<u64>, answer: Result<Done, Refusal>) -> Option<Replied> {
    ticket.map(|ticket| Replied {
        ticket,
        reply: Reply::from(answer),
    })
}

/// One turn of the core, the same for every input whoever sent it, at `now`, the one time a
/// turn reads the clock (ADR-0002): the rules that ran out end and the schedule's transitions
/// apply, then the input, then the turn works out what the shell must do as the input left
/// things, and saves. Gives the outcome, and the answer the CLI waits on, to send once the
/// snapshot the input left is published.
pub(crate) fn react(inner: &mut Inner, input: Input, now: Timestamp) -> (Outcome, Option<Answer>) {
    inner.catch_up(now);
    let (cause, mut effects, reply, answer) = match input {
        Input::Fact(fact) => {
            let cause = match fact {
                Fact::Launched => Cause::Launch,
                _ => Cause::Fact,
            };
            (cause, inner.fact(fact, now), None, None)
        }
        Input::Command(command, ReplyTo::App(ticket)) => {
            let (effects, answer) = inner.command(&command, now);
            (Cause::Command, effects, replied(ticket, answer), None)
        }
        Input::Command(command, ReplyTo::Cli(waiter)) => {
            let (effects, answer) = inner.command(&command, now);
            (Cause::Command, effects, None, Some((waiter, answer)))
        }
        Input::AppCommand(command, ticket) => {
            let done = inner.app_command(command, now);
            (Cause::Command, Vec::new(), replied(ticket, Ok(done)), None)
        }
        Input::Terminating => {
            // Nightcap can't tell a restart from being removed, so it never leaves a force
            // behind (ADR-0008), nor Night Shift on for a trial's stand-in (ADR-0016).
            inner.rules.release(now);
            (Cause::Quit, Vec::new(), None, None)
        }
    };
    effects.extend(inner.rewatch());
    effects.extend(inner.enforce(now));
    effects.extend(inner.reschedule(now));
    inner.save(now);
    (
        Outcome {
            cause,
            effects,
            reply,
        },
        answer,
    )
}

impl Inner {
    /// What a fact changes, and the effects it calls for itself.
    fn fact(&mut self, fact: Fact, now: Timestamp) -> Vec<Effect> {
        match fact {
            Fact::Launched => return self.launched(),
            Fact::AppActivated {
                bundle_id,
                is_browser,
            } => self.app_activated(bundle_id, is_browser),
            Fact::HostChanged { generation, host } => self.host_changed(generation, host),
            Fact::LoginItemChanged { item } => self.setup.login_item_changed(item),
            Fact::PermissionsChanged {
                accessibility,
                notifications,
            } => self.setup.permissions_changed(accessibility, notifications),
            Fact::StatusChanged {
                enabled,
                schedule,
                sun_schedule_set_aside,
                written_at,
                switched_by,
            } => {
                // A shell bug, logged rather than a panic, which would stop the app (ADR-0002).
                let written_at = written_at.and_then(|at| {
                    Timestamp::try_from(at)
                        .inspect_err(|error| {
                            tracing::error!("the shell reported a write out of range: {error}");
                        })
                        .ok()
                });
                return self
                    .status_changed(
                        enabled,
                        schedule,
                        sun_schedule_set_aside,
                        written_at,
                        switched_by,
                        now,
                    )
                    .into_iter()
                    .collect();
            }
            Fact::StrengthChanged { value } => self.strength = Some(value),
            // The shell's timer is spent, whether or not what it waited for came due.
            Fact::TimerFired => self.wakeup = None,
            Fact::SunTimesChanged { sunrises, sunsets } => {
                let timestamps = |times: Vec<SystemTime>| {
                    times
                        .into_iter()
                        .map(Timestamp::try_from)
                        .collect::<Result<Vec<_>, _>>()
                };
                let sun_times = match (timestamps(sunrises), timestamps(sunsets)) {
                    (Ok(sunrises), Ok(sunsets)) => SunTimes::new(&sunrises, &sunsets),
                    // A shell bug, logged rather than a panic, which would stop the app
                    // (ADR-0002).
                    (Err(error), _) | (_, Err(error)) => {
                        tracing::error!("the shell reported a sun time out of range: {error}");
                        SunTimes::default()
                    }
                };
                self.facts.timeline.sun_times = sun_times;
            }
            Fact::TimeZoneChanged { iana_id } => match TimeZone::get(&iana_id) {
                Ok(zone) => {
                    tracing::info!("time zone changed to {iana_id}");
                    self.facts.timeline.zone = zone;
                }
                Err(error) => {
                    tracing::error!("keeping the old time zone: {iana_id} is unknown: {error}");
                }
            },
            Fact::SessionChanged { active } => self.facts.session_changed(active),
            Fact::Woke => tracing::debug!("woke"),
        }
        Vec::new()
    }

    /// Runs a command only the app sends, answering with the decision it leads to.
    fn app_command(&mut self, command: AppCommand, now: Timestamp) -> Done {
        match command {
            AppCommand::BeginTrial { stands_in } => self.rules.begin_trial(stands_in),
            AppCommand::EndTrial => self.rules.end_trial(now),
        }
        Done::Decision(self.rules.decide(&self.facts.world(now)))
    }
}
