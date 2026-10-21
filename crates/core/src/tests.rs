use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    Check, CheckKind, CheckState, Command, Decision, ErrorCode, ExportFormat, Hello, HelloMessage,
    HelloReply, Install, Instance, NightShift, NotificationPermission, OnOff, Payload, Permissions,
    Query, Reason, Request, Response, Rule, RuleIdentity, RuleSource, Schedule, Status, TimeOfDay,
    Verdict, WebsiteChecking,
};

use crate::adhoc;
use crate::config::NightShiftConfig;
use crate::state::{Persisted, Store};
use crate::{
    Clock, Core, CoreError, Effect, Event, Fix, FixButton, LoginItem, MenuEntry, MenuIcon,
    MenuItem, OnboardingNightShift, OnboardingSteps, ProblemLine, RuleAction, SettingsCheck,
    SettingsRule, SettingsSwitch, Setup, Waker, normalize_domain,
};

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
        rules_file: socket.parent().unwrap().with_file_name("rules.toml"),
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

/// The launch's effects, leaving out onboarding, which only its own tests look at.
fn launch(core: &Core) -> Vec<Effect> {
    core.handle(Event::Launched)
        .into_iter()
        .filter(|effect| !matches!(effect, Effect::ShowOnboarding { .. }))
        .collect()
}

fn ad_hoc(identity: RuleIdentity) -> Rule {
    Rule {
        identity,
        source: RuleSource::AdHoc,
    }
}

fn rules_of(core: &Core, socket: &Path, wakes: &mpsc::Receiver<()>) -> Vec<Rule> {
    let (_, response) = exchange(core, socket, wakes, Request::Query(Query::Rules));
    let Response::Ok(Payload::Rules(rules)) = response else {
        panic!("expected rules, got {response:?}");
    };
    rules
}

fn figma() -> RuleIdentity {
    RuleIdentity::App("com.figma.Desktop".to_owned())
}

fn activate(core: &Core, bundle_id: &str) -> Vec<Effect> {
    core.handle(Event::AppActivated {
        bundle_id: Some(bundle_id.to_owned()),
        name: None,
        is_browser: false,
    })
}

fn activate_browser(core: &Core, bundle_id: &str) -> Vec<Effect> {
    core.handle(Event::AppActivated {
        bundle_id: Some(bundle_id.to_owned()),
        name: None,
        is_browser: true,
    })
}

fn report_host(core: &Core, generation: u64, host: &str) -> Vec<Effect> {
    core.handle(Event::HostChanged {
        generation,
        host: Some(host.to_owned()),
    })
}

fn watch(bundle_id: &str, generation: u64) -> Effect {
    Effect::WatchBrowser {
        bundle_id: bundle_id.to_owned(),
        generation,
    }
}

/// Website checking is useless without Accessibility, so tests about it start granted.
fn grant_accessibility(core: &Core) {
    core.handle(Event::PermissionsChanged {
        accessibility: true,
        notifications: NotificationPermission::Granted,
    });
}

fn domain_rule(domain: &str) -> Rule {
    Rule {
        identity: RuleIdentity::Domain(domain.to_owned()),
        source: RuleSource::Config,
    }
}

/// Starts at 23:00 with Night Shift on by the custom hours, website checking on and a config
/// rule for figma.com.
fn start_with_figma_domain(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    write_config(
        socket,
        "websites = true\n[rules]\ndomain = [\"figma.com\"]\n",
    );
    let (core, wakes, clock) = start_at(socket, "2026-10-16T23:00:00+02:00");
    grant_accessibility(&core);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    (core, wakes, clock)
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
    *status
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
fn the_status_reports_the_strength_as_last_read() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    assert_eq!(status_of(&core, &socket, &wakes).strength, None);

    core.handle(Event::StrengthChanged { value: 60 });

    assert_eq!(status_of(&core, &socket, &wakes).strength, Some(60));
}

/// Night Shift reported off with no schedule, at a strength of 60.
fn start_at_strength_60(socket: &Path) -> (Core, mpsc::Receiver<()>) {
    let (core, wakes) = start(socket);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
    });
    core.handle(Event::StrengthChanged { value: 60 });
    (core, wakes)
}

#[test]
fn strength_sets_it_and_answers_with_the_decision() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start_at_strength_60(&socket);
    let decision = status_of(&core, &socket, &wakes).decision;

    let (effects, response) = send(&core, &socket, &wakes, Command::Strength(40));

    assert_eq!(
        effects,
        vec![Effect::SetStrength {
            value: 40,
            commit: true
        }]
    );
    assert_eq!(response, Response::Ok(Payload::Decision(decision)));
}

#[test]
fn previewing_a_strength_writes_it_without_saving_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start_at_strength_60(&socket);

    let (effects, _) = send(&core, &socket, &wakes, Command::PreviewStrength(80));

    assert_eq!(
        effects,
        vec![Effect::SetStrength {
            value: 80,
            commit: false
        }]
    );
}

#[test]
fn strength_while_night_shift_is_unavailable_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    let (effects, response) = send(&core, &socket, &wakes, Command::Strength(40));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::NightShiftUnavailable);
}

#[test]
fn strength_before_the_shell_could_read_it_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::PreviewStrength(40));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::StrengthUnavailable);
}

#[test]
fn a_strength_above_100_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start_at_strength_60(&socket);

    let (effects, response) = send(&core, &socket, &wakes, Command::Strength(101));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::BadRequest);
}

#[test]
fn set_schedule_writes_it_and_answers_with_the_decision() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start_at_strength_60(&socket);
    let decision = status_of(&core, &socket, &wakes).decision;

    let (effects, response) = send(&core, &socket, &wakes, Command::SetSchedule(custom_hours()));

    assert_eq!(
        effects,
        vec![Effect::SetSchedule {
            schedule: custom_hours()
        }]
    );
    assert_eq!(response, Response::Ok(Payload::Decision(decision)));
}

#[test]
fn switching_the_schedule_off_while_night_shift_is_on_keeps_it_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    send(&core, &socket, &wakes, Command::SetSchedule(Schedule::Off));
    // macOS switches Night Shift off along with the schedule.
    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
    });

    assert!(effects.contains(&Effect::SetNightShift { enabled: true }));
    assert_eq!(
        status_of(&core, &socket, &wakes).manual_state,
        Some(OnOff::On)
    );
}

#[test]
fn set_schedule_while_night_shift_is_unavailable_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    let (effects, response) = send(&core, &socket, &wakes, Command::SetSchedule(Schedule::Off));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::NightShiftUnavailable);
}

#[test]
fn custom_hours_that_arent_times_of_day_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start_at_strength_60(&socket);
    let hours = Schedule::Custom {
        from: TimeOfDay {
            hour: 24,
            minute: 0,
        },
        to: TimeOfDay { hour: 7, minute: 0 },
    };

    let (effects, response) = send(&core, &socket, &wakes, Command::SetSchedule(hours));

    assert_eq!(effects, Vec::new());
    assert_eq!(refusal(response), ErrorCode::BadRequest);
}

/// Night Shift off with no schedule, at a strength of 60, as the shell reports it before
/// `Launched`.
fn report_night_shift(core: &Core, schedule: Schedule, strength: u8) {
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule,
    });
    core.handle(Event::StrengthChanged { value: strength });
}

#[test]
fn the_configs_night_shift_values_are_applied_at_launch() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n",
    );
    let (core, _wakes) = start(&socket);
    report_night_shift(&core, Schedule::Off, 60);

    assert_eq!(
        launch(&core),
        vec![
            Effect::SetStrength {
                value: 40,
                commit: true
            },
            Effect::SetSchedule {
                schedule: Schedule::SunsetToSunrise
            },
        ]
    );
}

#[test]
fn the_configs_night_shift_values_apply_once_and_a_change_made_since_stays() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n",
    );
    let (core, _wakes) = start(&socket);
    report_night_shift(&core, Schedule::Off, 60);
    launch(&core);

    // The user moved the strength in System Settings, then Nightcap started again.
    let (core, _socket, _wakes, _clock) = restart_at(&socket, "2026-10-16T12:05:00+02:00");
    report_night_shift(&core, Schedule::SunsetToSunrise, 70);

    assert_eq!(launch(&core), Vec::new());
}

#[test]
fn the_status_shows_the_configs_night_shift_values_beside_the_macs() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n",
    );
    let (core, wakes) = start(&socket);
    report_night_shift(&core, Schedule::Off, 60);

    let status = status_of(&core, &socket, &wakes);

    assert_eq!(
        (status.strength, status.config_strength),
        (Some(60), Some(40))
    );
    assert_eq!(
        (status.schedule, status.config_schedule),
        (Some(Schedule::Off), Some(Schedule::SunsetToSunrise))
    );
}

