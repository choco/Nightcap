use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    Command, Decision, ErrorCode, Hello, HelloMessage, HelloReply, Install, Instance, NightShift,
    NotificationPermission, OnOff, Payload, Permissions, Query, Reason, Request, Response, Rule,
    RuleIdentity, RuleSource, Schedule, Status, TimeOfDay, Verdict,
};

use crate::state::{Persisted, Store};
use crate::{Clock, Core, CoreError, Effect, Event, Setup, Waker};

/// Stands in for the shell: every wake is reported on a channel the test can wait on.
struct ChannelWaker(Mutex<mpsc::Sender<()>>);

impl Waker for ChannelWaker {
    fn wake(&self) {
        self.0.lock().unwrap().send(()).unwrap();
    }
}

/// A clock the test moves by hand.
struct TestClock(Mutex<Timestamp>);

impl TestClock {
    fn at(instant: &str) -> Arc<Self> {
        Arc::new(Self(Mutex::new(at(instant))))
    }

    fn set(&self, instant: &str) {
        *self.0.lock().unwrap() = at(instant);
    }
}

impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        *self.0.lock().unwrap()
    }
}

fn at(instant: &str) -> Timestamp {
    instant.parse().unwrap()
}

/// 22:00 to 07:00, local time.
fn custom_hours() -> Schedule {
    Schedule::Custom {
        from: TimeOfDay {
            hour: 22,
            minute: 0,
        },
        to: TimeOfDay { hour: 7, minute: 0 },
    }
}

fn start(socket: &Path) -> (Core, mpsc::Receiver<()>) {
    start_as(socket, Instance::Release, Install::Managed)
}

fn start_as(socket: &Path, instance: Instance, install: Install) -> (Core, mpsc::Receiver<()>) {
    start_with(setup(socket, instance, install))
}

fn start_with(setup: Setup) -> (Core, mpsc::Receiver<()>) {
    let (sender, wakes) = mpsc::channel();
    let waker = Arc::new(ChannelWaker(Mutex::new(sender)));
    let core = Core::start(setup, waker).unwrap();
    (core, wakes)
}

/// Starts a release core in Zurich whose clock reads `now` until the test moves it.
fn start_at(socket: &Path, now: &str) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    let clock = TestClock::at(now);
    let (core, wakes) = start_with(Setup {
        clock: clock.clone(),
        ..setup(socket, Instance::Release, Install::Managed)
    });
    (core, wakes, clock)
}

/// Starts a second core on the first one's state and config, as launchd would after a restart
/// or a crash. The first core still holds its socket, so the second gets its own.
fn restart_at(socket: &Path, now: &str) -> (Core, PathBuf, mpsc::Receiver<()>, Arc<TestClock>) {
    let clock = TestClock::at(now);
    let restarted = socket
        .parent()
        .unwrap()
        .with_file_name("restarted")
        .join("nightcap.sock");
    let (core, wakes) = start_with(Setup {
        socket: restarted.clone(),
        clock: clock.clone(),
        ..setup(socket, Instance::Release, Install::Managed)
    });
    (core, restarted, wakes, clock)
}

/// Keeps the state file beside the socket, inside the test's temporary folder.
fn setup(socket: &Path, instance: Instance, install: Install) -> Setup {
    Setup {
        instance,
        install,
        socket: socket.to_owned(),
        state_file: socket.with_file_name("state.json"),
        config_file: config_beside(socket),
        clock: TestClock::at("2026-10-16T12:00:00+02:00"),
        zone: TimeZone::get("Europe/Zurich").unwrap(),
    }
}

fn socket_in(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("instance").join("nightcap.sock")
}

/// The config sits outside the socket's folder, which the core creates private at start.
fn config_beside(socket: &Path) -> PathBuf {
    socket.parent().unwrap().with_file_name("config.toml")
}

fn write_config(socket: &Path, contents: &str) {
    std::fs::write(config_beside(socket), contents).unwrap();
}

