//! What each input means for the core: one turn, deciding its outcome (ADR-0002).

use std::sync::mpsc;
use std::time::SystemTime;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{Command, Payload, Refusal};

use crate::rules::schedule::SunTimes;
use crate::server::state::Inner;
use crate::{AppCommand, Cause, Fact, Outcome};

/// Where the CLI waits on a command's answer, which goes back over the socket (ADR-0010).
pub(crate) type Reply = mpsc::Sender<Result<Payload, Refusal>>;

/// The answer to a command the CLI sent, with where it waits.
pub(crate) type Answer = (Reply, Result<Payload, Refusal>);

/// Something that changes the core: a fact the platform reports, a command from the app or the
/// CLI, or the app quitting.
#[derive(Debug)]
pub(crate) enum Input {
    Fact(Fact),
    /// A command, and where the CLI waits on its answer; the app, which waits nowhere, hears of a
    /// refusal as an event.
    Command(Command, Option<Reply>),
    AppCommand(AppCommand),
    /// The app is quitting, which releases every force (ADR-0008).
    Terminating,
}

/// The outcome `input` leads to, which says what it answers, and the answer the CLI waits on,
/// to send once the snapshot `input` left is published.
pub(crate) fn react(inner: &mut Inner, input: Input) -> (Outcome, Option<Answer>) {
    let outcome = match input {
        Input::Fact(fact) => {
            let cause = match fact {
                Fact::Launched => Cause::Launch,
                _ => Cause::Fact,
            };
            inner
                .turn(cause, |inner, outcome| inner.fact(fact, outcome))
                .0
        }
        Input::Command(command, reply) => {
            let (outcome, answer) = inner.command(command, reply.is_some());
            return (outcome, reply.map(|reply| (reply, answer)));
        }
        Input::AppCommand(command) => {
            inner
                .turn(Cause::Command, |inner, _| inner.app_command(command))
                .0
        }
        Input::Terminating => inner.turn(Cause::Quit, |inner, _| inner.terminating()).0,
    };
    (outcome, None)
}

impl Inner {
    fn fact(&mut self, fact: Fact, outcome: &mut Outcome) {
        match fact {
            Fact::Launched => self.launched(&mut outcome.effects),
            Fact::AppActivated {
                bundle_id,
                is_browser,
            } => self.app_activated(bundle_id, is_browser),
            Fact::HostChanged { generation, host } => self.host_changed(generation, host),
            Fact::LoginItemChanged { item } => self.grants.login_item_changed(item),
            Fact::PermissionsChanged {
                accessibility,
                notifications,
            } => self
                .grants
                .permissions_changed(accessibility, notifications),
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
                outcome.events.extend(self.status_changed(
                    enabled,
                    schedule,
                    sun_schedule_set_aside,
                    written_at,
                    switched_by,
                ));
            }
            Fact::StrengthChanged { value } => self.strength = Some(value),
            Fact::TimerFired => self.reconciling.timer_fired(),
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
            Fact::SessionChanged { active } => self.reconciling.session_changed(active),
            Fact::Woke => tracing::debug!("woke"),
        }
    }

    fn app_command(&mut self, command: AppCommand) {
        match command {
            AppCommand::BeginTrial { stands_in } => self.places.begin_trial(stands_in),
            AppCommand::EndTrial => self.places.end_trial(self.clock.now()),
        }
    }

    /// Nightcap can't tell a restart from being removed, so it never leaves a force behind
    /// (ADR-0008), nor Night Shift on for a trial's stand-in (ADR-0016).
    fn terminating(&mut self) {
        let now = self.clock.now();
        self.places.release(now);
        if let Some(socket) = &self.socket
            && let Err(error) = std::fs::remove_file(socket)
        {
            tracing::warn!("couldn't remove {}: {error}", socket.display());
        }
    }
}