#[test]
fn reloading_applies_only_the_night_shift_value_that_changed() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "[night_shift]\nstrength = 40\nschedule = \"sunset-to-sunrise\"\n",
    );
    let (core, wakes) = start(&socket);
    report_night_shift(&core, Schedule::Off, 60);
    launch(&core);

    write_config(
        &socket,
        "[night_shift]\nstrength = 50\nschedule = \"sunset-to-sunrise\"\n",
    );
    let (effects, _) = exchange(&core, &socket, &wakes, Request::Reload);

    assert_eq!(
        effects,
        vec![Effect::SetStrength {
            value: 50,
            commit: true
        }]
    );
}

#[test]
fn a_night_shift_value_taken_out_of_the_config_stays_and_applies_again_when_put_back() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[night_shift]\nstrength = 40\n");
    let (core, wakes) = start(&socket);
    report_night_shift(&core, Schedule::Off, 60);
    launch(&core);

    write_config(&socket, "");
    let (taken_out, _) = exchange(&core, &socket, &wakes, Request::Reload);
    write_config(&socket, "[night_shift]\nstrength = 40\n");
    let (put_back, _) = exchange(&core, &socket, &wakes, Request::Reload);

    assert_eq!(taken_out, Vec::new());
    assert_eq!(
        put_back,
        vec![Effect::SetStrength {
            value: 40,
            commit: true
        }]
    );
}

#[test]
fn a_night_shift_value_the_config_cant_have_makes_it_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    report_night_shift(&core, Schedule::Off, 60);

    for night_shift in [
        "strength = 101",
        "schedule = \"dusk\"",
        "schedule = \"22:00-7am\"",
        "schedule = \"24:00-07:00\"",
    ] {
        write_config(&socket, &format!("[night_shift]\n{night_shift}\n"));
        let (effects, response) = exchange(&core, &socket, &wakes, Request::Reload);
        assert_eq!(effects, Vec::new(), "{night_shift}");
        assert_eq!(refusal(response), ErrorCode::InvalidConfig, "{night_shift}");
    }
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

/// The answer to a command that leaves Night Shift on at 23:00 by the custom hours.
fn following_the_custom_hours_until_morning() -> Response {
    Response::Ok(Payload::Decision(Decision {
        verdict: Verdict::HandsOff,
        reasons: vec![Reason::Schedule {
            state: OnOff::On,
            until: Some(at("2026-10-17T07:00:00+02:00")),
        }],
    }))
}

#[test]
fn toggling_night_shift_off_and_back_on_follows_the_schedule_again() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    send(&core, &socket, &wakes, Command::Toggle);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
    assert_eq!(entry_titled(&core, "Resume"), None);
}

#[test]
fn on_while_the_schedule_has_night_shift_on_sets_no_manual_state() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(effects, Vec::new());
    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
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
fn an_external_change_back_to_the_schedule_ends_the_manual_state() {
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

    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    assert_eq!(effects, Vec::new());
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
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

    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(
        status.next_transition,
        Some(at("2026-10-16T22:00:00+02:00"))
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

    let effects = launch(&core);

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
        launch(&core),
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

/// Starts at 23:00 with Night Shift on by the custom hours, then disables it for an hour.
fn start_disabled(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    let (core, wakes, clock) = start_at(socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    send(
        &core,
        socket,
        &wakes,
        Command::Disable(SignedDuration::from_hours(1)),
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    (core, wakes, clock)
}

#[test]
fn while_another_user_is_active_nightcap_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes, clock) = start_disabled(&socket);
    core.handle(Event::SessionChanged { active: false });

    clock.set("2026-10-17T00:00:00+02:00");

    assert_eq!(
        core.handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
}

#[test]
fn status_callbacks_while_another_user_is_active_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    core.handle(Event::SessionChanged { active: false });

    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(effects, Vec::new());
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.snoozes, Vec::new());
    assert_eq!(status.manual_state, None);
}

#[test]
fn returning_to_the_session_puts_night_shift_where_nightcap_wants_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_disabled(&socket);
    core.handle(Event::SessionChanged { active: false });
    clock.set("2026-10-17T00:00:00+02:00");
    core.handle(Event::TimerFired);

    assert_eq!(
        core.handle(Event::SessionChanged { active: true }),
        Vec::new()
    );
    // The shell's fresh read: still off, as the timed disable left it.
    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
}

#[test]
fn a_domain_rule_turns_night_shift_off_on_its_site() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_with_figma_domain(&socket);

    assert_eq!(
        activate_browser(&core, "com.apple.Safari"),
        vec![watch("com.apple.Safari", 1)]
    );
    assert_eq!(
        report_host(&core, 1, "www.figma.com"),
        vec![Effect::SetNightShift { enabled: false }]
    );

    let status = status_of(&core, &socket, &wakes);
    assert!(status.watching);
    assert_eq!(status.host.as_deref(), Some("www.figma.com"));
    assert_eq!(
        status.decision.reasons,
        vec![Reason::Rule(domain_rule("figma.com"))]
    );
}

/// Starts with Safari in front on figma.com, Night Shift forced off and reported off.
fn start_on_figma(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    let (core, wakes, clock) = start_with_figma_domain(socket);
    activate_browser(&core, "com.apple.Safari");
    report_host(&core, 1, "www.figma.com");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    (core, wakes, clock)
}

#[test]
fn a_host_from_an_older_watch_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_with_figma_domain(&socket);
    activate_browser(&core, "com.apple.Safari");
    assert_eq!(
        activate_browser(&core, "com.google.Chrome"),
        vec![watch("com.google.Chrome", 2)]
    );

    assert_eq!(report_host(&core, 1, "www.figma.com"), Vec::new());

    assert_eq!(status_of(&core, &socket, &wakes).host, None);
}

#[test]
fn leaving_the_browser_stops_watching_and_lifts_the_force() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_on_figma(&socket_in(&dir));

    assert_eq!(
        activate(&core, "com.apple.Terminal"),
        vec![
            Effect::StopWatching,
            Effect::SetNightShift { enabled: true }
        ]
    );
}

#[test]
fn a_browser_its_app_rule_forces_off_is_watched_again_while_the_rule_is_snoozed() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "websites = true\n[rules]\napp = [\"com.apple.Safari\"]\n",
    );
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    grant_accessibility(&core);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(
        activate_browser(&core, "com.apple.Safari"),
        vec![Effect::SetNightShift { enabled: false }]
    );
    let (effects, _) = send(&core, &socket, &wakes, Command::Snooze(safari()));
    assert!(effects.contains(&watch("com.apple.Safari", 1)));
    let (effects, _) = send(&core, &socket, &wakes, Command::Unsnooze(safari()));
    assert!(effects.contains(&Effect::StopWatching));
}

#[test]
fn with_website_checking_on_the_front_browser_is_watched_without_any_domain_rule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = true\n");
    let (core, _wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    grant_accessibility(&core);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(
        activate_browser(&core, "com.apple.Safari"),
        vec![watch("com.apple.Safari", 1)]
    );
}

#[test]
fn a_new_watch_leaves_night_shift_alone_until_its_first_read() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_on_figma(&socket_in(&dir));

    // Without a host yet, Chrome alone wouldn't force Night Shift off, but switching it on until
    // the first read would only flash it.
    assert_eq!(
        activate_browser(&core, "com.google.Chrome"),
        vec![watch("com.google.Chrome", 2)]
    );
    assert_eq!(report_host(&core, 2, "www.figma.com"), Vec::new());
}

#[test]
fn an_unreadable_host_leaves_only_app_rules() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_figma(&socket);

    let effects = core.handle(Event::HostChanged {
        generation: 1,
        host: None,
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    let status = status_of(&core, &socket, &wakes);
    assert!(status.watching);
    assert_eq!(status.host, None);
}

#[test]
fn domains_in_the_config_are_normalized_and_checked() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\ndomain = [\"Bücher.DE.\"]\n");
    let (core, wakes) = start(&socket);

    assert_eq!(
        status_of(&core, &socket, &wakes).rules,
        vec![domain_rule("xn--bcher-kva.de")]
    );
    for entry in [
        "https://figma.com",
        "figma.com/files",
        "figma.com:443",
        "*.figma.com",
        "",
    ] {
        write_config(&socket, &format!("[rules]\ndomain = [{entry:?}]\n"));
        let (_, response) = exchange(&core, &socket, &wakes, Request::Reload);
        let Response::Error(error) = response else {
            panic!("{entry:?} was accepted: {response:?}");
        };
        assert_eq!(error.code, ErrorCode::InvalidConfig, "{entry:?}");
    }
}

