//! The core as a server on its own thread (ADR-0002). Every fact and command, from the shell and
//! the CLI alike, goes into one mailbox, and the thread decides them one at a time in the order
//! they arrived: after each, it publishes a snapshot for the reads, then hands the outcome to the
//! shell's listener.

pub(crate) mod mailbox;
pub(crate) mod messages;
pub(crate) mod react;
pub(crate) mod snapshot;
pub(crate) mod start;
pub(crate) mod state;
pub(crate) mod turn;
pub(crate) mod watch;

use std::sync::{Arc, mpsc};
use std::thread;

use crate::CoreError;
use mailbox::{Mail, Mailbox};
use messages::Listener;
use react::{Input, react};
use snapshot::{Published, Snapshot};
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
/// reads it, and the listener hears each outcome with the snapshot its input left.
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
                let snapshot = published.publish(Snapshot::of(&inner));
                listener.outcome(outcome, snapshot);
            }
            Mail::Command { command, reply } => {
                let (outcome, answer) = inner.answer(&command);
                let snapshot = published.publish(Snapshot::of(&inner));
                // The receiver is gone only if the client already hung up, and then no one is
                // waiting.
                let _ = reply.send(answer);
                listener.outcome(outcome, snapshot);
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
    use nightcap_protocol::{
        Command, Install, Instance, NightShift, OnOff, RuleIdentity, Schedule, TimeOfDay,
    };

    use super::Listener;
    use super::mailbox::Mail;
    use crate::clock::ManualClock;
    use crate::server::start::Setup;
    use crate::server::state::Inner;
    use crate::storage::file::{Memory, Paths};
    use crate::{AppCommand, Cause, Core, Effect, Fact, Outcome, Runs, Snapshot};

    /// An outcome as the listener hears it, with the snapshot its input left.
    pub(crate) type Heard = (Outcome, Arc<Snapshot>);

    /// Stands in for the shell: every outcome arrives on a channel the test can wait on.
    pub(crate) struct ChannelListener(Mutex<mpsc::Sender<Heard>>);

    impl ChannelListener {
        pub(crate) fn new() -> (Self, mpsc::Receiver<Heard>) {
            let (sender, outcomes) = mpsc::channel();
            (Self(Mutex::new(sender)), outcomes)
        }
    }

    impl Listener for ChannelListener {
        fn outcome(&self, outcome: Outcome, snapshot: Arc<Snapshot>) {
            // A test that stopped listening has what it needed.
            let _ = self.0.lock().unwrap().send((outcome, snapshot));
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
    fn running(inner: Inner) -> (Core, mpsc::Receiver<Heard>) {
        let (listener, outcomes) = ChannelListener::new();
        (
            Core::run(inner, None, Arc::new(listener)).unwrap(),
            outcomes,
        )
    }

    fn next(outcomes: &mpsc::Receiver<Heard>) -> Outcome {
        next_heard(outcomes).0
    }

    fn next_heard(outcomes: &mpsc::Receiver<Heard>) -> Heard {
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
        assert_eq!(next(&outcomes), Outcome::new(Cause::Fact));
    }

    #[test]
    fn each_outcome_says_what_its_input_was() {
        let (core, outcomes) = running(noon(None));
        let Runs::Thread(mailbox) = &core.runs else {
            panic!("a running core decides on its own thread");
        };
        let (reply, answered) = mpsc::channel();

        core.report(Fact::Launched);
        core.report(reported(false));
        core.send(Command::On);
        core.send_app(AppCommand::BeginTrial { stands_in: false });
        mailbox
            .post(Mail::Command {
                command: Command::Off,
                reply,
            })
            .unwrap();

        let causes: Vec<Cause> = (0..5).map(|_| next(&outcomes).cause).collect();
        assert_eq!(
            causes,
            [
                Cause::Launch,
                Cause::Fact,
                Cause::Command,
                Cause::Command,
                Cause::Command
            ]
        );
        assert!(answered.recv().unwrap().is_ok());
        assert_eq!(core.stop().cause, Cause::Quit);
    }

    #[test]
    fn the_listener_hears_each_outcome_with_the_snapshot_its_input_left() {
        let (core, outcomes) = running(noon(None));

        core.report(reported(false));
        core.send(Command::On);

        let (_, after_the_report) = next_heard(&outcomes);
        let (_, after_the_command) = next_heard(&outcomes);
        assert_eq!(after_the_report.night_shift().state, NightShift::Off);
        assert_eq!(after_the_report.rules(), Vec::new());
        assert_eq!(after_the_command.rules().len(), 1);
        assert!(Arc::ptr_eq(&after_the_command, &core.snapshot()));
    }

    #[test]
    fn a_read_after_an_outcome_sees_what_its_input_left() {
        let (core, outcomes) = running(noon(None));
        assert_eq!(core.snapshot().night_shift().state, NightShift::Unavailable);

        core.report(reported(false));
        next(&outcomes);

        assert_eq!(core.snapshot().night_shift().state, NightShift::Off);
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