fn app_rule(bundle_id: &str) -> Rule {
    Rule {
        identity: RuleIdentity::App(bundle_id.to_owned()),
        source: RuleSource::Config,
    }
}

/// Starts at 23:00 with Night Shift on by the custom hours, then brings Figma, which a config
/// rule forces off, to the front, and reports the result.
fn start_forced(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    write_config(socket, "[rules]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, clock) = start_at(socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    assert_eq!(
        activate(&core, "com.figma.Desktop"),
        vec![Effect::SetNightShift { enabled: false }]
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    (core, wakes, clock)
}

fn figma() -> RuleIdentity {
    RuleIdentity::App("com.figma.Desktop".to_owned())
}

fn activate(core: &Core, bundle_id: &str) -> Vec<Effect> {
    core.handle(Event::AppActivated {
        bundle_id: Some(bundle_id.to_owned()),
    })
}

/// Speaks the wire protocol from the client side: a hello, then at most one request.
struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Client {
    fn connect(socket: &Path) -> Self {
        let stream = UnixStream::connect(socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        Self {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
        }
    }

    fn send(&mut self, line: &str) {
        writeln!(self.writer, "{line}").unwrap();
    }

    fn receive<T: serde::de::DeserializeOwned>(&mut self) -> T {
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn hello(&mut self, protocol: u32) -> HelloReply {
        let hello = HelloMessage {
            hello: Hello {
                protocol,
                version: "test".to_owned(),
            },
        };
        self.send(&serde_json::to_string(&hello).unwrap());
        self.receive()
    }
}

fn query_status(core: &Core, socket: &Path, wakes: &mpsc::Receiver<()>) -> Response {
    let socket = socket.to_owned();
    let client = thread::spawn(move || {
        let mut client = Client::connect(&socket);
        assert_eq!(client.hello(1), HelloReply::Hello(Hello::current()));
        client.send(&serde_json::to_string(&Request::Query(Query::Status)).unwrap());
        client.receive::<Response>()
    });
    wakes
        .recv_timeout(Duration::from_secs(5))
        .expect("the IPC thread asks for a tick");
    assert_eq!(core.tick(), Vec::new());
    client.join().unwrap()
}

/// Sends a command the way the CLI does, and returns the tick's effects and the response.
fn send(
    core: &Core,
    socket: &Path,
    wakes: &mpsc::Receiver<()>,
    command: Command,
) -> (Vec<Effect>, Response) {
    exchange(core, socket, wakes, Request::Command(command))
}

/// Sends a request the way the CLI does, and returns the tick's effects and the response.
fn exchange(
    core: &Core,
    socket: &Path,
    wakes: &mpsc::Receiver<()>,
    request: Request,
) -> (Vec<Effect>, Response) {
    let socket = socket.to_owned();
    let client = thread::spawn(move || {
        let mut client = Client::connect(&socket);
        assert_eq!(client.hello(1), HelloReply::Hello(Hello::current()));
        client.send(&serde_json::to_string(&request).unwrap());
        client.receive::<Response>()
    });
    wakes
        .recv_timeout(Duration::from_secs(5))
        .expect("the IPC thread asks for a tick");
    let effects = core.tick();
    (effects, client.join().unwrap())
}

/// The answer to a command with no force and no schedule: hands off, following the manual
/// state for good.
fn by_hand(state: OnOff) -> Response {
    Response::Ok(Payload::Decision(Decision {
        verdict: Verdict::HandsOff,
        reasons: vec![Reason::ManualState { state, until: None }],
    }))
}

fn status_of(core: &Core, socket: &Path, wakes: &mpsc::Receiver<()>) -> Status {
    let Response::Ok(Payload::Status(status)) = query_status(core, socket, wakes) else {
        panic!("expected a status");
    };
    status
}

#[test]
fn the_status_reports_night_shift_as_last_observed() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Off,
    });

    assert_eq!(
        status_of(&core, &socket, &wakes).night_shift,
        NightShift::On
    );
}

#[test]
fn on_while_night_shift_is_off_turns_it_on_and_sets_the_manual_state() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, by_hand(OnOff::On));
}