#[test]
fn hosts_compare_in_normal_form() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_with_figma_domain(&socket_in(&dir));
    activate_browser(&core, "com.apple.Safari");

    assert_eq!(
        report_host(&core, 1, "WWW.FIGMA.COM."),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn snoozing_a_domain_rule_lifts_its_force_and_keeps_watching() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_figma(&socket);

    let (effects, _) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(
        status.snoozes,
        vec![RuleIdentity::Domain("figma.com".to_owned())]
    );
    assert!(status.watching);
}

#[test]
fn an_added_rule_applies_at_once_and_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    activate(&core, "com.figma.Desktop");

    let (effects, response) = send(&core, &socket, &wakes, Command::AddRule(figma()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            verdict: Verdict::ForceOff,
            reasons: vec![Reason::Rule(ad_hoc(figma()))],
        }))
    );
    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-16T23:10:00+02:00");
    assert_eq!(rules_of(&core, &socket, &wakes), vec![ad_hoc(figma())]);
}

fn refusal(response: Response) -> ErrorCode {
    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    error.code
}

#[test]
fn an_added_domain_is_normalized_and_a_bad_identity_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule(RuleIdentity::Domain("Figma.COM".to_owned())),
    );

    assert_eq!(
        rules_of(&core, &socket, &wakes),
        vec![ad_hoc(RuleIdentity::Domain("figma.com".to_owned()))]
    );
    for bad in [
        RuleIdentity::Domain("https://figma.com".to_owned()),
        RuleIdentity::App("com.figma.Desktop/".to_owned()),
    ] {
        let (_, response) = send(&core, &socket, &wakes, Command::AddRule(bad));
        assert_eq!(refusal(response), ErrorCode::BadIdentity);
    }
}

#[test]
fn removing_an_ad_hoc_rule_lifts_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    activate(&core, "com.figma.Desktop");
    send(&core, &socket, &wakes, Command::AddRule(figma()));
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    let (effects, _) = send(&core, &socket, &wakes, Command::RemoveRule(figma()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(rules_of(&core, &socket, &wakes), Vec::new());
}

#[test]
fn a_config_rule_can_be_snoozed_but_not_removed() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);

    let (_, response) = send(&core, &socket, &wakes, Command::RemoveRule(figma()));
    let Response::Error(error) = response else {
        panic!("expected an error, got {response:?}");
    };
    assert_eq!(error.code, ErrorCode::ConfigRule);
    assert!(
        error
            .message
            .contains("nightcap rules snooze app:com.figma.Desktop")
    );

    let (effects, _) = send(&core, &socket, &wakes, Command::Snooze(figma()));
    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    let (effects, _) = send(&core, &socket, &wakes, Command::Unsnooze(figma()));
    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
}

#[test]
fn a_rule_that_doesnt_exist_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    let (_, removed) = send(&core, &socket, &wakes, Command::RemoveRule(figma()));
    let (_, snoozed) = send(&core, &socket, &wakes, Command::Snooze(figma()));

    assert_eq!(refusal(removed), ErrorCode::NoSuchRule);
    assert_eq!(refusal(snoozed), ErrorCode::NoSuchRule);
}

#[test]
fn an_ad_hoc_rule_graduates_when_the_config_takes_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    send(&core, &socket, &wakes, Command::AddRule(figma()));

    write_config(&socket, "[rules]\napp = [\"com.figma.desktop\"]\n");
    exchange(&core, &socket, &wakes, Request::Reload);

    assert_eq!(
        rules_of(&core, &socket, &wakes),
        vec![app_rule("com.figma.desktop")]
    );
    let store = adhoc::Store::new(socket.parent().unwrap().with_file_name("rules.toml"));
    assert_eq!(store.load(), Ok(Vec::new()));
}

#[test]
fn export_prints_the_ad_hoc_rules_for_the_config() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    send(&core, &socket, &wakes, Command::AddRule(figma()));
    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule(RuleIdentity::Domain("figma.com".to_owned())),
    );

    let export = |format| {
        let (_, response) = exchange(
            &core,
            &socket,
            &wakes,
            Request::Query(Query::Export(format)),
        );
        let Response::Ok(Payload::Export(text)) = response else {
            panic!("expected an export, got {response:?}");
        };
        text
    };

    assert_eq!(
        export(ExportFormat::Nix),
        "{ app = [ \"com.figma.Desktop\" ]; domain = [ \"figma.com\" ]; }\n"
    );
    assert_eq!(
        export(ExportFormat::Toml),
        "[rules]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n"
    );
}

#[test]
fn ad_hoc_rules_from_a_newer_build_are_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let rules_file = socket.parent().unwrap().with_file_name("rules.toml");
    let newer = "version = 2\n[rules]\napp = [\"com.figma.Desktop\"]\n";
    std::fs::write(&rules_file, newer).unwrap();
    let (core, wakes) = start(&socket);

    let (_, response) = send(&core, &socket, &wakes, Command::AddRule(figma()));

    assert_eq!(refusal(response), ErrorCode::AdHocRulesOff);
    assert!(status_of(&core, &socket, &wakes).ad_hoc_error.is_some());
    assert_eq!(rules_of(&core, &socket, &wakes), Vec::new());
    assert_eq!(std::fs::read_to_string(&rules_file).unwrap(), newer);
}

#[test]
fn the_rules_query_lists_every_rule_with_its_source() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes) = start(&socket);

    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule(RuleIdentity::Domain("figma.com".to_owned())),
    );

    assert_eq!(
        rules_of(&core, &socket, &wakes),
        vec![
            app_rule("com.figma.Desktop"),
            ad_hoc(RuleIdentity::Domain("figma.com".to_owned())),
        ]
    );
}

/// The menu's status line and the reason under it.
fn status_line(core: &Core) -> (String, Option<String>) {
    let menu = core.menu();
    (menu.status, menu.reason)
}

fn says(status: &str, reason: &str) -> (String, Option<String>) {
    (status.to_owned(), Some(reason.to_owned()))
}

#[test]
fn the_menu_says_what_night_shift_is_doing_and_why() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    assert_eq!(core.menu().icon, MenuIcon::ForcedOff);
    assert_eq!(
        status_line(&core),
        says("Night Shift: Forced Off", "While Figma is in front")
    );
    send(
        &core,
        &socket,
        &wakes,
        Command::Disable(SignedDuration::from_mins(30)),
    );
    assert_eq!(
        status_line(&core),
        says(
            "Night Shift: Forced Off",
            "While Figma is in front, and until 23:30"
        )
    );
    activate(&core, "com.apple.Terminal");
    assert_eq!(
        status_line(&core),
        says("Night Shift: Forced Off", "Until 23:30")
    );
    send(&core, &socket, &wakes, Command::Resume);
    assert_eq!(
        status_line(&core),
        says("Night Shift: On", "Following the schedule until 07:00")
    );
    send(&core, &socket, &wakes, Command::Off);
    assert_eq!(
        status_line(&core),
        says("Night Shift: Off", "Turned off by hand until 07:00")
    );
}

fn menu_item(title: &str, checked: bool, command: Command) -> MenuItem {
    MenuItem {
        title: title.to_owned(),
        subtitle: None,
        indented: false,
        checked,
        enabled: true,
        command,
    }
}

/// The first menu entry whose title starts with `prefix`.
fn entry_titled(core: &Core, prefix: &str) -> Option<MenuEntry> {
    core.menu().entries.into_iter().find(|entry| match entry {
        MenuEntry::Item { item } => item.title.starts_with(prefix),
        MenuEntry::Submenu { title, .. } | MenuEntry::Header { title } => title.starts_with(prefix),
        MenuEntry::Strength { .. } | MenuEntry::Separator => false,
    })
}

#[test]
fn the_menu_opens_with_a_strength_slider_once_the_strength_is_known() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start(&socket_in(&dir));
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
    });
    assert!(
        !core
            .menu()
            .entries
            .iter()
            .any(|entry| matches!(entry, MenuEntry::Strength { .. }))
    );

    core.handle(Event::StrengthChanged { value: 60 });

    assert_eq!(core.menu().entries[0], MenuEntry::Strength { value: 60 });
}

#[test]
fn the_menu_turns_night_shift_the_other_way_or_off_for_a_while() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    let menu = core.menu();

    assert_eq!(menu.icon, MenuIcon::On);
    assert_eq!(
        menu.entries[..2],
        [
            MenuEntry::Item {
                item: menu_item("Turn Night Shift Off", false, Command::Off)
            },
            MenuEntry::Submenu {
                title: "Turn Off For".to_owned(),
                items: vec![
                    menu_item(
                        "30 Minutes",
                        false,
                        Command::Disable(SignedDuration::from_mins(30))
                    ),
                    menu_item(
                        "1 Hour",
                        false,
                        Command::Disable(SignedDuration::from_hours(1))
                    ),
                    menu_item(
                        "2 Hours",
                        false,
                        Command::Disable(SignedDuration::from_hours(2))
                    ),
                ],
            },
        ]
    );
}

