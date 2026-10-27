//! What each input means for the core: one turn, deciding its outcome (ADR-0002).

use std::time::SystemTime;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::Command;

use crate::schedule::SunTimes;
use crate::state::Inner;
use crate::{Fact, Outcome, UiCommand};

/// Something that changes the core: a fact the platform reports, a command from the menu-bar
/// UI, or the app quitting. The CLI's requests are answered apart, since their response goes
/// back over the socket (ADR-0010).
#[derive(Debug)]
pub(crate) enum Input {
    Fact(Fact),
    /// A command the UI sent, which hears of a refusal as an event.
    Command(Command),
    UiCommand(UiCommand),
    /// The app is quitting, which releases every force (ADR-0008).
    Terminating,
}

/// The outcome `input` leads to. Only a fact can announce a problem: a command, and quitting,
/// are the user's own doing.
pub(crate) fn react(inner: &mut Inner, input: Input) -> Outcome {
    match input {
        Input::Fact(fact) => {
            inner
                .turn(true, |inner, outcome| inner.fact(fact, outcome))
                .0
        }
        Input::Command(command) => inner.choose(command).0,
        Input::UiCommand(command) => {
            inner
                .turn(false, |inner, outcome| inner.ui_command(command, outcome))
                .0
        }
        Input::Terminating => inner.turn(false, |inner, _| inner.terminating()).0,
    }
}

impl Inner {
    fn fact(&mut self, fact: Fact, outcome: &mut Outcome) {
        match fact {
            Fact::Launched => self.launched(outcome),
            Fact::AppActivated {
                bundle_id,
                name,
                is_browser,
            } => self.app_activated(bundle_id, name, is_browser),
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
            } => outcome.events.extend(self.status_changed(
                enabled,
                schedule,
                sun_schedule_set_aside,
            )),
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
            Fact::Woke => self.reconciling.woke(self.clock.now()),
        }
    }

    fn ui_command(&mut self, command: UiCommand, outcome: &mut Outcome) {
        match command {
            UiCommand::StartTour {
                sample,
                site,
                example,
            } => self.tour_started(&sample, site.as_ref(), &example),
            UiCommand::EndTour => self.tour_ended(self.clock.now()),
            UiCommand::FinishOnboarding { websites } => self.onboarding_finished(websites),
            UiCommand::Fix(fix) => self.run_fix(fix, &mut outcome.effects),
        }
    }

    /// Nightcap can't tell a restart from being removed, so it never leaves a force behind
    /// (ADR-0008), nor Night Shift on for a tour (ADR-0016).
    fn terminating(&mut self) {
        let now = self.clock.now();
        self.places.release(now);
        self.tour = None;
        if let Some(socket) = &self.socket
            && let Err(error) = std::fs::remove_file(socket)
        {
            tracing::warn!("couldn't remove {}: {error}", socket.display());
        }
    }
}