#[test]
fn off_while_night_shift_is_on_turns_it_off_and_sets_the_manual_state() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Off,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::Off);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(response, by_hand(OnOff::Off));
}

#[test]
fn on_while_night_shift_is_already_on_writes_nothing_but_still_sets_the_manual_state() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Off,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(effects, Vec::new());
    assert_eq!(response, by_hand(OnOff::On));
}

#[test]
fn toggle_inverts_night_shift_as_last_observed() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Off,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(response, by_hand(OnOff::Off));
}

#[test]
fn a_command_while_night_shift_is_unavailable_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    let (effects, response) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(effects, Vec::new());
    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    assert_eq!(error.code, ErrorCode::NightShiftUnavailable);
}

#[test]
fn the_status_reports_the_schedule_its_scheduled_state_and_the_next_transition() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");

    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.schedule, Some(custom_hours()));
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(
        status.next_transition,
        Some(at("2026-10-16T22:00:00+02:00"))
    );
}

#[test]
fn the_core_asks_to_be_woken_at_the_next_schedule_transition() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T20:00:00+02:00");

    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+02:00").into()
        }]
    );
}

#[test]
fn the_manual_state_ends_at_the_next_schedule_transition() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    send(&core, &socket, &wakes, Command::On);

    clock.set("2026-10-16T22:00:00+02:00");
    let effects = core.handle(Event::TimerFired);

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
}

#[test]
fn a_timer_that_fires_before_its_transition_is_set_again() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, clock) = start_at(&socket_in(&dir), "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    clock.set("2026-10-16T21:59:59+02:00");

    assert_eq!(
        core.handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+02:00").into()
        }]
    );
}

#[test]
fn an_external_change_is_adopted_as_the_manual_state() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(effects, Vec::new());
    assert_eq!(
        status_of(&core, &socket, &wakes).manual_state,
        Some(OnOff::On)
    );
}

#[test]
fn an_external_change_back_to_the_schedule_replaces_the_manual_state() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    send(&core, &socket, &wakes, Command::On);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    assert_eq!(
        status_of(&core, &socket, &wakes).manual_state,
        Some(OnOff::Off)
    );
}

#[test]
fn a_change_just_before_a_schedule_transition_is_that_transition() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    send(&core, &socket, &wakes, Command::Off);

    clock.set("2026-10-16T21:59:00+02:00");
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.manual_state, None);
    assert_eq!(status.scheduled_state, Some(OnOff::On));
}

#[test]
fn a_change_against_the_coming_transition_is_still_an_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    send(&core, &socket, &wakes, Command::On);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    clock.set("2026-10-16T21:59:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    assert_eq!(
        status_of(&core, &socket, &wakes).manual_state,
        Some(OnOff::Off)
    );
}

#[test]
fn editing_the_schedule_is_a_schedule_transition() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    send(&core, &socket, &wakes, Command::Off);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    // Moving the start to 21:00 makes macOS apply the schedule again, so Night Shift comes on.
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Custom {
            from: TimeOfDay {
                hour: 21,
                minute: 0,
            },
            to: TimeOfDay { hour: 7, minute: 0 },
        },
    });

    assert_eq!(effects, Vec::new());
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
}

#[test]
fn a_transition_missed_during_sleep_ends_the_manual_state_on_waking() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    send(&core, &socket, &wakes, Command::On);

    clock.set("2026-10-17T08:00:00+02:00");
    let effects = core.handle(Event::Woke);

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T22:00:00+02:00").into()
        }]
    );
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
}

#[test]
fn a_time_zone_change_moves_the_custom_hours() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    let effects = core.handle(Event::TimeZoneChanged {
        iana_id: "Europe/London".to_owned(),
    });

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+01:00").into()
        }]
    );
}

