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

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;

use crate::CoreError;
use crate::clock::Clock;
use mailbox::{Mail, Mailbox};
use messages::Listener;
use react::{Input, react};
use snapshot::{Published, Snapshot};
use state::Inner;

/// Starts the core's thread on `inner`, by `clock`, which publishes to `published`, tells
/// `listener`, and removes the socket at `socket` on the way out, and gives the mailbox it takes
/// its mail from.
pub(crate) fn spawn(
    inner: Inner,
    clock: Arc<dyn Clock>,
    socket: Option<PathBuf>,
    published: Arc<Published>,
    listener: Arc<dyn Listener>,
) -> Result<Mailbox, CoreError> {
    let (mailbox, inbox) = Mailbox::new();
    thread::Builder::new()
        .name("nightcap-core".to_owned())
        .spawn(move || {
            let server = Server {
                clock: clock.as_ref(),
                socket: socket.as_deref(),
                published: &published,
                listener: listener.as_ref(),
            };
            server.serve(inner, &inbox);
        })
        .map_err(|error| {
            tracing::error!("couldn't start the core's thread: {error}");
            CoreError::Thread {
                failure: error.kind().into(),
            }
        })?;
    Ok(mailbox)
}

/// What the core's thread runs by, beside the state it decides from.
struct Server<'a> {
    clock: &'a dyn Clock,
    /// The socket the CLI reaches the core at, which the server removes on the way out.
    socket: Option<&'a Path>,
    published: &'a Published,
    listener: &'a dyn Listener,
}