#[test]
fn the_menu_offers_resume_only_when_there_is_something_to_resume() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    assert_eq!(entry_titled(&core, "Resume"), None);
    send(&core, &socket, &wakes, Command::Off);
    assert!(entry_titled(&core, "Resume").is_some());
}

#[test]
fn resume_says_what_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    send(&core, &socket, &wakes, Command::On);
    assert_eq!(
        subtitle_of(&core, "Resume"),
        Some("Unsnoozes the Figma rule and goes back to the schedule".to_owned())
    );
    send(&core, &socket, &wakes, Command::Resume);
    send(
        &core,
        &socket,
        &wakes,
        Command::Disable(SignedDuration::from_mins(30)),
    );
    assert_eq!(
        subtitle_of(&core, "Resume"),
        Some("Ends the timed disable".to_owned())
    );
}

/// The subtitle of the menu item titled `title`, which must exist.
fn subtitle_of(core: &Core, title: &str) -> Option<String> {
    match entry_titled(core, title) {
        Some(MenuEntry::Item { item }) if item.title == title => item.subtitle,
        other => panic!("expected an item titled {title:?}, got {other:?}"),
    }
}

#[test]
fn turning_night_shift_on_during_a_force_says_what_it_snoozes_and_ends() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    assert_eq!(
        subtitle_of(&core, "Turn Night Shift On"),
        Some("Snoozes the Figma rule until 07:00".to_owned())
    );
    send(
        &core,
        &socket,
        &wakes,
        Command::Disable(SignedDuration::from_mins(30)),
    );
    assert_eq!(
        subtitle_of(&core, "Turn Night Shift On"),
        Some("Snoozes the Figma rule until 07:00 and ends the timed disable".to_owned())
    );
    activate(&core, "com.apple.Terminal");
    assert_eq!(
        subtitle_of(&core, "Turn Night Shift On"),
        Some("Ends the timed disable".to_owned())
    );
}

fn app_toggle(core: &Core) -> Option<MenuEntry> {
    entry_titled(core, "In ")
}

fn site_toggle(core: &Core) -> Option<MenuEntry> {
    entry_titled(core, "On ")
}

/// `entry` drawn one level in, as the site's toggle is under its browser's.
fn nested(entry: Option<MenuEntry>) -> Option<MenuEntry> {
    entry.map(|entry| match entry {
        MenuEntry::Item { item } => MenuEntry::Item {
            item: MenuItem {
                indented: true,
                ..item
            },
        },
        other => other,
    })
}

fn toggle_item(title: &str, checked: bool, command: Command) -> Option<MenuEntry> {
    Some(MenuEntry::Item {
        item: menu_item(title, checked, command),
    })
}

fn subtitled(title: &str, subtitle: &str, checked: bool, command: Command) -> Option<MenuEntry> {
    Some(MenuEntry::Item {
        item: MenuItem {
            subtitle: Some(subtitle.to_owned()),
            ..menu_item(title, checked, command)
        },
    })
}

#[test]
fn the_menu_adds_and_removes_an_ad_hoc_rule_for_the_frontmost_app() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    let header = MenuEntry::Header {
        title: "Force Off".to_owned(),
    };
    let toggle = toggle_item("In Figma", false, Command::AddRule(figma()));
    assert_eq!(app_toggle(&core), toggle);
    assert!(
        core.menu()
            .entries
            .windows(2)
            .any(|pair| pair[0] == header && Some(pair[1].clone()) == toggle)
    );
    send(&core, &socket, &wakes, Command::AddRule(figma()));
    assert_eq!(
        app_toggle(&core),
        toggle_item("In Figma", true, Command::RemoveRule(figma()))
    );
}

#[test]
fn the_menu_snoozes_a_config_rule_for_the_frontmost_app_rather_than_removing_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);

    assert_eq!(
        app_toggle(&core),
        subtitled(
            "In com.figma.Desktop",
            "From your config",
            true,
            Command::Snooze(figma())
        )
    );
    send(&core, &socket, &wakes, Command::Snooze(figma()));
    assert_eq!(
        app_toggle(&core),
        subtitled(
            "In com.figma.Desktop",
            "Snoozed until 07:00",
            false,
            Command::Unsnooze(figma())
        )
    );
}

#[test]
fn choosing_a_menu_item_runs_its_command() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    activate(&core, "com.figma.Desktop");

    let effects = core.handle(Event::CommandChosen {
        command: Command::AddRule(figma()),
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
}

fn offers_turn_off_for(core: &Core) -> bool {
    core.menu()
        .entries
        .iter()
        .any(|entry| matches!(entry, MenuEntry::Submenu { title, .. } if title == "Turn Off For"))
}

#[test]
fn the_menu_offers_turn_off_for_only_while_it_would_change_something() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, clock) = start_at(&socket_in(&dir), "2026-10-16T12:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });

    assert!(!offers_turn_off_for(&core), "off, and on only at 22:00");
    clock.set("2026-10-16T20:30:00+02:00");
    core.handle(Event::Woke);
    assert!(offers_turn_off_for(&core), "off, but on within two hours");
}

#[test]
fn website_checking_is_off_until_turned_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\ndomain = [\"figma.com\"]\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });

    assert_eq!(activate_browser(&core, "com.apple.Safari"), Vec::new());
    assert_eq!(
        status_of(&core, &socket, &wakes).website_checking,
        WebsiteChecking {
            state: OnOff::Off,
            from_config: false,
        }
    );

    let (effects, _) = send(&core, &socket, &wakes, Command::Websites(OnOff::On));

    assert!(
        effects.contains(&watch("com.apple.Safari", 1)),
        "{effects:?}"
    );
}

#[test]
fn the_config_decides_website_checking_when_it_sets_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = true\n");
    let (core, wakes) = start(&socket);

    let (_, response) = send(&core, &socket, &wakes, Command::Websites(OnOff::Off));

    assert_eq!(refusal(response), ErrorCode::SetInConfig);
    assert_eq!(
        status_of(&core, &socket, &wakes).website_checking,
        WebsiteChecking {
            state: OnOff::On,
            from_config: true,
        }
    );
}

#[test]
fn turning_website_checking_on_without_accessibility_asks_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::PermissionsChanged {
        accessibility: false,
        notifications: NotificationPermission::Granted,
    });

    let (effects, _) = send(&core, &socket, &wakes, Command::Websites(OnOff::On));

    assert_eq!(effects, vec![Effect::AskForAccessibility]);
}

#[test]
fn settings_switch_website_checking_unless_the_config_sets_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    let switch = |on, locked, command| SettingsSwitch {
        on,
        locked,
        command,
    };

    assert_eq!(entry_titled(&core, "Check Websites"), None);
    assert_eq!(
        core.settings().website_checking,
        switch(false, false, Command::Websites(OnOff::On))
    );
    core.handle(Event::CommandChosen {
        command: Command::Websites(OnOff::On),
    });
    assert_eq!(
        core.settings().website_checking,
        switch(true, false, Command::Websites(OnOff::Off))
    );

    write_config(&socket, "websites = false\n");
    exchange(&core, &socket, &wakes, Request::Reload);
    assert_eq!(
        core.settings().website_checking,
        switch(false, true, Command::Websites(OnOff::On))
    );
}

#[test]
fn the_menu_shows_a_problem_line_while_any_problem_exists() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = true\n");
    let (core, _wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
    });
    let line = |title: &str, subtitle: Option<&str>| {
        Some(ProblemLine {
            title: title.to_owned(),
            subtitle: subtitle.map(str::to_owned),
        })
    };

    assert_eq!(core.menu().problem_line, None);
    core.handle(permissions(false, NotificationPermission::Granted));
    assert_eq!(
        core.menu().problem_line,
        line("1 Thing Needs Attention…", Some("Accessibility"))
    );
    core.handle(permissions(false, NotificationPermission::Denied));
    assert_eq!(
        core.menu().problem_line,
        line(
            "2 Things Need Attention…",
            Some("Accessibility and notifications")
        )
    );
    core.handle(permissions(true, NotificationPermission::Granted));
    assert_eq!(core.menu().problem_line, None);
}

fn domain(name: &str) -> RuleIdentity {
    RuleIdentity::Domain(name.to_owned())
}