#[test]
fn sunset_to_sunrise_follows_the_reported_sun_times() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T01:40:00+02:00");

    // As CoreBrightness reported them in Zurich that night.
    core.handle(Event::SunTimesChanged {
        sunrises: [
            "2026-10-15T05:49:20Z",
            "2026-10-16T05:50:47Z",
            "2026-10-17T05:52:14Z",
        ]
        .map(|sunrise| at(sunrise).into())
        .to_vec(),
        sunsets: [
            "2026-10-15T16:33:25Z",
            "2026-10-16T16:31:32Z",
            "2026-10-17T16:29:41Z",
        ]
        .map(|sunset| at(sunset).into())
        .to_vec(),
    });
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::SunsetToSunrise,
    });

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T05:50:47Z").into()
        }]
    );
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.scheduled_state, Some(OnOff::On));
    assert_eq!(status.manual_state, None);
}

#[test]
fn turning_the_schedule_off_cancels_the_wakeup() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
    });

    assert_eq!(effects, vec![Effect::CancelWakeup]);
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(status.next_transition, None);
}

#[test]
fn a_config_rule_turns_night_shift_off_while_its_app_is_frontmost() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    let effects = activate(&core, "com.figma.Desktop");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.frontmost_app.as_deref(), Some("com.figma.Desktop"));
    assert_eq!(
        status.decision,
        Decision {
            verdict: Verdict::ForceOff,
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }
    );
}

#[test]
fn leaving_the_app_hands_night_shift_back_to_the_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\napp = [\"com.figma.Desktop\"]\n");
    let (core, _wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    activate(&core, "com.figma.Desktop");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    let effects = activate(&core, "com.apple.Terminal");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
}

#[test]
fn an_invalid_config_at_launch_runs_without_config_rules_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\napps = [\"com.figma.Desktop\"]\n");
    let (core, wakes) = start(&socket);

    let effects = core.handle(Event::Launched);

    let [Effect::Notify { body, .. }] = effects.as_slice() else {
        panic!("expected one notification, got {effects:?}");
    };
    assert!(body.contains("apps"), "{body}");
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.rules, Vec::new());
    assert!(
        status
            .config_error
            .is_some_and(|error| error.contains("apps"))
    );
}

#[test]
fn reload_applies_a_changed_config() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    activate(&core, "com.figma.Desktop");

    write_config(&socket, "[rules]\napp = [\"com.figma.Desktop\"]\n");
    let (effects, response) = exchange(&core, &socket, &wakes, Request::Reload);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    let Response::Ok(Payload::Status(status)) = response else {
        panic!("expected a status, got {response:?}");
    };
    assert_eq!(status.rules, vec![app_rule("com.figma.Desktop")]);
}

#[test]
fn reloading_an_invalid_config_is_refused_and_keeps_the_rules_in_force() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes) = start(&socket);

    write_config(&socket, "[rules]\napp = \"com.figma.Desktop\"\n");
    let (effects, response) = exchange(&core, &socket, &wakes, Request::Reload);

    assert_eq!(effects, Vec::new());
    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    assert_eq!(error.code, ErrorCode::InvalidConfig);
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.rules, vec![app_rule("com.figma.Desktop")]);
    assert_eq!(status.config_error, Some(error.message));
}

#[test]
fn a_command_answers_with_the_resulting_decision() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    let (_, response) = send(&core, &socket, &wakes, Command::Off);

    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            verdict: Verdict::HandsOff,
            reasons: vec![Reason::ManualState {
                state: OnOff::Off,
                until: Some(at("2026-10-17T07:00:00+02:00")),
            }],
        }))
    );
}

#[test]
fn macos_performing_a_transition_just_after_the_core_applied_it_is_not_an_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T21:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    activate(&core, "com.figma.Desktop");
    clock.set("2026-10-16T22:00:00+02:00");
    core.handle(Event::TimerFired);

    clock.set("2026-10-16T22:01:00+02:00");
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
}

#[test]
fn after_waking_macos_catching_up_on_a_missed_transition_is_not_an_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T21:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    activate(&core, "com.figma.Desktop");
    clock.set("2026-10-16T23:00:00+02:00");
    core.handle(Event::Woke);

    clock.set("2026-10-16T23:03:00+02:00");
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
}

