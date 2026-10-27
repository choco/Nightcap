//! The core as a server on its own thread (ADR-0002). Every fact and command, from the shell and
//! the CLI alike, goes into one mailbox, and the thread decides them one at a time in the order
//! they arrived: after each, it publishes a snapshot for the reads, then hands the outcome to the
//! shell's listener.

mod mailbox;
pub(crate) mod messages;
pub(crate) mod react;
mod snapshot;
pub(crate) mod start;
pub(crate) mod state;
pub(crate) mod turn;
pub(crate) mod watch;

use std::sync::{Arc, mpsc};
use std::thread;

pub(crate) use mailbox::{Mail, Mailbox};
pub(crate) use snapshot::{Published, Snapshot};

use crate::CoreError;
use messages::Listener;
use react::{Input, react};
use state::Inner;

/// Starts the core's thread on `inner`, which publishes to `published` and tells `listener`,
/// and gives the mailbox it takes its mail from.
pub(crate) fn spawn(
    inner: Inner,
    published: Arc<Published>,
    listener: Arc<dyn Listener>,
) -> Result<Mailbox, CoreError> {
    let (mailbox, inbox) = Mailbox::new();
    thread::Builder::new()
        .name("nightcap-core".to_owned())
        .spawn(move || serve(inner, &inbox, &published, listener.as_ref()))
        .map_err(|error| CoreError::Io {
            message: format!("couldn't start the core's thread: {error}"),
        })?;
    Ok(mailbox)
}

/// Decides each piece of mail in turn, until the app quits or every sender is gone. Each
/// snapshot is published before the outcome or the answer goes out, so whoever hears of a change
/// reads it.
fn serve(
    mut inner: Inner,
    inbox: &mpsc::Receiver<Mail>,
    published: &Published,
    listener: &dyn Listener,
) {
    for mail in inbox {
        match mail {
            Mail::Input(input) => {
                let outcome = react(&mut inner, input);
                published.publish(Snapshot::of(&inner));
                listener.outcome(outcome);
            }
            Mail::Request { request, reply } => {
                let (outcome, response) = inner.answer(&request);
                published.publish(Snapshot::of(&inner));
                // The receiver is gone only if the client already hung up, and then no one is
                // waiting.
                let _ = reply.send(response);
                listener.outcome(outcome);
            }
            Mail::Stop { reply } => {
                let outcome = react(&mut inner, Input::Terminating);
                published.publish(Snapshot::of(&inner));
                // The core stops whether or not anyone still waits for what it released.
                let _ = reply.send(outcome);
                return;
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    use jiff::tz::TimeZone;
    use nightcap_protocol::{Command, Install, Instance, OnOff, RuleIdentity, Schedule, TimeOfDay};

    use super::Listener;
    use crate::scenario::ManualClock;
    use crate::server::start::Setup;
    use crate::server::state::Inner;
    use crate::storage::file::{Memory, Paths};
    use crate::{Core, Effect, Fact, MenuIcon, Outcome};

    /// Stands in for the shell: every outcome arrives on a channel the test can wait on.
    pub(crate) struct ChannelListener(Mutex<mpsc::Sender<Outcome>>);

    impl ChannelListener {
        pub(crate) fn new() -> (Self, mpsc::Receiver<Outcome>) {
            let (sender, outcomes) = mpsc::channel();
            (Self(Mutex::new(sender)), outcomes)
        }
    }

    impl Listener for ChannelListener {
        fn outcome(&self, outcome: Outcome) {
            // A test that stopped listening has what it needed.
            let _ = self.0.lock().unwrap().send(outcome);
        }
    }

    /// A managed release in Zurich at `now`, on files in memory, serving `socket` if given.
    fn inner_at(now: &str, socket: Option<&Path>) -> Inner {
        Inner::start(Setup {
            instance: Instance::Release,
            install: Install::Managed,
            socket: socket.map(Path::to_owned),
            files: Arc::new(Memory::new(Paths::in_folder(Path::new("/nightcap")))),
            clock: Arc::new(ManualClock::new(now.parse().unwrap())),
            zone: TimeZone::get("Europe/Zurich").unwrap(),
        })
    }

    /// Noon, with nothing reported yet.
    pub(crate) fn noon(socket: Option<&Path>) -> Inner {
        inner_at("2026-10-16T12:00:00+02:00", socket)
    }

    /// A core on its own thread, with the outcomes it tells its listener.
    fn running(inner: Inner) -> (Core, mpsc::Receiver<Outcome>) {
        let (listener, outcomes) = ChannelListener::new();
        (
            Core::run(inner, None, Arc::new(listener)).unwrap(),
            outcomes,
        )
    }

    fn next(outcomes: &mpsc::Receiver<Outcome>) -> Outcome {
        outcomes.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    /// Night Shift as CoreBrightness reports it, on custom hours from 22:00 to 07:00.
    fn reported(enabled: bool) -> Fact {
        Fact::StatusChanged {
            enabled,
            schedule: Schedule::Custom {
                from: TimeOfDay {
                    hour: 22,
                    minute: 0,
                },
                to: TimeOfDay { hour: 7, minute: 0 },
            },
            sun_schedule_set_aside: false,
        }
    }

    fn figma() -> Fact {
        Fact::AppActivated {
            bundle_id: Some("com.figma.Desktop".to_owned()),
            name: Some("Figma".to_owned()),
            is_browser: false,
        }
    }

    #[test]
    fn the_listener_hears_each_outcome_in_the_order_the_inputs_arrived() {
        let (core, outcomes) = running(noon(None));

        core.report(reported(false));
        core.send(Command::On);
        core.report(reported(true));

        assert_eq!(
            next(&outcomes).effects,
            vec![Effect::ScheduleWakeup {
                at: "2026-10-16T22:00:00+02:00"
                    .parse::<jiff::Timestamp>()
                    .unwrap()
                    .into()
            }]
        );
        assert_eq!(
            next(&outcomes).effects,
            vec![Effect::SetNightShift { enabled: true }]
        );
        assert_eq!(next(&outcomes), Outcome::default());
    }

    #[test]
    fn a_read_after_an_outcome_sees_what_its_input_left() {
        let (core, outcomes) = running(noon(None));
        assert_eq!(core.menu().icon, MenuIcon::Unavailable);

        core.report(reported(false));
        next(&outcomes);

        assert_eq!(core.menu().icon, MenuIcon::Off);
    }

    #[test]
    fn stopping_decides_everything_sent_before_it_and_releases_every_force() {
        let (core, outcomes) = running(inner_at("2026-10-16T23:00:00+02:00", None));
        core.report(reported(true));
        core.report(figma());
        core.send(Command::AddRule {
            identity: RuleIdentity::App("com.figma.Desktop".parse().unwrap()),
            says: OnOff::Off,
        });
        core.report(reported(false));

        let released = core.stop();

        assert_eq!(
            released.effects,
            vec![Effect::SetNightShift { enabled: true }]
        );
        assert_eq!(outcomes.try_iter().count(), 4);
        core.report(reported(true));
        assert!(outcomes.recv_timeout(Duration::from_millis(100)).is_err());
    }
}