#[test]
fn the_menu_adds_a_rule_for_the_tabs_host_without_www_and_removes_the_most_specific_one() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = true\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    activate_browser(&core, "com.apple.Safari");
    report_host(&core, 1, "docs.figma.com");
    assert_eq!(
        site_toggle(&core),
        nested(toggle_item(
            "On docs.figma.com",
            false,
            Command::AddRule(domain("docs.figma.com"))
        ))
    );
    report_host(&core, 1, "www.figma.com");

    assert_eq!(
        site_toggle(&core),
        nested(toggle_item(
            "On figma.com",
            false,
            Command::AddRule(domain("figma.com"))
        ))
    );
    report_host(&core, 1, "docs.figma.com");
    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule(domain("figma.com")),
    );
    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule(domain("docs.figma.com")),
    );
    assert_eq!(
        site_toggle(&core),
        nested(toggle_item(
            "On docs.figma.com",
            true,
            Command::RemoveRule(domain("docs.figma.com"))
        ))
    );
}

#[test]
fn a_rule_from_the_tab_keeps_www_where_nothing_but_a_suffix_would_be_left() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = true\n");
    let (core, _wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    report(&core, true);
    activate_browser(&core, "com.apple.Safari");
    report_host(&core, 1, "www.com");

    assert_eq!(
        site_toggle(&core),
        nested(toggle_item(
            "On www.com",
            false,
            Command::AddRule(domain("www.com"))
        ))
    );
}

#[test]
fn the_menu_snoozes_a_config_rule_for_the_site_rather_than_removing_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_figma(&socket);

    assert_eq!(
        site_toggle(&core),
        nested(subtitled(
            "On figma.com",
            "From your config",
            true,
            Command::Snooze(domain("figma.com"))
        ))
    );
    send(&core, &socket, &wakes, Command::Snooze(domain("figma.com")));
    assert_eq!(
        site_toggle(&core),
        nested(subtitled(
            "On figma.com",
            "Snoozed until 07:00",
            false,
            Command::Unsnooze(domain("figma.com"))
        ))
    );
}

fn safari() -> RuleIdentity {
    RuleIdentity::App("com.apple.Safari".to_owned())
}

#[test]
fn a_browser_its_own_app_rule_forces_off_is_not_watched() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_figma(&socket);

    let (effects, _) = send(&core, &socket, &wakes, Command::AddRule(safari()));
    assert!(effects.contains(&Effect::StopWatching));
    assert_eq!(site_toggle(&core), None);
    assert_eq!(status_of(&core, &socket, &wakes).host, None);
    let (effects, _) = send(&core, &socket, &wakes, Command::RemoveRule(safari()));
    assert!(effects.contains(&watch("com.apple.Safari", 2)));
}

fn accessibility_notice() -> Effect {
    Effect::Notify {
        title: "Website checking needs Accessibility".to_owned(),
        body: "Open Nightcap’s settings to allow it, or to stop checking websites.".to_owned(),
    }
}

fn notifies(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::Notify { .. }))
}

#[test]
fn a_problem_that_appears_on_its_own_is_announced_once() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = true\n");
    let (core, _wakes) = start(&socket);

    assert!(
        core.handle(permissions(false, NotificationPermission::Granted))
            .contains(&accessibility_notice())
    );
    assert!(!notifies(
        &core.handle(permissions(false, NotificationPermission::Granted))
    ));
    core.handle(permissions(true, NotificationPermission::Granted));
    assert!(
        core.handle(permissions(false, NotificationPermission::Granted))
            .contains(&accessibility_notice())
    );

    let (core, _socket, _wakes, _clock) = restart_at(&socket, "2026-10-16T12:10:00+02:00");
    assert!(!notifies(
        &core.handle(permissions(false, NotificationPermission::Granted))
    ));
}

#[test]
fn a_problem_the_user_causes_in_nightcap_is_not_announced() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes) = start(&socket);
    core.handle(permissions(false, NotificationPermission::Granted));

    let effects = core.handle(Event::CommandChosen {
        command: Command::Websites(OnOff::On),
    });
    assert!(effects.contains(&Effect::AskForAccessibility));
    assert!(!notifies(&effects));
    assert!(!notifies(
        &core.handle(permissions(false, NotificationPermission::Granted))
    ));
}

#[test]
fn a_config_broken_at_launch_is_announced() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\ndomian = [\"figma.com\"]\n");
    let (core, _wakes) = start(&socket);

    assert!(launch(&core).iter().any(|effect| matches!(
        effect,
        Effect::Notify { title, body }
            if title == "Nightcap’s config is invalid" && body.contains("domian")
    )));
}

#[test]
fn onboarding_runs_at_the_first_launch_and_not_again() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    assert!(
        core.handle(Event::Launched)
            .contains(&Effect::ShowOnboarding { completed: 0 })
    );
    core.handle(Event::OnboardingFinished { websites: true });
    assert_eq!(
        status_of(&core, &socket, &wakes).website_checking.state,
        OnOff::On
    );

    let (core, _socket, _wakes, _clock) = restart_at(&socket, "2026-10-16T12:10:00+02:00");
    assert!(
        !core
            .handle(Event::Launched)
            .iter()
            .any(|effect| matches!(effect, Effect::ShowOnboarding { .. }))
    );
}

fn shows_onboarding(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, Effect::ShowOnboarding { .. }))
}

#[test]
fn onboarding_asks_about_night_shift_starting_from_the_macs_values() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start(&socket_in(&dir));
    report_night_shift(&core, Schedule::Off, 60);

    assert_eq!(
        core.onboarding(),
        OnboardingSteps {
            night_shift: OnboardingNightShift {
                ask_strength: true,
                ask_schedule: true,
                already_set_up: false,
                strength: Some(60),
                schedule: Some(Schedule::Off),
            },
            websites_from_config: None,
            opens_at_login: None,
        }
    );
}

#[test]
fn onboarding_offers_to_skip_night_shift_once_its_on_a_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start(&socket_in(&dir));
    report_night_shift(&core, Schedule::SunsetToSunrise, 60);

    assert!(core.onboarding().night_shift.already_set_up);
}

#[test]
fn onboarding_leaves_out_what_the_config_sets() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = true\n[night_shift]\nstrength = 40\n");
    let (core, _wakes) = start(&socket);
    report_night_shift(&core, Schedule::Off, 60);

    let steps = core.onboarding();

    assert!(!steps.night_shift.ask_strength);
    assert!(steps.night_shift.ask_schedule);
    assert_eq!(steps.websites_from_config, Some(true));
}

#[test]
fn a_config_that_sets_website_checking_and_night_shift_leaves_onboarding_nothing_to_ask() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "websites = false\n[night_shift]\nstrength = 40\nschedule = \"off\"\n",
    );
    let (core, _wakes) = start(&socket);
    report_night_shift(&core, Schedule::Off, 60);

    assert!(!shows_onboarding(&core.handle(Event::Launched)));
}

#[test]
fn a_config_that_sets_only_website_checking_still_leaves_night_shift_to_ask() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = false\n");
    let (core, _wakes) = start(&socket);
    report_night_shift(&core, Schedule::Off, 60);

    assert!(shows_onboarding(&core.handle(Event::Launched)));
}

#[test]
fn a_config_that_sets_website_checking_leaves_onboarding_nothing_to_ask_without_night_shift() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = false\n");
    let (core, _wakes) = start(&socket);

    assert!(
        !core
            .handle(Event::Launched)
            .iter()
            .any(|effect| matches!(effect, Effect::ShowOnboarding { .. }))
    );
}

#[test]
fn a_self_managed_release_still_asks_about_opening_at_login_when_the_config_sets_website_checking()
{
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = false\n");
    let (core, _wakes) = start_as(&socket, Instance::Release, Install::SelfManaged);

    assert!(
        core.handle(Event::Launched)
            .contains(&Effect::ShowOnboarding { completed: 0 })
    );
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

    let Response::Ok(Payload::Status(mut status)) = query_status(&core, &socket, &wakes) else {
        panic!("expected a status");
    };
    // Checks have tests of their own.
    status.checks.clear();
    assert_eq!(
        status,
        Box::new(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: Install::Managed,
            night_shift: NightShift::Unavailable,
            strength: None,
            config_strength: None,
            decision: Decision {
                verdict: Verdict::HandsOff,
                reasons: Vec::new(),
            },
            manual_state: None,
            schedule: None,
            config_schedule: None,
            scheduled_state: None,
            next_transition: None,
            frontmost_app: None,
            watching: false,
            host: None,
            rules: Vec::new(),
            config_error: None,
            ad_hoc_error: None,
            website_checking: WebsiteChecking {
                state: OnOff::Off,
                from_config: false,
            },
            snoozes: Vec::new(),
            timed_disable: None,
            permissions: Permissions {
                accessibility: false,
                notifications: NotificationPermission::NotRequested,
            },
            checks: Vec::new(),
        })
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
fn a_self_managed_release_starts_its_updater_at_launch_and_registers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Release, Install::SelfManaged);

    assert_eq!(launch(&core), vec![Effect::StartUpdater]);
}