#[test]
fn on_during_a_force_snoozes_the_forcing_rules_until_the_next_transition() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);

    let (effects, response) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    let morning = Some(at("2026-10-17T07:00:00+02:00"));
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            verdict: Verdict::HandsOff,
            reasons: vec![
                Reason::Snooze {
                    identity: figma(),
                    until: morning,
                },
                Reason::ManualState {
                    state: OnOff::On,
                    until: morning,
                },
            ],
        }))
    );
}

#[test]
fn snoozes_end_at_the_next_schedule_transition() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_forced(&socket);
    send(&core, &socket, &wakes, Command::On);

    clock.set("2026-10-17T07:00:00+02:00");
    core.handle(Event::TimerFired);

    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.snoozes, Vec::new());
    assert_eq!(status.decision.verdict, Verdict::ForceOff);
}

#[test]
fn turning_night_shift_on_outside_nightcap_during_a_force_snoozes_it_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);

    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(
        effects,
        vec![Effect::Notify {
            title: "Night Shift turned on outside Nightcap".to_owned(),
            body: "Snoozed app:com.figma.Desktop until 07:00.".to_owned(),
        }]
    );
    assert_eq!(status_of(&core, &socket, &wakes).snoozes, vec![figma()]);
}

#[test]
fn a_timed_disable_forces_night_shift_off_until_its_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    let (effects, response) = send(
        &core,
        &socket,
        &wakes,
        Command::Disable(SignedDuration::from_hours(1)),
    );

    let midnight = at("2026-10-17T00:00:00+02:00");
    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: midnight.into()
            },
        ]
    );
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            verdict: Verdict::ForceOff,
            reasons: vec![Reason::TimedDisable { until: midnight }],
        }))
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    clock.set("2026-10-17T00:00:00+02:00");
    assert_eq!(
        core.handle(Event::TimerFired),
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(status_of(&core, &socket, &wakes).timed_disable, None);
}

#[test]
fn on_cancels_a_timed_disable() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    send(
        &core,
        &socket,
        &wakes,
        Command::Disable(SignedDuration::from_hours(1)),
    );

    let (_, response) = send(&core, &socket, &wakes, Command::On);

    let Response::Ok(Payload::Decision(decision)) = response else {
        panic!("expected a decision, got {response:?}");
    };
    assert_eq!(decision.verdict, Verdict::HandsOff);
    assert_eq!(status_of(&core, &socket, &wakes).timed_disable, None);
}

#[test]
fn resume_clears_the_timed_disable_every_snooze_and_the_manual_state() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    send(&core, &socket, &wakes, Command::On);
    send(
        &core,
        &socket,
        &wakes,
        Command::Disable(SignedDuration::from_hours(1)),
    );

    let (_, response) = send(&core, &socket, &wakes, Command::Resume);

    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            verdict: Verdict::ForceOff,
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }))
    );
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.snoozes, Vec::new());
    assert_eq!(status.timed_disable, None);
    assert_eq!(status.manual_state, None);
}

#[test]
fn the_overrides_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    send(&core, &socket, &wakes, Command::On);
    send(
        &core,
        &socket,
        &wakes,
        Command::Disable(SignedDuration::from_hours(1)),
    );

    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-16T23:30:00+02:00");

    assert_eq!(
        core.handle(Event::Launched),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T00:00:00+02:00").into()
        }]
    );
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.manual_state, Some(OnOff::On));
    assert_eq!(status.snoozes, vec![figma()]);
    assert_eq!(status.timed_disable, Some(at("2026-10-17T00:00:00+02:00")));
}

#[test]
fn overrides_that_ended_while_nightcap_was_stopped_are_dropped_on_start() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    send(&core, &socket, &wakes, Command::On);
    send(
        &core,
        &socket,
        &wakes,
        Command::Disable(SignedDuration::from_hours(1)),
    );

    // Past both the timed disable's end and the 07:00 schedule transition.
    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-17T08:00:00+02:00");

    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.manual_state, None);
    assert_eq!(status.snoozes, Vec::new());
    assert_eq!(status.timed_disable, None);
}