impl Server<'_> {
    /// Decides each piece of mail in turn, reading the clock once for each, until the app quits
    /// or every sender is gone. Each snapshot is published before the outcome or the answer goes
    /// out, so whoever hears of a change reads it, and the listener hears each outcome with the
    /// snapshot its input left.
    fn serve(&self, mut inner: Inner, inbox: &mpsc::Receiver<Mail>) {
        for mail in inbox {
            let now = self.clock.now();
            match mail {
                Mail::Input(input) => {
                    let (outcome, answer) = react(&mut inner, input, now);
                    let snapshot = self.published.publish(Snapshot::of(&inner, now));
                    if let Some((reply, answer)) = answer {
                        // The receiver is gone only if the client already hung up, and then no
                        // one is waiting.
                        let _ = reply.send(answer);
                    }
                    self.listener.outcome(outcome, snapshot);
                }
                Mail::Stop { reply } => {
                    let (outcome, _) = react(&mut inner, Input::Terminating, now);
                    self.published.publish(Snapshot::of(&inner, now));
                    if let Some(socket) = self.socket
                        && let Err(error) = std::fs::remove_file(socket)
                    {
                        tracing::warn!("couldn't remove {}: {error}", socket.display());
                    }
                    // The core stops whether or not anyone still waits for what it released.
                    let _ = reply.send(outcome);
                    return;
                }
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
        Command, Done, Install, NightShift, NotSelfManaged, OnOff, Refusal, RuleIdentity, Schedule,
        TimeOfDay,
    };

    use super::Listener;
    use super::mailbox::Mail;
    use super::react::{Input, ReplyTo};
    use crate::clock::{Clock, ManualClock};
    use crate::server::start::Setup;
    use crate::server::state::Inner;
    use crate::storage::file::{Memory, Paths};
    use crate::{
        AppCommand, Cause, Core, Effect, Fact, Outcome, Reply, Runs, Snapshot, SwitchedBy,
    };

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

    /// A core's state as it starts, and the clock it runs by.
    pub(crate) struct Stage {
        pub(crate) inner: Inner,
        pub(crate) clock: Arc<dyn Clock>,
    }

    impl Stage {
        pub(crate) fn now(&self) -> jiff::Timestamp {
            self.clock.now()
        }
    }

    /// A managed release in Zurich at `now`, on files in memory.
    fn stage_at(now: &str) -> Stage {
        let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(now.parse().unwrap()));
        let inner = Inner::start(Setup {
            install: Install::NotSelfManaged(NotSelfManaged::ManagedInstall),
            files: Arc::new(Memory::new(Paths::in_folder(Path::new("/nightcap")))),
            clock: clock.clone(),
            zone: TimeZone::get("Europe/Zurich").unwrap(),
        });
        Stage { inner, clock }
    }

    /// Noon, with nothing reported yet.
    pub(crate) fn noon() -> Stage {
        stage_at("2026-10-16T12:00:00+02:00")
    }

    /// A core on its own thread, with the outcomes it tells its listener.
    fn running(stage: Stage) -> (Core, mpsc::Receiver<Heard>) {
        let (listener, outcomes) = ChannelListener::new();
        (
            Core::run(stage.inner, stage.clock, None, Arc::new(listener)).unwrap(),
            outcomes,
        )
    }

    fn next(outcomes: &mpsc::Receiver<Heard>) -> Outcome {
        next_heard(outcomes).0
    }

    fn next_heard(outcomes: &mpsc::Receiver<Heard>) -> Heard {
        outcomes.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    /// Night Shift as CoreBrightness reports it, on custom hours from 22:00 to 07:00, with
    /// Nightcap's own write standing, if any.
    fn status(enabled: bool, written_at: Option<&str>) -> Fact {
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
            written_at: written_at.map(|at| at.parse::<jiff::Timestamp>().unwrap().into()),
            switched_by: if written_at.is_some() {
                SwitchedBy::Nightcap
            } else {
                SwitchedBy::Schedule
            },
        }
    }

    /// Night Shift as the schedule leaves it.
    fn reported(enabled: bool) -> Fact {
        status(enabled, None)
    }

    fn figma() -> Fact {
        Fact::AppActivated {
            bundle_id: Some("com.figma.Desktop".to_owned()),
            is_browser: false,
        }
    }

    #[test]
    fn the_listener_hears_each_outcome_in_the_order_the_inputs_arrived() {
        let (core, outcomes) = running(noon());

        core.report(reported(false));
        core.send(Command::On, None);
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
    fn each_command_from_the_app_is_replied_to_in_its_own_outcome_whatever_comes_before() {
        let (core, outcomes) = running(noon());

        core.report(reported(false));
        core.send(Command::On, Some(7));
        core.report(figma());
        core.send(Command::CheckForUpdates, Some(8));
        core.send(Command::Off, None);

        let replies: Vec<Option<(u64, Reply)>> = (0..5)
            .map(|_| {
                next(&outcomes)
                    .reply
                    .map(|replied| (replied.ticket, replied.reply))
            })
            .collect();
        assert!(matches!(
            replies.as_slice(),
            [
                None,
                Some((7, Reply::Done(Done::Decision(_)))),
                None,
                Some((
                    8,
                    Reply::Refused(Refusal::NoUpdater(NotSelfManaged::ManagedInstall))
                )),
                None,
            ]
        ));
    }

    #[test]
    fn each_outcome_says_what_its_input_was() {
        let (core, outcomes) = running(noon());
        let Runs::Thread(mailbox) = &core.runs else {
            panic!("a running core decides on its own thread");
        };
        let (reply, answered) = mpsc::channel();

        core.report(Fact::Launched);
        core.report(reported(false));
        core.send(Command::On, None);
        core.send_app(AppCommand::BeginTrial { stands_in: false }, None);
        mailbox
            .post(Mail::Input(Input::Command(
                Command::Off,
                ReplyTo::Cli(reply),
            )))
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
        let (core, outcomes) = running(noon());

        core.report(reported(false));
        core.send(Command::On, None);

        let (_, after_the_report) = next_heard(&outcomes);
        let (_, after_the_command) = next_heard(&outcomes);
        assert_eq!(after_the_report.night_shift().state, NightShift::Off);
        assert_eq!(after_the_report.front().everywhere.indefinite, None);
        assert!(after_the_command.front().everywhere.indefinite.is_some());
        assert!(Arc::ptr_eq(&after_the_command, &core.snapshot()));
    }

    #[test]
    fn a_read_after_an_outcome_sees_what_its_input_left() {
        let (core, outcomes) = running(noon());
        assert_eq!(core.snapshot().night_shift().state, NightShift::Unavailable);

        core.report(reported(false));
        next(&outcomes);

        assert_eq!(core.snapshot().night_shift().state, NightShift::Off);
    }

    #[test]
    fn stopping_decides_everything_sent_before_it_and_releases_every_force() {
        let (core, outcomes) = running(stage_at("2026-10-16T23:00:00+02:00"));
        core.report(reported(true));
        core.report(figma());
        core.send(
            Command::AddRule {
                identity: RuleIdentity::App("com.figma.Desktop".parse().unwrap()),
                says: OnOff::Off,
            },
            None,
        );
        // Nightcap's own write, landing.
        core.report(status(false, Some("2026-10-16T23:00:00+02:00")));

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