#[test]
fn a_managed_install_leaves_starting_at_login_to_its_installer() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Release, Install::Managed);

    assert_eq!(launch(&core), Vec::new());
}

#[test]
fn a_dev_build_never_registers_a_login_item() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Dev, Install::SelfManaged);

    assert_eq!(launch(&core), Vec::new());
}

#[test]
fn saved_ad_hoc_rules_load_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = adhoc::Store::new(dir.path().join("nightcap").join("rules.toml"));
    let rules = vec![figma(), RuleIdentity::Domain("figma.com".to_owned())];

    store.save(&rules).unwrap();

    assert_eq!(store.load(), Ok(rules));
}

#[test]
fn ad_hoc_rules_from_a_newer_build_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.toml");
    std::fs::write(
        &path,
        "version = 2\n[rules]\napp = [\"com.figma.Desktop\"]\n",
    )
    .unwrap();

    assert!(adhoc::Store::new(path).load().is_err());
}

#[test]
fn ad_hoc_rules_with_keys_this_build_doesnt_know_still_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.toml");
    std::fs::write(
        &path,
        "version = 1\nlabel = \"x\"\n[rules]\napp = [\"com.figma.Desktop\"]\nkeyword = []\n",
    )
    .unwrap();

    assert_eq!(adhoc::Store::new(path).load(), Ok(vec![figma()]));
}

#[test]
fn saved_state_loads_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("nightcap").join("state.json"));
    let state = Persisted {
        manual_state: Some(OnOff::Off),
        snoozes: vec![figma()],
        timed_disable: Some(at("2026-10-17T00:00:00+02:00")),
        websites: true,
        onboarding: 1,
        notifications_wanted: true,
        open_at_login: true,
        announced: vec![CheckKind::Accessibility],
        night_shift: Some(OnOff::Off),
        until: Some(at("2026-10-17T07:00:00+02:00")),
        applied_from_config: NightShiftConfig {
            strength: Some(40),
            schedule: Some(custom_hours()),
        },
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
        r#"{"version":1,"login_item_registered":true,"websites":true}"#,
    )
    .unwrap();

    assert!(Store::new(path).load().websites);
}

/// The check of `kind` in the status, if the core reports one.
fn check_of(
    core: &Core,
    socket: &Path,
    wakes: &mpsc::Receiver<()>,
    kind: CheckKind,
) -> Option<(CheckState, String)> {
    status_of(core, socket, wakes)
        .checks
        .into_iter()
        .find(|check| check.kind == kind)
        .map(|Check { state, text, .. }| (state, text))
}

fn stands(state: CheckState, text: &str) -> Option<(CheckState, String)> {
    Some((state, text.to_owned()))
}

fn permissions(accessibility: bool, notifications: NotificationPermission) -> Event {
    Event::PermissionsChanged {
        accessibility,
        notifications,
    }
}

#[test]
fn missing_accessibility_is_a_problem_only_while_website_checking_is_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(permissions(false, NotificationPermission::Granted));

    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::Accessibility),
        stands(CheckState::Off, "Not needed while website checking is off.")
    );
    send(&core, &socket, &wakes, Command::Websites(OnOff::On));
    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::Accessibility),
        stands(
            CheckState::Drifted,
            "Accessibility isn’t allowed, so website checking can’t read the front tab."
        )
    );
    core.handle(permissions(true, NotificationPermission::Granted));
    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::Accessibility),
        stands(CheckState::Ok, "Accessibility is allowed.")
    );
}

#[test]
fn notifications_switched_off_after_being_allowed_are_a_problem() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    let notifications = |core: &Core, socket: &Path, wakes: &mpsc::Receiver<()>| {
        check_of(core, socket, wakes, CheckKind::Notifications)
    };

    core.handle(permissions(true, NotificationPermission::Denied));
    assert_eq!(
        notifications(&core, &socket, &wakes),
        stands(CheckState::Off, "Notifications are off.")
    );
    core.handle(permissions(true, NotificationPermission::Granted));
    assert_eq!(
        notifications(&core, &socket, &wakes),
        stands(CheckState::Ok, "Allowed.")
    );
    core.handle(permissions(true, NotificationPermission::Denied));
    let drifted = stands(
        CheckState::Drifted,
        "Notifications were allowed, but they’re switched off in System Settings.",
    );
    assert_eq!(notifications(&core, &socket, &wakes), drifted);

    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-16T12:10:00+02:00");
    core.handle(permissions(true, NotificationPermission::Denied));
    assert_eq!(notifications(&core, &socket, &wakes), drifted);
}

#[test]
fn the_config_file_check_says_whether_the_file_is_missing_valid_or_broken() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    let config = |core: &Core| check_of(core, &socket, &wakes, CheckKind::ConfigFile);

    assert_eq!(config(&core), stands(CheckState::Off, "No config file."));
    write_config(&socket, "[rules]\napp = [\"com.figma.Desktop\"]\n");
    exchange(&core, &socket, &wakes, Request::Reload);
    assert_eq!(config(&core), stands(CheckState::Ok, "Valid, with 1 rule."));
    write_config(&socket, "[rules]\ndomian = [\"figma.com\"]\n");
    exchange(&core, &socket, &wakes, Request::Reload);
    let (state, text) = config(&core).unwrap();
    assert_eq!(state, CheckState::Broken);
    assert!(
        text.starts_with("The config file is invalid: ") && text.contains("domian"),
        "{text}"
    );
}

#[test]
fn the_ad_hoc_rules_check_counts_them_or_says_why_they_are_off() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::AdHocRules),
        stands(CheckState::Ok, "No ad-hoc rules.")
    );
    send(&core, &socket, &wakes, Command::AddRule(figma()));
    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::AdHocRules),
        stands(CheckState::Ok, "1 ad-hoc rule.")
    );

    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let rules_file = socket.parent().unwrap().with_file_name("rules.toml");
    std::fs::write(&rules_file, "version = 2\n").unwrap();
    let (core, wakes) = start(&socket);
    let (state, text) = check_of(&core, &socket, &wakes, CheckKind::AdHocRules).unwrap();
    assert_eq!(state, CheckState::Broken);
    assert!(text.starts_with("Ad-hoc rules are off: "), "{text}");
}

#[test]
fn night_shift_missing_on_this_mac_is_broken() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::NightShift),
        stands(
            CheckState::Broken,
            "Night Shift isn’t available on this Mac."
        )
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
    });
    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::NightShift),
        stands(CheckState::Ok, "Available, with no schedule.")
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
    });
    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::NightShift),
        stands(
            CheckState::Ok,
            "Available, following its schedule: 22:00 to 07:00."
        )
    );
}

/// The check of `kind` as Settings shows it.
fn settings_check(core: &Core, kind: CheckKind) -> Option<SettingsCheck> {
    core.settings()
        .checks
        .into_iter()
        .find(|check| check.kind == kind)
}

fn button(title: &str, fix: Fix) -> Option<FixButton> {
    Some(FixButton {
        title: title.to_owned(),
        fix,
    })
}

#[test]
fn settings_offer_to_allow_accessibility_or_stop_checking_websites() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "");
    let (core, _wakes) = start(&socket);
    core.handle(Event::CommandChosen {
        command: Command::Websites(OnOff::On),
    });
    core.handle(permissions(false, NotificationPermission::Granted));

    assert_eq!(
        settings_check(&core, CheckKind::Accessibility),
        Some(SettingsCheck {
            kind: CheckKind::Accessibility,
            state: CheckState::Drifted,
            text: "Accessibility isn’t allowed, so website checking can’t read the front tab."
                .to_owned(),
            restore: button("Allow…", Fix::AskForAccessibility),
            accept: button("Stop Checking Websites", Fix::StopCheckingWebsites),
        })
    );
    core.handle(Event::FixChosen {
        fix: Fix::StopCheckingWebsites,
    });
    assert!(!core.settings().website_checking.on);
    assert_eq!(
        settings_check(&core, CheckKind::Accessibility),
        Some(SettingsCheck {
            kind: CheckKind::Accessibility,
            state: CheckState::Off,
            text: "Not needed while website checking is off.".to_owned(),
            restore: None,
            accept: None,
        })
    );
}