#[test]
fn terminating_releases_every_force() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_forced(&socket_in(&dir));

    assert_eq!(
        core.handle(Event::Terminating),
        vec![Effect::SetNightShift { enabled: true }]
    );
}

#[test]
fn after_a_crash_night_shift_left_off_by_a_force_is_not_an_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_crashed, _wakes, _clock) = start_forced(&socket);

    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-16T23:10:00+02:00");
    activate(&core, "com.apple.Terminal");
    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
}

#[test]
fn after_a_clean_exit_night_shift_found_off_is_an_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (stopped, _wakes, _clock) = start_forced(&socket);
    stopped.handle(Event::Terminating);

    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-16T23:10:00+02:00");
    activate(&core, "com.apple.Terminal");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    assert_eq!(
        status_of(&core, &socket, &wakes).manual_state,
        Some(OnOff::Off)
    );
}

#[test]
fn after_a_clean_exit_a_force_still_in_place_comes_back() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (stopped, _wakes, _clock) = start_forced(&socket);
    stopped.handle(Event::Terminating);

    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-16T23:10:00+02:00");
    activate(&core, "com.figma.Desktop");
    // Night Shift as the release on exit left it.
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(status_of(&core, &socket, &wakes).snoozes, Vec::new());
}

#[test]
fn update_in_a_self_managed_release_starts_sparkles_check() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start_as(&socket, Instance::Release, Install::SelfManaged);

    let (effects, response) = exchange(&core, &socket, &wakes, Request::Update);

    assert_eq!(effects, vec![Effect::CheckForUpdates]);
    assert_eq!(response, Response::Ok(Payload::UpdateCheckStarted));
}

#[test]
fn update_is_refused_where_nightcap_doesnt_update_itself() {
    for (instance, install) in [
        (Instance::Release, Install::Managed),
        (Instance::Dev, Install::SelfManaged),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let socket = socket_in(&dir);
        let (core, wakes) = start_as(&socket, instance, install);

        let (effects, response) = exchange(&core, &socket, &wakes, Request::Update);

        assert_eq!(effects, Vec::new());
        let Response::Error(error) = response else {
            panic!("expected an error, got {response:?}");
        };
        assert_eq!(error.code, ErrorCode::NoUpdater);
    }
}

#[test]
fn the_status_reports_the_last_permission_change() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    core.handle(Event::PermissionsChanged {
        accessibility: true,
        notifications: NotificationPermission::Denied,
    });

    let Response::Ok(Payload::Status(status)) = query_status(&core, &socket, &wakes) else {
        panic!("expected a status");
    };
    assert_eq!(
        status.permissions,
        Permissions {
            accessibility: true,
            notifications: NotificationPermission::Denied,
        }
    );
}

#[test]
fn a_status_query_is_answered_on_the_next_tick() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    assert_eq!(
        query_status(&core, &socket, &wakes),
        Response::Ok(Payload::Status(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: Install::Managed,
            night_shift: NightShift::Unavailable,
            decision: Decision {
                verdict: Verdict::HandsOff,
                reasons: Vec::new(),
            },
            manual_state: None,
            schedule: None,
            scheduled_state: None,
            next_transition: None,
            frontmost_app: None,
            rules: Vec::new(),
            config_error: None,
            snoozes: Vec::new(),
            timed_disable: None,
            permissions: Permissions {
                accessibility: false,
                notifications: NotificationPermission::NotRequested,
            },
        }))
    );
}

#[test]
fn a_client_that_hangs_up_at_once_leaves_the_server_answering() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_core, _wakes) = start(&socket);

    drop(UnixStream::connect(&socket).unwrap());

    assert_eq!(
        Client::connect(&socket).hello(1),
        HelloReply::Hello(Hello::current())
    );
}

#[test]
fn a_protocol_mismatch_is_refused_at_hello() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_core, wakes) = start(&socket);

    let reply = Client::connect(&socket).hello(99);

    let HelloReply::Error(error) = reply else {
        panic!("expected an error, got {reply:?}");
    };
    assert_eq!(error.code, ErrorCode::ProtocolMismatch);
    assert!(
        wakes.try_recv().is_err(),
        "a refused client never reaches the main thread"
    );
}