#[test]
fn settings_offer_to_allow_notifications_or_keep_them_off() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes) = start(&socket);
    core.handle(permissions(true, NotificationPermission::Granted));
    core.handle(permissions(true, NotificationPermission::Denied));

    assert_eq!(
        settings_check(&core, CheckKind::Notifications),
        Some(SettingsCheck {
            kind: CheckKind::Notifications,
            state: CheckState::Drifted,
            text: "Notifications were allowed, but they’re switched off in System Settings."
                .to_owned(),
            restore: button("Allow Notifications…", Fix::AllowNotifications),
            accept: button("Keep Notifications Off", Fix::KeepNotificationsOff),
        })
    );
    core.handle(Event::FixChosen {
        fix: Fix::KeepNotificationsOff,
    });
    let kept_off = Some(SettingsCheck {
        kind: CheckKind::Notifications,
        state: CheckState::Off,
        text: "Notifications are off.".to_owned(),
        restore: button("Allow Notifications…", Fix::AllowNotifications),
        accept: None,
    });
    assert_eq!(settings_check(&core, CheckKind::Notifications), kept_off);

    let (core, _socket, _wakes, _clock) = restart_at(&socket, "2026-10-16T12:10:00+02:00");
    core.handle(permissions(true, NotificationPermission::Denied));
    assert_eq!(settings_check(&core, CheckKind::Notifications), kept_off);
}

#[test]
fn settings_offer_to_reload_a_broken_config() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules]\ndomian = [\"figma.com\"]\n");
    let (core, _wakes) = start(&socket);

    let broken = settings_check(&core, CheckKind::ConfigFile).unwrap();
    assert_eq!(broken.state, CheckState::Broken);
    assert_eq!(broken.restore, button("Reload", Fix::Reload));
    write_config(&socket, "[rules]\ndomain = [\"figma.com\"]\n");
    core.handle(Event::FixChosen { fix: Fix::Reload });
    assert_eq!(
        settings_check(&core, CheckKind::ConfigFile),
        Some(SettingsCheck {
            kind: CheckKind::ConfigFile,
            state: CheckState::Ok,
            text: "Valid, with 1 rule.".to_owned(),
            restore: None,
            accept: None,
        })
    );
}

fn login_item(item: LoginItem) -> Event {
    Event::LoginItemChanged { item }
}

#[test]
fn settings_switch_opening_at_login_in_a_self_managed_install() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Release, Install::SelfManaged);
    let switch = |on, command| {
        Some(SettingsSwitch {
            on,
            locked: false,
            command,
        })
    };
    core.handle(login_item(LoginItem::NotRegistered));

    assert_eq!(
        core.settings().open_at_login,
        switch(false, Command::OpenAtLogin(OnOff::On))
    );
    assert!(
        core.handle(Event::CommandChosen {
            command: Command::OpenAtLogin(OnOff::On),
        })
        .contains(&Effect::RegisterLoginItem)
    );
    core.handle(login_item(LoginItem::Enabled));
    assert_eq!(
        core.settings().open_at_login,
        switch(true, Command::OpenAtLogin(OnOff::Off))
    );
    assert!(
        core.handle(Event::CommandChosen {
            command: Command::OpenAtLogin(OnOff::Off),
        })
        .contains(&Effect::UnregisterLoginItem)
    );
}

#[test]
fn managed_installs_and_dev_builds_have_no_open_at_login_switch() {
    let dir = tempfile::tempdir().unwrap();
    let (managed, _wakes) = start_as(&socket_in(&dir), Instance::Release, Install::Managed);
    managed.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(managed.settings().open_at_login, None);

    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (dev, wakes) = start_as(&socket, Instance::Dev, Install::SelfManaged);
    dev.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(dev.settings().open_at_login, None);
    let (_, response) = send(&dev, &socket, &wakes, Command::OpenAtLogin(OnOff::On));
    assert_eq!(refusal(response), ErrorCode::NoLoginItem);
}

/// A self-managed release on `socket`'s state and config, restarted as launchd would.
fn restart_self_managed(socket: &Path) -> (Core, PathBuf, mpsc::Receiver<()>) {
    let restarted = socket
        .parent()
        .unwrap()
        .with_file_name("restarted")
        .join("nightcap.sock");
    let (core, wakes) = start_with(Setup {
        socket: restarted.clone(),
        ..setup(socket, Instance::Release, Install::SelfManaged)
    });
    (core, restarted, wakes)
}

fn login_check(
    state: CheckState,
    text: &str,
    restore: Option<FixButton>,
    accept: Option<FixButton>,
) -> Option<SettingsCheck> {
    Some(SettingsCheck {
        kind: CheckKind::OpenAtLogin,
        state,
        text: text.to_owned(),
        restore,
        accept,
    })
}

#[test]
fn opening_at_login_that_stops_after_the_user_chose_it_is_a_problem() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes) = start_as(&socket, Instance::Release, Install::SelfManaged);
    let off = login_check(
        CheckState::Off,
        "Nightcap doesn’t open at login.",
        None,
        None,
    );
    core.handle(login_item(LoginItem::NotRegistered));

    assert_eq!(settings_check(&core, CheckKind::OpenAtLogin), off);
    core.handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    core.handle(login_item(LoginItem::Enabled));
    assert_eq!(
        settings_check(&core, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Ok,
            "Nightcap opens when you log in.",
            None,
            None
        )
    );
    core.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(
        settings_check(&core, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Drifted,
            "Nightcap no longer opens at login: it was removed in System Settings.",
            button("Open at Login", Fix::OpenAtLogin),
            button("Don’t Open at Login", Fix::DontOpenAtLogin),
        )
    );
    assert!(
        core.handle(Event::FixChosen {
            fix: Fix::OpenAtLogin
        })
        .contains(&Effect::RegisterLoginItem)
    );
    core.handle(Event::FixChosen {
        fix: Fix::DontOpenAtLogin,
    });
    assert_eq!(settings_check(&core, CheckKind::OpenAtLogin), off);

    let (core, _socket, _wakes) = restart_self_managed(&socket);
    core.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(settings_check(&core, CheckKind::OpenAtLogin), off);
}

#[test]
fn opening_at_login_waiting_for_approval_offers_login_items() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Release, Install::SelfManaged);
    core.handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    core.handle(login_item(LoginItem::RequiresApproval));

    assert_eq!(
        settings_check(&core, CheckKind::OpenAtLogin),
        login_check(
            CheckState::Drifted,
            "macOS is waiting for your approval in Login Items.",
            button("Open Login Items…", Fix::OpenLoginItems),
            button("Don’t Open at Login", Fix::DontOpenAtLogin),
        )
    );
    assert!(
        core.handle(Event::FixChosen {
            fix: Fix::DontOpenAtLogin
        })
        .contains(&Effect::UnregisterLoginItem)
    );
}

#[test]
fn a_login_item_added_in_system_settings_becomes_the_users_choice() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Release, Install::SelfManaged);

    core.handle(login_item(LoginItem::Enabled));
    core.handle(login_item(LoginItem::NotRegistered));
    assert_eq!(
        settings_check(&core, CheckKind::OpenAtLogin).map(|check| check.state),
        Some(CheckState::Drifted)
    );
}

#[test]
fn opening_at_login_removed_in_system_settings_is_announced() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start_as(&socket_in(&dir), Instance::Release, Install::SelfManaged);
    core.handle(Event::CommandChosen {
        command: Command::OpenAtLogin(OnOff::On),
    });
    core.handle(login_item(LoginItem::Enabled));

    assert!(
        core.handle(login_item(LoginItem::NotRegistered))
            .contains(&Effect::Notify {
                title: "Nightcap no longer opens at login".to_owned(),
                body: "Open Nightcap’s settings to turn it back on, or to keep it off.".to_owned(),
            })
    );
}

#[test]
fn the_rules_tab_lists_every_rule_with_what_can_be_done_about_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "[rules]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n",
    );
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
    });
    let photos = RuleIdentity::App("com.apple.Photos".to_owned());
    send(&core, &socket, &wakes, Command::AddRule(photos.clone()));
    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule(domain("youtube.com")),
    );
    send(&core, &socket, &wakes, Command::Snooze(domain("figma.com")));
    let rule = |identity: RuleIdentity, label: &str, snoozed, title: &str, command| SettingsRule {
        identity,
        label: label.to_owned(),
        snoozed,
        action: RuleAction {
            title: title.to_owned(),
            command,
        },
    };

    assert_eq!(
        core.settings().rules,
        vec![
            rule(
                figma(),
                "From your config",
                false,
                "Snooze",
                Command::Snooze(figma())
            ),
            rule(
                domain("figma.com"),
                "Snoozed until 07:00",
                true,
                "Unsnooze",
                Command::Unsnooze(domain("figma.com"))
            ),
            rule(
                photos.clone(),
                "Ad-hoc",
                false,
                "Remove",
                Command::RemoveRule(photos)
            ),
            rule(
                domain("youtube.com"),
                "Ad-hoc",
                false,
                "Remove",
                Command::RemoveRule(domain("youtube.com"))
            ),
        ]
    );
}