#[test]
fn a_malformed_request_is_answered_without_a_tick() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_core, wakes) = start(&socket);

    let mut client = Client::connect(&socket);
    client.hello(1);
    client.send(r#"{"query":"weather"}"#);
    let response: Response = client.receive();

    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(wakes.try_recv().is_err());
}

#[test]
fn a_second_instance_on_the_same_socket_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_first, _wakes) = start(&socket);

    let (sender, _) = mpsc::channel();
    let second = Core::start(
        setup(&socket, Instance::Release, Install::Managed),
        Arc::new(ChannelWaker(Mutex::new(sender))),
    );

    assert!(matches!(second, Err(CoreError::AlreadyRunning { .. })));
}

#[test]
fn a_stale_socket_left_by_a_crash_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    drop(UnixListener::bind(&socket).unwrap());

    let (_core, _wakes) = start(&socket);

    assert!(UnixStream::connect(&socket).is_ok());
}

#[test]
fn the_socket_folder_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (_core, _wakes) = start(&socket);

    let mode = std::fs::metadata(socket.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn terminating_removes_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes) = start(&socket);

    assert_eq!(core.handle(Event::Terminating), Vec::new());

    assert!(!socket.exists());
}

#[test]
fn a_self_managed_release_registers_its_login_item_and_starts_its_updater_at_launch() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Release, Install::SelfManaged);

    assert_eq!(
        core.handle(Event::Launched),
        vec![Effect::RegisterLoginItem, Effect::StartUpdater]
    );
}

#[test]
fn a_managed_install_leaves_starting_at_login_to_its_installer() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Release, Install::Managed);

    assert_eq!(core.handle(Event::Launched), Vec::new());
}

#[test]
fn a_dev_build_never_registers_a_login_item() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Dev, Install::SelfManaged);

    assert_eq!(core.handle(Event::Launched), Vec::new());
}

#[test]
fn the_login_item_is_registered_only_once_across_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let state_file = dir.path().join("state").join("state.json");
    let first = Setup {
        state_file: state_file.clone(),
        ..setup(&socket_in(&dir), Instance::Release, Install::SelfManaged)
    };
    let (core, _wakes) = start_with(first);
    assert_eq!(
        core.handle(Event::Launched),
        vec![Effect::RegisterLoginItem, Effect::StartUpdater]
    );
    core.handle(Event::LoginItemRegistered);

    // The first core still holds its socket, so the restarted one gets its own.
    let restarted = Setup {
        socket: dir.path().join("restarted").join("nightcap.sock"),
        state_file,
        ..setup(&socket_in(&dir), Instance::Release, Install::SelfManaged)
    };
    let (core, _wakes) = start_with(restarted);

    assert_eq!(core.handle(Event::Launched), vec![Effect::StartUpdater]);
}

#[test]
fn saved_state_loads_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("nightcap").join("state.json"));
    let state = Persisted {
        login_item_registered: true,
        manual_state: Some(OnOff::Off),
        snoozes: vec![figma()],
        timed_disable: Some(at("2026-10-17T00:00:00+02:00")),
        night_shift: Some(OnOff::Off),
        until: Some(at("2026-10-17T07:00:00+02:00")),
    };

    store.save(&state).unwrap();

    assert_eq!(store.load(), state);
}

#[test]
fn missing_state_starts_fresh() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(
        Store::new(dir.path().join("state.json")).load(),
        Persisted::default()
    );
}

#[test]
fn state_from_a_newer_build_starts_fresh() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, r#"{"version":2,"login_item_registered":true}"#).unwrap();

    assert_eq!(Store::new(path).load(), Persisted::default());
}

#[test]
fn state_with_keys_this_build_doesnt_know_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(
        &path,
        r#"{"version":1,"login_item_registered":true,"snoozes":[]}"#,
    )
    .unwrap();

    assert!(Store::new(path).load().login_item_registered);
}