#[test]
fn the_rules_tab_shows_where_the_config_is_and_copies_the_ad_hoc_rules_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    send(&core, &socket, &wakes, Command::AddRule(figma()));
    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule(domain("figma.com")),
    );

    assert_eq!(
        core.settings().config_path,
        config_beside(&socket).display().to_string()
    );
    assert_eq!(
        core.export(ExportFormat::Nix),
        "{ app = [ \"com.figma.Desktop\" ]; domain = [ \"figma.com\" ]; }\n"
    );
    assert_eq!(
        core.export(ExportFormat::Toml),
        "[rules]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n"
    );
}

#[test]
fn a_typed_domain_is_normalized_or_turned_down() {
    assert_eq!(
        normalize_domain("Docs.Figma.COM".to_owned()),
        Some("docs.figma.com".to_owned())
    );
    assert_eq!(normalize_domain("https://figma.com/file".to_owned()), None);
    assert_eq!(normalize_domain("*.figma.com".to_owned()), None);
}

// The tour (ADR-0022).

const SAMPLE: &str = "io.github.choco.nightcap.tour-sample";

fn sample_rule() -> Rule {
    Rule {
        identity: RuleIdentity::App(SAMPLE.to_owned()),
        source: RuleSource::Tour,
    }
}

fn start_tour(core: &Core, sample: bool) -> Vec<Effect> {
    core.handle(Event::TourStarted {
        sample_app: sample.then(|| SAMPLE.to_owned()),
    })
}

fn report(core: &Core, enabled: bool) -> Vec<Effect> {
    core.handle(Event::StatusChanged {
        enabled,
        schedule: custom_hours(),
    })
}

/// Starts at 20:00, with Night Shift off by the custom hours until 22:00.
fn start_off_by_the_schedule(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    let (core, wakes, clock) = start_at(socket, "2026-10-16T20:00:00+02:00");
    report(&core, false);
    (core, wakes, clock)
}

/// Starts at 23:00, with Night Shift on by the custom hours until 07:00.
fn start_on_by_the_schedule(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    let (core, wakes, clock) = start_at(socket, "2026-10-16T23:00:00+02:00");
    report(&core, true);
    (core, wakes, clock)
}

fn on_for_the_tour() -> Decision {
    Decision {
        verdict: Verdict::HandsOff,
        reasons: vec![Reason::Tour],
    }
}

#[test]
fn a_tour_started_with_night_shift_off_by_the_schedule_turns_it_on_for_the_tour() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_off_by_the_schedule(&socket);

    assert_eq!(
        start_tour(&core, false),
        vec![Effect::SetNightShift { enabled: true }]
    );
    assert_eq!(report(&core, true), Vec::new());

    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.decision, on_for_the_tour());
    assert_eq!(status.manual_state, None);
}

#[test]
fn the_tours_end_turns_night_shift_back_off_following_the_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_off_by_the_schedule(&socket);
    start_tour(&core, false);
    report(&core, true);

    assert_eq!(
        core.handle(Event::TourEnded),
        vec![Effect::SetNightShift { enabled: false }]
    );
    assert_eq!(report(&core, false), Vec::new());

    let status = status_of(&core, &socket, &wakes);
    assert_eq!(
        status.decision.reasons,
        vec![Reason::Schedule {
            state: OnOff::Off,
            until: Some(at("2026-10-16T22:00:00+02:00")),
        }]
    );
    assert_eq!(status.manual_state, None);
}

#[test]
fn a_tour_over_a_manual_off_puts_it_back_when_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    send(&core, &socket, &wakes, Command::Off);
    report(&core, false);

    assert_eq!(
        start_tour(&core, false),
        vec![Effect::SetNightShift { enabled: true }]
    );
    report(&core, true);
    assert_eq!(
        core.handle(Event::TourEnded),
        vec![Effect::SetNightShift { enabled: false }]
    );

    assert_eq!(
        status_of(&core, &socket, &wakes).manual_state,
        Some(OnOff::Off)
    );
}

#[test]
fn a_tour_over_a_timed_disable_puts_it_back_with_its_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    send(
        &core,
        &socket,
        &wakes,
        Command::Disable(SignedDuration::from_hours(1)),
    );
    report(&core, false);

    assert_eq!(
        start_tour(&core, false),
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    report(&core, true);
    assert_eq!(
        core.handle(Event::TourEnded),
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T00:00:00+02:00").into()
            },
        ]
    );

    assert_eq!(
        status_of(&core, &socket, &wakes).decision.reasons,
        vec![Reason::TimedDisable {
            until: at("2026-10-17T00:00:00+02:00")
        }]
    );
}

#[test]
fn while_the_tour_stands_in_a_tap_off_and_on_returns_to_the_tours_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_off_by_the_schedule(&socket);
    start_tour(&core, false);
    report(&core, true);

    let (effects, _) = send(&core, &socket, &wakes, Command::Toggle);
    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    report(&core, false);
    let (effects, response) = send(&core, &socket, &wakes, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, Response::Ok(Payload::Decision(on_for_the_tour())));
    assert_eq!(status_of(&core, &socket, &wakes).manual_state, None);
}

#[test]
fn a_tour_started_with_night_shift_on_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);

    assert_eq!(start_tour(&core, false), Vec::new());
    assert!(matches!(
        status_of(&core, &socket, &wakes).decision.reasons[..],
        [Reason::Schedule { .. }]
    ));
    assert_eq!(core.handle(Event::TourEnded), Vec::new());
}

#[test]
fn the_tour_sample_in_front_forces_night_shift_off_by_its_rule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    start_tour(&core, true);

    assert_eq!(
        activate(&core, SAMPLE),
        vec![Effect::SetNightShift { enabled: false }]
    );
    report(&core, false);
    assert_eq!(
        status_of(&core, &socket, &wakes).decision,
        Decision {
            verdict: Verdict::ForceOff,
            reasons: vec![Reason::Rule(sample_rule())],
        }
    );
    assert_eq!(
        activate(&core, "com.apple.mail"),
        vec![Effect::SetNightShift { enabled: true }]
    );
}

#[test]
fn outside_a_tour_the_sample_matches_no_rule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes, _clock) = start_on_by_the_schedule(&socket);
    assert_eq!(activate(&core, SAMPLE), Vec::new());

    start_tour(&core, true);
    activate(&core, "com.apple.mail");
    core.handle(Event::TourEnded);

    assert_eq!(activate(&core, SAMPLE), Vec::new());
}

#[test]
fn the_tour_samples_rule_is_never_listed_or_saved_and_goes_with_its_snooze() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    start_tour(&core, true);
    let (_, response) = send(
        &core,
        &socket,
        &wakes,
        Command::Snooze(RuleIdentity::App(SAMPLE.to_owned())),
    );
    assert!(matches!(response, Response::Ok(_)), "{response:?}");

    assert!(core.settings().rules.is_empty());
    assert!(!core.export(ExportFormat::Toml).contains(SAMPLE));
    let saved = std::fs::read_to_string(socket.with_file_name("state.json")).unwrap();
    assert!(!saved.contains(SAMPLE), "{saved}");

    core.handle(Event::TourEnded);
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.rules, Vec::new());
    assert_eq!(status.snoozes, Vec::new());
}

#[test]
fn a_restart_after_a_crash_mid_tour_puts_night_shift_back_where_the_tour_found_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (crashed, wakes, _clock) = start_on_by_the_schedule(&socket);
    send(&crashed, &socket, &wakes, Command::Off);
    report(&crashed, false);
    start_tour(&crashed, false);
    report(&crashed, true);

    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-16T23:10:00+02:00");
    let effects = report(&core, true);

    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(
        status_of(&core, &socket, &wakes).manual_state,
        Some(OnOff::Off)
    );
}

#[test]
fn quitting_mid_tour_ends_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes, _clock) = start_off_by_the_schedule(&socket);
    start_tour(&core, false);
    report(&core, true);

    assert_eq!(
        core.handle(Event::Terminating),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn starting_the_tour_again_keeps_what_the_first_start_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    send(&core, &socket, &wakes, Command::Off);
    report(&core, false);
    start_tour(&core, false);
    report(&core, true);

    assert_eq!(start_tour(&core, true), Vec::new());
    core.handle(Event::TourEnded);

    assert_eq!(
        status_of(&core, &socket, &wakes).manual_state,
        Some(OnOff::Off)
    );
}
