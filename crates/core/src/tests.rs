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
    HelloReply, Install, Instance, Layer, Length, NightShift, NotificationPermission, OnOff,
    Payload, Permissions, Place, PlaceLayers, Query, Reason, Request, Response, Rule, RuleIdentity,
    RuleSource, Schedule, Status, TimeOfDay, WebsiteChecking,
};

use crate::adhoc;
use crate::config::NightShiftConfig;
use crate::state::{Persisted, Store};
use crate::{
    Clock, Core, CoreError, Effect, Event, Fix, FixButton, LoginItem, MenuEntry, MenuIcon,
    MenuItem, NightShiftItem, OnboardingNightShift, OnboardingSteps, PlaceRow, ProblemLine,
    RowImage, RowItem, RuleAction, SettingsCheck, SettingsLayers, SettingsRule, SettingsSwitch,
    Setup, Waker, normalize_domain,
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
        identity: RuleIdentity::App(bundle_id.parse().unwrap()),
        source: RuleSource::Config,
        says: OnOff::Off,
    }
}

/// Starts at 23:00 with Night Shift on by the custom hours, then brings Figma, which a config
/// rule forces off, to the front, and reports the result.
fn start_forced(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    write_config(socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, clock) = start_at(socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    assert_eq!(
        activate(&core, "com.figma.Desktop"),
        vec![Effect::SetNightShift { enabled: false }]
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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
        says: OnOff::Off,
    }
}

/// Adds an ad-hoc rule that says off.
fn add_rule(identity: RuleIdentity) -> Command {
    Command::AddRule {
        identity,
        says: OnOff::Off,
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
    RuleIdentity::App("com.figma.Desktop".parse().unwrap())
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
        identity: RuleIdentity::Domain(domain.parse().unwrap()),
        source: RuleSource::Config,
        says: OnOff::Off,
    }
}

/// Starts at 23:00 with Night Shift on by the custom hours, website checking on and a config
/// rule for figma.com.
fn start_with_figma_domain(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    write_config(
        socket,
        "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n",
    );
    let (core, wakes, clock) = start_at(socket, "2026-10-16T23:00:00+02:00");
    grant_accessibility(&core);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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
        assert_eq!(
            client.hello(nightcap_protocol::PROTOCOL_VERSION),
            HelloReply::Hello(Hello::current())
        );
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
    exchange_line(
        core,
        socket,
        wakes,
        serde_json::to_string(&request).unwrap(),
    )
}

/// Sends one line as the request, which the IPC thread passes on for a tick.
fn exchange_line(
    core: &Core,
    socket: &Path,
    wakes: &mpsc::Receiver<()>,
    line: String,
) -> (Vec<Effect>, Response) {
    let socket = socket.to_owned();
    let client = thread::spawn(move || {
        let mut client = Client::connect(&socket);
        assert_eq!(
            client.hello(nightcap_protocol::PROTOCOL_VERSION),
            HelloReply::Hello(Hello::current())
        );
        client.send(&line);
        client.receive::<Response>()
    });
    wakes
        .recv_timeout(Duration::from_secs(5))
        .expect("the IPC thread asks for a tick");
    let effects = core.tick();
    (effects, client.join().unwrap())
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
        sun_schedule_set_aside: false,
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
        sun_schedule_set_aside: false,
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
        sun_schedule_set_aside: false,
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
        sun_schedule_set_aside: false,
    });

    send(&core, &socket, &wakes, Command::SetSchedule(Schedule::Off));
    // macOS switches Night Shift off along with the schedule.
    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
    });

    assert!(effects.contains(&Effect::SetNightShift { enabled: true }));
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, None)),
            None
        )]
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
        sun_schedule_set_aside: false,
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

/// The answer to a command that leaves Night Shift on at 23:00 by the custom hours.
fn following_the_custom_hours_until_morning() -> Response {
    Response::Ok(Payload::Decision(Decision {
        state: Some(OnOff::On),
        place: Place::Everywhere,
        reasons: vec![Reason::Schedule {
            state: OnOff::On,
            until: Some(at("2026-10-17T07:00:00+02:00")),
        }],
    }))
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

/// Everywhere held `state` until the user turns it back, over no schedule.
fn held(state: OnOff) -> Response {
    Response::Ok(Payload::Decision(Decision {
        state: Some(state),
        place: Place::Everywhere,
        reasons: vec![
            Reason::Hold(layer(state, None)),
            Reason::Schedule {
                state: OnOff::Off,
                until: None,
            },
        ],
    }))
}

/// Everywhere off by having no schedule, with nothing over it.
fn off_with_no_schedule() -> Response {
    Response::Ok(Payload::Decision(Decision {
        state: Some(OnOff::Off),
        place: Place::Everywhere,
        reasons: vec![Reason::Schedule {
            state: OnOff::Off,
            until: None,
        }],
    }))
}

fn layer(state: OnOff, until: Option<&str>) -> Layer {
    Layer {
        state,
        until: until.map(at),
        with: None,
    }
}

/// A place's layers as the status lists them.
fn layered(place: Place, hold: Option<Layer>, timer: Option<Layer>) -> PlaceLayers {
    PlaceLayers { place, hold, timer }
}

fn set(place: Place, state: OnOff, length: Length) -> Command {
    Command::Set {
        place,
        state,
        length,
    }
}

fn minutes(count: i64) -> Length {
    Length::For(SignedDuration::from_mins(count))
}

#[test]
fn on_turns_night_shift_on_everywhere_until_turned_back() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, held(OnOff::On));
}

#[test]
fn off_takes_off_everywheres_hold_where_that_is_enough() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    // Found on with no schedule, which the core takes as the user's own on.
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::Off);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(response, off_with_no_schedule());
}

#[test]
fn on_while_everywhere_says_on_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(effects, Vec::new());
    assert_eq!(response, held(OnOff::On));
}

#[test]
fn toggle_turns_everywhere_the_other_way() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(response, off_with_no_schedule());
}

#[test]
fn two_flips_leave_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(&core, &socket, &wakes, Command::Toggle);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn on_while_the_schedule_says_on_holds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let (effects, response) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(effects, Vec::new());
    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn a_hold_with_no_end_outlasts_sunrise_and_sunset() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(&core, &socket, &wakes, Command::On);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    clock.set("2026-10-17T07:00:00+02:00");
    core.handle(Event::TimerFired);
    // macOS performs its own transition, and Nightcap puts Night Shift back on.
    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, None)),
            None
        )]
    );
}

#[test]
fn an_external_change_holds_everywhere_until_the_schedule_turns_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(effects, Vec::new());
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn an_external_change_back_takes_off_what_the_switch_held() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(&core, &socket, &wakes, Command::On);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(effects, Vec::new());
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn an_edited_schedule_ends_the_holds_waiting_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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
        sun_schedule_set_aside: false,
    });

    assert_eq!(effects, Vec::new());
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn an_edited_schedule_keeps_timers_and_holds_with_no_end() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(&core, &socket, &wakes, Command::Off);
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::On, minutes(30)),
    );

    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::SunsetToSunrise,
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            Some(layer(OnOff::On, Some("2026-10-16T23:30:00+02:00")))
        )]
    );
}

#[test]
fn a_hold_until_the_schedule_turns_ends_at_its_turn_even_asleep() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::On, Length::UntilScheduleTurns),
    );

    clock.set("2026-10-17T08:00:00+02:00");
    let effects = core.handle(Event::Woke);

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T22:00:00+02:00").into()
        }]
    );
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn a_timer_gives_back_what_was_under_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let (effects, _) = send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, minutes(30)),
    );
    assert_eq!(
        effects,
        vec![
            Effect::SetNightShift { enabled: false },
            Effect::ScheduleWakeup {
                at: at("2026-10-16T23:30:00+02:00").into()
            },
        ]
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    clock.set("2026-10-16T23:30:00+02:00");
    assert_eq!(
        core.handle(Event::TimerFired),
        vec![
            Effect::SetNightShift { enabled: true },
            Effect::ScheduleWakeup {
                at: at("2026-10-17T07:00:00+02:00").into()
            },
        ]
    );
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn a_new_timer_replaces_the_places_timer_and_keeps_its_hold() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(&core, &socket, &wakes, Command::Off);
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::On, minutes(30)),
    );

    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::On, minutes(120)),
    );

    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            Some(layer(OnOff::On, Some("2026-10-17T01:00:00+02:00")))
        )]
    );
}

#[test]
fn until_the_schedule_turns_ends_at_sunrise_for_on_and_at_sunset_for_off() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::On, Length::UntilScheduleTurns),
    );
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );

    clock.set("2026-10-16T23:00:00+02:00");
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
    );
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, Some("2026-10-17T22:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn a_new_hold_replaces_the_places_hold_and_ends_its_timer() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(&core, &socket, &wakes, Command::Off);
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::On, minutes(30)),
    );

    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
    );

    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, Some("2026-10-17T22:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn until_a_picked_time_is_a_timer_that_ends_then() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let (effects, _) = send(
        &core,
        &socket,
        &wakes,
        set(
            Place::Everywhere,
            OnOff::Off,
            Length::Until(at("2026-10-17T00:15:00+02:00")),
        ),
    );

    assert!(effects.contains(&Effect::ScheduleWakeup {
        at: at("2026-10-17T00:15:00+02:00").into()
    }));
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            None,
            Some(layer(OnOff::Off, Some("2026-10-17T00:15:00+02:00")))
        )]
    );
}

#[test]
fn lengths_that_cant_be_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
    });

    for command in [
        set(Place::Everywhere, OnOff::Off, minutes(0)),
        set(
            Place::Everywhere,
            OnOff::Off,
            Length::Until(at("2026-10-16T22:00:00+02:00")),
        ),
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
        set(
            Place::App("com.figma.Desktop".parse().unwrap()),
            OnOff::Off,
            Length::UntilTurnedBack,
        ),
    ] {
        let (effects, response) = send(&core, &socket, &wakes, command.clone());
        assert_eq!(effects, Vec::new(), "{command:?}");
        assert_eq!(refusal(response), ErrorCode::BadRequest, "{command:?}");
    }
}

#[test]
fn the_status_reports_the_schedule_its_scheduled_state_and_the_next_transition() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");

    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+02:00").into()
        }]
    );
}

#[test]
fn a_timer_that_fires_before_its_transition_is_set_again() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, clock) = start_at(&socket_in(&dir), "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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
fn a_change_just_before_a_schedule_transition_is_that_transition() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, Length::UntilScheduleTurns),
    );

    clock.set("2026-10-16T21:59:00+02:00");
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.layers, Vec::new());
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
        sun_schedule_set_aside: false,
    });
    send(&core, &socket, &wakes, Command::On);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    clock.set("2026-10-16T21:59:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(
        status.next_transition,
        Some(at("2026-10-16T22:00:00+02:00"))
    );
}

#[test]
fn a_time_zone_change_moves_the_custom_hours() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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

/// The sun times CoreBrightness reported in Zurich around the night of 15 October 2026.
fn report_zurich_sun(core: &Core) {
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
}

#[test]
fn sunset_to_sunrise_follows_the_reported_sun_times() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T01:40:00+02:00");

    report_zurich_sun(&core);
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::SunsetToSunrise,
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T05:50:47Z").into()
        }]
    );
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.scheduled_state, Some(OnOff::On));
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn until_corebrightness_has_the_sun_times_the_shell_reads_them_again_each_minute() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T01:40:20+02:00");

    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::SunsetToSunrise,
        sun_schedule_set_aside: false,
    });
    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T01:41:00+02:00").into()
        }]
    );
    clock.set("2026-10-16T01:40:50+02:00");
    assert_eq!(core.handle(Event::Woke), Vec::new());

    clock.set("2026-10-16T01:41:00+02:00");
    assert_eq!(
        core.handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T01:42:00+02:00").into()
        }]
    );

    clock.set("2026-10-16T01:42:00+02:00");
    report_zurich_sun(&core);
    assert_eq!(
        core.handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T05:50:47Z").into()
        }]
    );
    assert_eq!(
        status_of(&core, &socket, &wakes).scheduled_state,
        Some(OnOff::On)
    );
}

#[test]
fn turning_the_schedule_off_cancels_the_wakeup() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T20:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
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
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let effects = activate(&core, "com.figma.Desktop");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.frontmost_app.as_deref(), Some("com.figma.Desktop"));
    assert_eq!(
        status.decision,
        Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }
    );
}

#[test]
fn leaving_the_app_hands_night_shift_back_to_the_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, _wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.figma.Desktop");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let effects = activate(&core, "com.apple.Terminal");

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
}

#[test]
fn an_invalid_config_at_launch_runs_without_config_rules_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napps = [\"com.figma.Desktop\"]\n");
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
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.figma.Desktop");

    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
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
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes) = start(&socket);

    write_config(&socket, "[rules.off]\napp = \"com.figma.Desktop\"\n");
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
        sun_schedule_set_aside: false,
    });

    let (_, response) = send(&core, &socket, &wakes, Command::Off);

    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: Place::Everywhere,
            reasons: vec![
                Reason::Hold(layer(OnOff::Off, None)),
                Reason::Schedule {
                    state: OnOff::On,
                    until: Some(at("2026-10-17T07:00:00+02:00")),
                },
            ],
        }))
    );
}

#[test]
fn macos_performing_a_transition_just_after_the_core_applied_it_is_not_an_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T21:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.figma.Desktop");
    clock.set("2026-10-16T22:00:00+02:00");
    core.handle(Event::TimerFired);

    clock.set("2026-10-16T22:01:00+02:00");
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn after_waking_macos_catching_up_on_a_missed_transition_is_not_an_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, clock) = start_at(&socket, "2026-10-16T21:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.figma.Desktop");
    clock.set("2026-10-16T23:00:00+02:00");
    core.handle(Event::Woke);

    clock.set("2026-10-16T23:03:00+02:00");
    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

fn figma_app() -> Place {
    Place::App("com.figma.Desktop".parse().unwrap())
}

#[test]
fn in_front_of_a_rule_the_switch_changes_everywhere_and_the_rule_keeps_its_say() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);

    let (effects, response) = send(&core, &socket, &wakes, Command::Off);

    assert_eq!(effects, Vec::new());
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }))
    );
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            None
        )]
    );
    assert_eq!(activate(&core, "com.apple.Terminal"), Vec::new());
}

#[test]
fn setting_a_rules_place_the_other_way_snoozes_the_rule_for_that_length() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);

    let (effects, response) = send(
        &core,
        &socket,
        &wakes,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::On),
            place: figma_app(),
            reasons: vec![
                Reason::Hold(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
                Reason::Rule(app_rule("com.figma.Desktop")),
            ],
        }))
    );
}

#[test]
fn a_snoozed_rule_comes_back_when_its_snooze_ends() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_forced(&socket);
    send(
        &core,
        &socket,
        &wakes,
        set(figma_app(), OnOff::On, minutes(30)),
    );
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    clock.set("2026-10-16T23:30:00+02:00");
    let effects = core.handle(Event::TimerFired);

    assert!(effects.contains(&Effect::SetNightShift { enabled: false }));
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.layers, Vec::new());
    assert_eq!(status.decision.place, figma_app());
}

#[test]
fn a_snooze_stays_with_its_rule_when_another_app_comes_to_the_front() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    send(
        &core,
        &socket,
        &wakes,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(activate(&core, "com.apple.Terminal"), Vec::new());
    assert_eq!(activate(&core, "com.figma.Desktop"), Vec::new());
    assert_eq!(
        status_of(&core, &socket, &wakes).decision.state,
        Some(OnOff::On)
    );
}

#[test]
fn an_external_on_in_front_of_a_rule_snoozes_it_until_the_schedule_turns_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);

    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        effects,
        vec![Effect::Notify {
            title: "Night Shift turned on outside Nightcap".to_owned(),
            body: "Snoozed the com.figma.Desktop rule until 07:00.".to_owned(),
            opens: None,
        }]
    );
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            figma_app(),
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );
}

#[test]
fn with_no_schedule_the_night_shift_item_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T12:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        core.menu().night_shift.reason.as_deref(),
        Some("No schedule")
    );
}

#[test]
fn with_no_schedule_an_external_change_holds_until_turned_back() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
    });

    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::On, None)),
            None
        )]
    );
}

#[test]
fn flipping_back_from_outside_takes_off_both_linked_holds() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T12:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.figma.Desktop");

    let effects = core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        effects,
        vec![Effect::Notify {
            title: "Night Shift turned on outside Nightcap".to_owned(),
            body: "Snoozed the com.figma.Desktop rule until 07:00.".to_owned(),
            opens: None,
        }]
    );
    let morning = Some(at("2026-10-17T07:00:00+02:00"));
    let linked = |with| {
        Some(Layer {
            state: OnOff::On,
            until: morning,
            with: Some(with),
        })
    };
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![
            layered(figma_app(), linked(Place::Everywhere), None),
            layered(Place::Everywhere, linked(figma_app()), None),
        ]
    );

    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
    assert_eq!(activate(&core, "com.apple.mail"), Vec::new());
}

#[test]
fn the_switch_first_takes_off_everywheres_layers_that_say_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, minutes(60)),
    );

    let (_, response) = send(&core, &socket, &wakes, Command::On);

    assert_eq!(response, following_the_custom_hours_until_morning());
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn turning_night_shift_off_everywhere_leaves_a_snoozed_rule_warm() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    report(&core, true);
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });
    let figma = Place::App("com.figma.Desktop".parse().unwrap());
    send(
        &core,
        &socket,
        &wakes,
        set(figma.clone(), OnOff::On, minutes(30)),
    );
    report(&core, true);

    let (effects, response) = send(&core, &socket, &wakes, Command::Off);

    assert_eq!(effects, Vec::new());
    let Response::Ok(Payload::Decision(decision)) = response else {
        panic!("{response:?}");
    };
    assert_eq!((decision.state, decision.place), (Some(OnOff::On), figma));
}

#[test]
fn reset_puts_a_snoozed_rule_back_early() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    send(
        &core,
        &socket,
        &wakes,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );

    let (_, response) = send(&core, &socket, &wakes, Command::Reset(figma_app()));

    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
            reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
        }))
    );
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

/// Snoozes the Figma rule until 07:00, then turns Night Shift off everywhere for an hour.
fn snooze_figma_and_break(core: &Core, socket: &Path, wakes: &mpsc::Receiver<()>) {
    send(
        core,
        socket,
        wakes,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );
    send(
        core,
        socket,
        wakes,
        set(Place::Everywhere, OnOff::Off, minutes(60)),
    );
}

#[test]
fn holds_and_timers_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    snooze_figma_and_break(&core, &socket, &wakes);

    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-16T23:30:00+02:00");

    assert_eq!(
        launch(&core),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T00:00:00+02:00").into()
        }]
    );
    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![
            layered(
                figma_app(),
                Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
                None
            ),
            layered(
                Place::Everywhere,
                None,
                Some(layer(OnOff::Off, Some("2026-10-17T00:00:00+02:00")))
            ),
        ]
    );
}

#[test]
fn holds_and_timers_that_ended_while_nightcap_was_stopped_are_dropped_on_start() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    snooze_figma_and_break(&core, &socket, &wakes);

    // Past both the timer's end and the 07:00 schedule transition.
    let (core, socket, wakes, _clock) = restart_at(&socket, "2026-10-17T08:00:00+02:00");

    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn on_the_way_out_only_everywheres_hold_still_applies() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(&core, &socket, &wakes, Command::Off);
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::On, minutes(30)),
    );
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        core.handle(Event::Terminating),
        vec![Effect::SetNightShift { enabled: false }]
    );
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
        sun_schedule_set_aside: false,
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
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
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
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, Some("2026-10-17T22:00:00+02:00"))),
            None
        )]
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
        sun_schedule_set_aside: false,
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
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
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

/// Starts at 23:00 with Night Shift on by the custom hours, then turns it off for an hour.
fn start_disabled(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    let (core, wakes, clock) = start_at(socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    send(
        &core,
        socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, minutes(60)),
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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
        sun_schedule_set_aside: false,
    });

    assert_eq!(effects, Vec::new());
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
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
    // The shell's fresh read: still off, as the timer left it.
    let effects = core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
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
        sun_schedule_set_aside: false,
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
fn with_website_checking_on_the_front_browser_is_watched_without_any_domain_rule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = true\n");
    let (core, _wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    grant_accessibility(&core);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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
    write_config(&socket, "[rules.off]\ndomain = [\"Bücher.DE.\"]\n");
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
        write_config(&socket, &format!("[rules.off]\ndomain = [{entry:?}]\n"));
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
fn an_added_rule_applies_at_once_and_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.figma.Desktop");

    let (effects, response) = send(&core, &socket, &wakes, add_rule(figma()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: figma_app(),
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
fn an_added_domain_is_normalized_and_a_bad_identity_refused_before_the_core() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    let add = |identity: &str| {
        format!(r#"{{"command":{{"add_rule":{{"identity":"{identity}","says":"off"}}}}}}"#)
    };

    exchange_line(&core, &socket, &wakes, add("domain:Figma.COM"));

    assert_eq!(
        rules_of(&core, &socket, &wakes),
        vec![ad_hoc(RuleIdentity::Domain("figma.com".parse().unwrap()))]
    );
    for bad in ["domain:https://figma.com", "app:com.figma.Desktop/", "app:"] {
        let mut client = Client::connect(&socket);
        client.hello(nightcap_protocol::PROTOCOL_VERSION);
        client.send(&add(bad));
        assert_eq!(refusal(client.receive()), ErrorCode::BadRequest, "{bad}");
    }
    assert!(wakes.try_recv().is_err());
}

#[test]
fn removing_an_ad_hoc_rule_lifts_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.figma.Desktop");
    send(&core, &socket, &wakes, add_rule(figma()));
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let (effects, _) = send(&core, &socket, &wakes, Command::RemoveRule(figma()));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(rules_of(&core, &socket, &wakes), Vec::new());
}

#[test]
fn a_rule_that_doesnt_exist_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    let (_, removed) = send(&core, &socket, &wakes, Command::RemoveRule(figma()));

    assert_eq!(refusal(removed), ErrorCode::NoSuchRule);
}

#[test]
fn ad_hoc_rules_from_a_newer_build_are_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let rules_file = socket.parent().unwrap().with_file_name("rules.toml");
    let newer = "version = 3\n[rules.off]\napp = [\"com.figma.Desktop\"]\n";
    std::fs::write(&rules_file, newer).unwrap();
    let (core, wakes) = start(&socket);

    let (_, response) = send(&core, &socket, &wakes, add_rule(figma()));

    assert_eq!(refusal(response), ErrorCode::AdHocRulesOff);
    assert!(status_of(&core, &socket, &wakes).ad_hoc_error.is_some());
    assert_eq!(rules_of(&core, &socket, &wakes), Vec::new());
    assert_eq!(std::fs::read_to_string(&rules_file).unwrap(), newer);
}

#[test]
fn ad_hoc_rules_with_an_app_no_rule_could_name_are_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let rules_file = socket.parent().unwrap().with_file_name("rules.toml");
    let invalid = "version = 2\n[rules.off]\napp = [\"com.figma.Desktop \"]\n";
    std::fs::write(&rules_file, invalid).unwrap();
    let (core, wakes) = start(&socket);

    let status = status_of(&core, &socket, &wakes);

    assert!(
        status
            .ad_hoc_error
            .is_some_and(|error| error.contains("\"com.figma.Desktop \" isn't a bundle ID")),
    );
    assert_eq!(status.rules, Vec::new());
    assert_eq!(std::fs::read_to_string(&rules_file).unwrap(), invalid);
}

#[test]
fn an_app_whose_bundle_id_no_rule_could_name_counts_as_having_none() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    activate(&core, "unity.Team Cherry.Hollow Knight");

    assert_eq!(status_of(&core, &socket, &wakes).frontmost_app, None);
    let app_rows = core.menu().entries.into_iter().filter(|entry| {
        matches!(entry, MenuEntry::Row { row } if matches!(row.image, RowImage::App { .. }))
    });
    assert_eq!(app_rows.count(), 0);
}

#[test]
fn the_rules_query_lists_every_rule_with_its_source() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes) = start(&socket);

    send(
        &core,
        &socket,
        &wakes,
        add_rule(RuleIdentity::Domain("figma.com".parse().unwrap())),
    );

    assert_eq!(
        rules_of(&core, &socket, &wakes),
        vec![
            app_rule("com.figma.Desktop"),
            ad_hoc(RuleIdentity::Domain("figma.com".parse().unwrap())),
        ]
    );
}

#[test]
fn a_browser_whose_place_says_off_is_watched_while_a_site_in_it_could_say_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n",
    );
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    grant_accessibility(&core);
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    let safari_app = Place::App("com.apple.Safari".parse().unwrap());

    assert_eq!(
        activate_browser(&core, "com.apple.Safari"),
        vec![Effect::SetNightShift { enabled: false }]
    );
    let (effects, _) = send(
        &core,
        &socket,
        &wakes,
        set(safari_app.clone(), OnOff::On, Length::UntilScheduleTurns),
    );
    assert!(effects.contains(&watch("com.apple.Safari", 1)));
    let (effects, _) = send(&core, &socket, &wakes, Command::Reset(safari_app));
    assert!(effects.contains(&Effect::StopWatching));
    let (effects, _) = send(
        &core,
        &socket,
        &wakes,
        Command::AddRule {
            identity: RuleIdentity::Domain("youtube.com".parse().unwrap()),
            says: OnOff::On,
        },
    );
    assert!(effects.contains(&watch("com.apple.Safari", 2)));
}

#[test]
fn snoozing_a_domain_rule_lifts_it_and_keeps_watching() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_figma(&socket);
    let figma_site = Place::Domain("figma.com".parse().unwrap());

    let (effects, _) = send(
        &core,
        &socket,
        &wakes,
        set(figma_site.clone(), OnOff::On, Length::UntilScheduleTurns),
    );

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(
        status.layers,
        vec![layered(
            figma_site,
            Some(layer(OnOff::On, Some("2026-10-17T07:00:00+02:00"))),
            None
        )]
    );
    assert!(status.watching);
}

#[test]
fn a_config_rule_cant_be_replaced_or_removed() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);

    for command in [
        Command::RemoveRule(figma()),
        Command::AddRule {
            identity: figma(),
            says: OnOff::On,
        },
    ] {
        let (effects, response) = send(&core, &socket, &wakes, command.clone());
        assert_eq!(effects, Vec::new(), "{command:?}");
        let Response::Error(error) = response else {
            panic!("{command:?} was accepted: {response:?}");
        };
        assert_eq!(error.code, ErrorCode::ConfigRule, "{command:?}");
        assert!(
            error.message.contains("change it there"),
            "{}",
            error.message
        );
    }
    assert_eq!(
        rules_of(&core, &socket, &wakes),
        vec![app_rule("com.figma.Desktop")]
    );
}

fn mail() -> RuleIdentity {
    RuleIdentity::App("com.apple.mail".parse().unwrap())
}

#[test]
fn always_makes_the_users_rule_say_that_way_replacing_one_that_said_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.apple.mail");
    send(&core, &socket, &wakes, add_rule(mail()));
    send(
        &core,
        &socket,
        &wakes,
        set(Place::from(mail()), OnOff::On, minutes(30)),
    );

    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule {
            identity: mail(),
            says: OnOff::On,
        },
    );

    let status = status_of(&core, &socket, &wakes);
    assert_eq!(
        status.rules,
        vec![Rule {
            identity: mail(),
            source: RuleSource::AdHoc,
            says: OnOff::On,
        }]
    );
    assert_eq!(status.layers, Vec::new());
}

/// Starts at noon, off by the custom hours, with Books in front.
fn start_in_books_at_noon(socket: &Path) -> (Core, mpsc::Receiver<()>, Arc<TestClock>) {
    let (core, wakes, clock) = start_at(socket, "2026-10-16T12:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.apple.iBooksX");
    (core, wakes, clock)
}

fn books() -> RuleIdentity {
    RuleIdentity::App("com.apple.iBooksX".parse().unwrap())
}

#[test]
fn an_app_rule_that_says_on_turns_night_shift_on_by_day() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_in_books_at_noon(&socket);

    let (effects, _) = send(
        &core,
        &socket,
        &wakes,
        Command::AddRule {
            identity: books(),
            says: OnOff::On,
        },
    );
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(
        activate(&core, "com.apple.mail"),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn a_rule_that_says_on_is_released_on_a_clean_exit() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_in_books_at_noon(&socket);
    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule {
            identity: books(),
            says: OnOff::On,
        },
    );
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        core.handle(Event::Terminating),
        vec![Effect::SetNightShift { enabled: false }]
    );
}

#[test]
fn an_ad_hoc_rule_graduates_whatever_either_says() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule {
            identity: figma(),
            says: OnOff::On,
        },
    );

    write_config(&socket, "[rules.off]\napp = [\"com.figma.desktop\"]\n");
    exchange(&core, &socket, &wakes, Request::Reload);

    assert_eq!(
        rules_of(&core, &socket, &wakes),
        vec![app_rule("com.figma.desktop")]
    );
    let store = adhoc::Store::new(socket.parent().unwrap().with_file_name("rules.toml"));
    assert_eq!(store.load(), Ok(Vec::new()));
}

#[test]
fn export_writes_both_lists() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    send(&core, &socket, &wakes, add_rule(figma()));
    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule {
            identity: RuleIdentity::Domain("youtube.com".parse().unwrap()),
            says: OnOff::On,
        },
    );

    assert_eq!(
        core.export(ExportFormat::Nix),
        "{ off = { app = [ \"com.figma.Desktop\" ]; domain = [ ]; }; on = { app = [ ]; domain = [ \"youtube.com\" ]; }; }\n"
    );
    assert_eq!(
        core.export(ExportFormat::Toml),
        "[rules.off]\napp = [\"com.figma.Desktop\"]\n\n[rules.on]\ndomain = [\"youtube.com\"]\n"
    );
}

#[test]
fn rules_are_listed_by_what_they_say() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.apple.iBooksX\"]\ndomain = [\"youtube.com\"]\n",
    );
    let (core, wakes) = start(&socket);

    let on = |identity| Rule {
        identity,
        source: RuleSource::Config,
        says: OnOff::On,
    };
    assert_eq!(
        status_of(&core, &socket, &wakes).rules,
        vec![
            app_rule("com.figma.Desktop"),
            on(books()),
            on(RuleIdentity::Domain("youtube.com".parse().unwrap())),
        ]
    );
}

#[test]
fn a_config_that_cant_be_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);

    for (config, said) in [
        (
            "[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.figma.desktop\"]\n",
            "both rules.off and rules.on",
        ),
        (
            "[rules]\napp = [\"com.figma.Desktop\"]\n",
            "under [rules.off] and [rules.on]",
        ),
        ("[menu]\nlengths = [\"45m\", \"forever\"]\n", "menu.lengths"),
        ("[menu]\nlengths = [\"30s\"]\n", "menu.lengths"),
        ("[menu]\nlengths = []\n", "menu.lengths"),
        (
            "[rules.off]\napp = [\"com.figma.Desktop \"]\n",
            "\"com.figma.Desktop \" isn't a bundle ID",
        ),
        ("[rules.on]\napp = [\"\"]\n", "\"\" isn't a bundle ID"),
        (
            "[rules.off]\ndomain = [\"https://figma.com\"]\n",
            "\"https://figma.com\" isn't a domain",
        ),
    ] {
        write_config(&socket, config);
        let (_, response) = exchange(&core, &socket, &wakes, Request::Reload);
        let Response::Error(error) = response else {
            panic!("{config:?} was accepted: {response:?}");
        };
        assert_eq!(error.code, ErrorCode::InvalidConfig, "{config:?}");
        assert!(
            error.message.contains(said),
            "{config:?}: {}",
            error.message
        );
    }
}

/// The menu's Night Shift item: checked while it's on everywhere, with what decides the
/// screen.
fn night_shift_item(on: bool, reason: &str, command: Command) -> NightShiftItem {
    NightShiftItem {
        on,
        reason: Some(reason.to_owned()),
        command: Some(command),
    }
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
        core.menu().night_shift,
        night_shift_item(true, "Off in Figma: your config’s rule", Command::Off)
    );
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, minutes(30)),
    );
    assert_eq!(
        core.menu().night_shift,
        night_shift_item(false, "Off in Figma: your config’s rule", Command::On)
    );
    activate(&core, "com.apple.Terminal");
    assert_eq!(core.menu().icon, MenuIcon::Off);
    assert_eq!(
        core.menu().night_shift,
        night_shift_item(false, "Off until 23:30", Command::On)
    );
    send(&core, &socket, &wakes, Command::On);
    assert_eq!(
        core.menu().night_shift,
        night_shift_item(true, "Following the schedule until 07:00", Command::Off)
    );
    send(&core, &socket, &wakes, Command::Off);
    assert_eq!(
        core.menu().night_shift,
        night_shift_item(false, "Off until you turn it on", Command::On)
    );
}

#[test]
fn while_night_shift_is_unavailable_the_menu_only_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start(&socket_in(&dir));

    let menu = core.menu();

    assert_eq!(menu.icon, MenuIcon::Unavailable);
    assert_eq!(
        menu.night_shift,
        NightShiftItem {
            on: false,
            reason: Some("Unavailable".to_owned()),
            command: None,
        }
    );
    assert_eq!(menu.entries, Vec::new());
}

/// The menu's place rows, top down.
fn rows(core: &Core) -> Vec<PlaceRow> {
    core.menu()
        .entries
        .into_iter()
        .filter_map(|entry| match entry {
            MenuEntry::Row { row } => Some(row),
            _ => None,
        })
        .collect()
}

fn everywhere_row(core: &Core) -> Option<PlaceRow> {
    rows(core)
        .into_iter()
        .find(|row| row.image == RowImage::Everywhere)
}

fn app_row(core: &Core) -> Option<PlaceRow> {
    rows(core)
        .into_iter()
        .find(|row| matches!(row.image, RowImage::App { .. }))
}

fn site_row(core: &Core) -> Option<PlaceRow> {
    rows(core)
        .into_iter()
        .find(|row| row.image == RowImage::Site)
}

/// A site row's title, and the site it names under it.
fn site_title(row: &PlaceRow) -> (&str, Option<&str>) {
    (&row.title, row.site.as_deref())
}

/// A row's items as they read: their titles, "Custom…" for picking a time, and "—" between
/// groups.
fn titles(row: &PlaceRow) -> Vec<String> {
    row.items
        .iter()
        .map(|item| match item {
            RowItem::Choice { item } => item.title.clone(),
            RowItem::PickTime { .. } => "Custom…".to_owned(),
            RowItem::Separator => "—".to_owned(),
        })
        .collect()
}

/// The item titled `title` in a row, which must have one.
fn choice_titled(row: &PlaceRow, title: &str) -> MenuItem {
    row.items
        .iter()
        .find_map(|item| match item {
            RowItem::Choice { item } if item.title == title => Some(item.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {title:?} in {:?}", titles(row)))
}

fn choice(title: &str, command: Command) -> RowItem {
    RowItem::Choice {
        item: MenuItem {
            title: title.to_owned(),
            subtitle: None,
            enabled: true,
            command,
        },
    }
}

#[test]
fn the_menu_has_everywheres_row_then_the_apps_and_the_sites() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_on_figma(&socket_in(&dir));
    core.handle(Event::StrengthChanged { value: 60 });

    let entries = core.menu().entries;

    let kinds: Vec<_> = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Strength { .. } => "strength",
            MenuEntry::Separator => "—",
            MenuEntry::Row { row } => match row.image {
                RowImage::Everywhere => "everywhere",
                RowImage::App { .. } => "app",
                RowImage::Site => "site",
            },
        })
        .collect();
    assert_eq!(kinds, ["strength", "everywhere", "—", "app", "site"]);
    assert!(site_row(&core).unwrap().indented);
}

#[test]
fn everywheres_row_has_every_length_but_the_switchs() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    let menu = core.menu();

    assert_eq!(menu.icon, MenuIcon::On);
    assert_eq!(menu.night_shift.command, Some(Command::Off));
    let off = |length| set(Place::Everywhere, OnOff::Off, length);
    assert_eq!(
        everywhere_row(&core),
        Some(PlaceRow {
            title: "Turn Off".to_owned(),
            site: None,
            subtitle: None,
            image: RowImage::Everywhere,
            indented: false,
            items: vec![
                choice("For 30 Minutes", off(minutes(30))),
                choice("For 1 Hour", off(minutes(60))),
                choice("For 2 Hours", off(minutes(120))),
                RowItem::PickTime {
                    title: "Turn Off Everywhere".to_owned(),
                    place: Place::Everywhere,
                    state: OnOff::Off,
                    start: at("2026-10-17T00:00:00+02:00"),
                },
                RowItem::Choice {
                    item: MenuItem {
                        title: "Until 22:00".to_owned(),
                        subtitle: Some("When the schedule starts".to_owned()),
                        enabled: true,
                        command: off(Length::UntilScheduleTurns),
                    },
                },
            ],
        })
    );
}

/// The line under everywhere's "Until" item at `time`, with Night Shift `enabled` as the
/// schedule has it then.
fn until_note(time: &str, enabled: bool, schedule: Schedule, title: &str) -> Option<String> {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), time);
    report_zurich_sun(&core);
    core.handle(Event::StatusChanged {
        enabled,
        schedule,
        sun_schedule_set_aside: false,
    });
    choice_titled(&everywhere_row(&core).unwrap(), title).subtitle
}

#[test]
fn until_the_schedule_turns_names_sunrise_or_sunset() {
    assert_eq!(
        until_note(
            "2026-10-16T01:40:00+02:00",
            true,
            Schedule::SunsetToSunrise,
            "Until 18:31"
        )
        .as_deref(),
        Some("At sunset")
    );
    assert_eq!(
        until_note(
            "2026-10-16T12:00:00+02:00",
            false,
            Schedule::SunsetToSunrise,
            "Until 07:52"
        )
        .as_deref(),
        Some("At sunrise")
    );
}

#[test]
fn until_the_schedule_turns_names_custom_hours_start_or_end() {
    assert_eq!(
        until_note(
            "2026-10-16T23:00:00+02:00",
            true,
            custom_hours(),
            "Until 22:00"
        )
        .as_deref(),
        Some("When the schedule starts")
    );
    assert_eq!(
        until_note(
            "2026-10-16T12:00:00+02:00",
            false,
            custom_hours(),
            "Until 07:00"
        )
        .as_deref(),
        Some("When the schedule ends")
    );
}

#[test]
fn the_menus_lengths_come_from_the_config() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[menu]\nlengths = [\"15m\", \"1h30m\"]\n");
    let (core, _wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });

    assert_eq!(
        titles(&everywhere_row(&core).unwrap()),
        [
            "For 15 Minutes",
            "For 1 Hour 30 Minutes",
            "Custom…",
            "Until 22:00"
        ]
    );
}

#[test]
fn everywheres_row_names_the_place_that_keeps_the_other_way() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.apple.iBooksX\"]\n",
    );
    let (core, _wakes, clock) = start_at(&socket, "2026-10-16T12:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    let front = |bundle_id: &str, name: &str| {
        core.handle(Event::AppActivated {
            bundle_id: Some(bundle_id.to_owned()),
            name: Some(name.to_owned()),
            is_browser: false,
        });
        let row = everywhere_row(&core).unwrap();
        (row.title, row.subtitle)
    };
    let row = |title: &str, subtitle: Option<&str>| (title.to_owned(), subtitle.map(str::to_owned));

    // By day, turning Night Shift on everywhere leaves Figma off, and Books on as it is.
    assert_eq!(
        front("com.figma.Desktop", "Figma"),
        row("Turn On", Some("Everywhere but in Figma"))
    );
    assert_eq!(front("com.apple.iBooksX", "Books"), row("Turn On", None));
    // At night, turning it off everywhere leaves Books on, and Figma off as it is.
    clock.set("2026-10-16T23:00:00+02:00");
    core.handle(Event::Woke);
    assert_eq!(
        front("com.apple.iBooksX", "Books"),
        row("Turn Off", Some("Everywhere but in Books"))
    );
    assert_eq!(front("com.figma.Desktop", "Figma"), row("Turn Off", None));
}

#[test]
fn by_day_the_rows_turn_on_and_by_night_off() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, clock) = start_at(&socket_in(&dir), "2026-10-16T12:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    core.handle(Event::AppActivated {
        bundle_id: Some("com.apple.mail".to_owned()),
        name: Some("Mail".to_owned()),
        is_browser: false,
    });
    let titles_of =
        |core: &Core| -> Vec<String> { rows(core).into_iter().map(|row| row.title).collect() };

    assert_eq!(titles_of(&core), ["Turn On", "Turn On in Mail"]);
    clock.set("2026-10-16T23:00:00+02:00");
    core.handle(Event::Woke);
    assert_eq!(titles_of(&core), ["Turn Off", "Turn Off in Mail"]);
}

#[test]
fn without_the_sun_times_an_apps_row_turns_night_shift_the_other_way_from_how_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T23:00:00+02:00");
    core.handle(Event::AppActivated {
        bundle_id: Some("com.googlecode.iterm2".to_owned()),
        name: Some("iTerm2".to_owned()),
        is_browser: false,
    });

    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::SunsetToSunrise,
        sun_schedule_set_aside: false,
    });
    assert_eq!(app_row(&core).unwrap().title, "Turn Off in iTerm2");
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::SunsetToSunrise,
        sun_schedule_set_aside: false,
    });
    assert_eq!(app_row(&core).unwrap().title, "Turn On in iTerm2");
}

#[test]
fn without_the_sun_times_the_menu_follows_the_schedule_as_macos_applies_it() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T23:00:00+02:00");

    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: Schedule::SunsetToSunrise,
        sun_schedule_set_aside: false,
    });

    let menu = core.menu();
    assert!(menu.night_shift.on);
    assert_eq!(
        menu.night_shift.reason.as_deref(),
        Some("Following the schedule")
    );
    let row = everywhere_row(&core).unwrap();
    assert_eq!(row.title, "Turn Off");
    assert_eq!(
        titles(&row),
        ["For 30 Minutes", "For 1 Hour", "For 2 Hours", "Custom…"]
    );
    assert!(
        core.handle(Event::CommandChosen {
            command: choice_titled(&row, "For 30 Minutes").command,
        })
        .contains(&Effect::SetNightShift { enabled: false })
    );
}

#[test]
fn undoing_is_offered_where_it_goes_the_right_way_and_a_rule_of_yours_can_always_go() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, clock) = start_forced(&socket);
    send(
        &core,
        &socket,
        &wakes,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );
    assert_eq!(
        app_row(&core).unwrap().items[0],
        choice("Put the Rule Back", Command::Reset(figma_app()))
    );

    activate(&core, "com.apple.mail");
    send(
        &core,
        &socket,
        &wakes,
        set(Place::from(mail()), OnOff::Off, minutes(30)),
    );
    assert_eq!(
        app_row(&core).unwrap().items[0],
        choice("Like Everywhere Else", Command::Reset(Place::from(mail())))
    );

    send(&core, &socket, &wakes, Command::Reset(Place::from(mail())));
    send(&core, &socket, &wakes, add_rule(mail()));
    assert_eq!(
        choice_titled(&app_row(&core).unwrap(), "Remove Your Rule").command,
        Command::RemoveRule(mail())
    );
    // By day removing it changes nothing on screen, but the rule is still the user's to remove.
    clock.set("2026-10-17T12:00:00+02:00");
    core.handle(Event::Woke);
    assert_eq!(
        choice_titled(&app_row(&core).unwrap(), "Remove Your Rule").command,
        Command::RemoveRule(mail())
    );
}

#[test]
fn lengths_that_come_to_the_same_as_undoing_are_left_out() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_forced(&socket);
    send(
        &core,
        &socket,
        &wakes,
        set(figma_app(), OnOff::On, Length::UntilScheduleTurns),
    );

    assert_eq!(
        titles(&app_row(&core).unwrap()),
        [
            "Put the Rule Back",
            "—",
            "For 30 Minutes",
            "For 1 Hour",
            "For 2 Hours",
            "Custom…"
        ]
    );
    activate(&core, "com.apple.mail");
    send(
        &core,
        &socket,
        &wakes,
        set(Place::from(mail()), OnOff::Off, minutes(30)),
    );
    assert_eq!(
        titles(&app_row(&core).unwrap()),
        ["Like Everywhere Else", "—", "Always"]
    );
}

#[test]
fn a_config_rules_always_is_greyed() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_forced(&socket_in(&dir));

    let row = app_row(&core).unwrap();

    assert_eq!(row.title, "Turn On in com.figma.Desktop");
    assert_eq!(
        row.subtitle.as_deref(),
        Some("Always off: your config’s rule")
    );
    let always = choice_titled(&row, "Always");
    assert!(!always.enabled);
    assert_eq!(always.subtitle.as_deref(), Some("Your config decides"));
}

#[test]
fn custom_is_offered_beside_the_lengths_with_its_place_and_way() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, clock) = start_forced(&socket_in(&dir));
    clock.set("2026-10-16T23:10:00+02:00");
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    let row = app_row(&core).unwrap();

    assert!(row.items.contains(&RowItem::PickTime {
        title: "Turn On in Figma".to_owned(),
        place: figma_app(),
        state: OnOff::On,
        start: at("2026-10-17T00:15:00+02:00"),
    }));
}

#[test]
fn the_site_row_is_the_tab_domain_or_the_most_specific_covering_rules() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "websites = true\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate_browser(&core, "com.apple.Safari");
    report_host(&core, 1, "docs.figma.com");
    let row = site_row(&core).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn Off at This Site", Some("docs.figma.com"))
    );
    assert_eq!(
        choice_titled(&row, "Always").command,
        add_rule(domain("docs.figma.com"))
    );
    report_host(&core, 1, "www.figma.com");
    assert_eq!(
        site_title(&site_row(&core).unwrap()),
        ("Turn Off at This Site", Some("figma.com"))
    );

    report_host(&core, 1, "docs.figma.com");
    send(&core, &socket, &wakes, add_rule(domain("figma.com")));
    assert_eq!(
        site_title(&site_row(&core).unwrap()),
        ("Turn On at This Site", Some("figma.com"))
    );
    send(&core, &socket, &wakes, add_rule(domain("docs.figma.com")));
    assert_eq!(
        site_title(&site_row(&core).unwrap()),
        ("Turn On at This Site", Some("docs.figma.com"))
    );
}

#[test]
fn a_site_goes_back_to_its_browsers_say_by_the_browsers_name() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n",
    );
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    core.handle(Event::AppActivated {
        bundle_id: Some("com.apple.Safari".to_owned()),
        name: Some("Safari".to_owned()),
        is_browser: true,
    });
    report_host(&core, 1, "example.com");
    let example = Place::Domain("example.com".parse().unwrap());

    send(
        &core,
        &socket,
        &wakes,
        set(example.clone(), OnOff::On, minutes(30)),
    );

    let row = site_row(&core).unwrap();
    assert_eq!(
        choice_titled(&row, "Like the Rest of Safari").command,
        Command::Reset(example)
    );
}

#[test]
fn the_site_row_snoozes_a_config_rule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_figma(&socket);
    let figma_site = Place::Domain("figma.com".parse().unwrap());

    let row = site_row(&core).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn On at This Site", Some("figma.com"))
    );
    assert_eq!(
        row.subtitle.as_deref(),
        Some("Always off: your config’s rule")
    );
    let until_morning = choice_titled(&row, "Until 07:00").command;
    assert_eq!(
        until_morning,
        set(figma_site.clone(), OnOff::On, Length::UntilScheduleTurns)
    );
    send(&core, &socket, &wakes, until_morning);
    let row = site_row(&core).unwrap();
    assert_eq!(
        site_title(&row),
        ("Turn Off at This Site", Some("figma.com"))
    );
    assert_eq!(
        row.subtitle.as_deref(),
        Some("On until 07:00, then your config’s rule")
    );
    assert_eq!(
        row.items[0],
        choice("Put the Rule Back", Command::Reset(figma_site))
    );
}

#[test]
fn the_menu_opens_with_a_strength_slider_once_the_strength_is_known() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start(&socket_in(&dir));
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: Schedule::Off,
        sun_schedule_set_aside: false,
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
fn the_menu_adds_and_removes_an_ad_hoc_rule_for_the_frontmost_app() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    core.handle(Event::AppActivated {
        bundle_id: Some("com.figma.Desktop".to_owned()),
        name: Some("Figma".to_owned()),
        is_browser: false,
    });

    let row = app_row(&core).unwrap();
    assert_eq!(row.title, "Turn Off in Figma");
    assert_eq!(row.subtitle, None);
    assert_eq!(choice_titled(&row, "Always").command, add_rule(figma()));
    send(&core, &socket, &wakes, add_rule(figma()));
    let row = app_row(&core).unwrap();
    assert_eq!(row.title, "Turn On in Figma");
    assert_eq!(row.subtitle.as_deref(), Some("Always off: your rule"));
    assert_eq!(
        choice_titled(&row, "Remove Your Rule").command,
        Command::RemoveRule(figma())
    );
}

#[test]
fn choosing_a_menu_item_runs_its_command() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    activate(&core, "com.figma.Desktop");

    let effects = core.handle(Event::CommandChosen {
        command: add_rule(figma()),
    });

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
}

#[test]
fn website_checking_is_off_until_turned_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\ndomain = [\"figma.com\"]\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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
        sun_schedule_set_aside: false,
    });
    let switch = |on, locked, command| SettingsSwitch {
        on,
        locked,
        command,
    };

    assert!(!rows(&core).iter().any(|row| row.title == "Check Websites"));
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
        sun_schedule_set_aside: false,
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
    RuleIdentity::Domain(name.parse().unwrap())
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

    let row = site_row(&core).unwrap();
    assert_eq!(site_title(&row), ("Turn Off at This Site", Some("www.com")));
    assert_eq!(
        choice_titled(&row, "Always").command,
        add_rule(domain("www.com"))
    );
}

fn safari() -> RuleIdentity {
    RuleIdentity::App("com.apple.Safari".parse().unwrap())
}

#[test]
fn a_browser_its_own_app_rule_forces_off_is_not_watched() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_figma(&socket);

    let (effects, _) = send(&core, &socket, &wakes, add_rule(safari()));
    assert!(effects.contains(&Effect::StopWatching));
    assert_eq!(site_row(&core), None);
    assert_eq!(status_of(&core, &socket, &wakes).host, None);
    let (effects, _) = send(&core, &socket, &wakes, Command::RemoveRule(safari()));
    assert!(effects.contains(&watch("com.apple.Safari", 2)));
}

fn accessibility_notice() -> Effect {
    Effect::Notify {
        title: "Website checking needs Accessibility".to_owned(),
        body: "Open Nightcap’s settings to allow it, or to stop checking websites.".to_owned(),
        opens: None,
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
    write_config(&socket, "[rules.off]\ndomian = [\"figma.com\"]\n");
    let (core, _wakes) = start(&socket);

    assert!(launch(&core).iter().any(|effect| matches!(
        effect,
        Effect::Notify { title, body, .. }
            if title == "Nightcap’s config is invalid" && body.contains("domian")
    )));
}

fn update_notice() -> Effect {
    let version = env!("CARGO_PKG_VERSION");
    Effect::Notify {
        title: format!("Nightcap updated to {version}"),
        body: "Click to see what’s new.".to_owned(),
        opens: Some(format!("https://nightcap.choco.me/changelog/#v{version}")),
    }
}

/// Leaves the state an earlier run saved after onboarding, by a version that kept its own, or
/// by one from before versions were kept.
fn saved_by(socket: &Path, last_version: Option<&str>) {
    Store::new(socket.with_file_name("state.json"))
        .save(&Persisted {
            onboarding: 1,
            last_version: last_version.map(str::to_owned),
            ..Persisted::default()
        })
        .unwrap();
}

#[test]
fn the_first_launch_of_a_new_version_says_whats_new_once() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    saved_by(&socket, Some("1.0.0"));
    let (core, _wakes) = start(&socket);

    assert!(launch(&core).contains(&update_notice()));
    let (core, _socket, _wakes, _clock) = restart_at(&socket, "2026-10-16T12:10:00+02:00");
    assert!(!launch(&core).contains(&update_notice()));
}

#[test]
fn a_run_from_before_versions_were_kept_counts_as_an_update() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    saved_by(&socket, None);
    let (core, _wakes) = start(&socket);

    assert!(launch(&core).contains(&update_notice()));
}

#[test]
fn a_first_run_has_nothing_new_to_tell() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes) = start(&socket_in(&dir));

    assert!(!notifies(&launch(&core)));
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
                state: None,
                place: Place::Everywhere,
                reasons: Vec::new(),
            },
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
            layers: Vec::new(),
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
        Client::connect(&socket).hello(nightcap_protocol::PROTOCOL_VERSION),
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
    client.hello(nightcap_protocol::PROTOCOL_VERSION);
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
    let rules = vec![
        (figma(), OnOff::Off),
        (
            RuleIdentity::Domain("youtube.com".parse().unwrap()),
            OnOff::On,
        ),
    ];

    store.save(&rules).unwrap();

    assert_eq!(store.load(), Ok(rules));
}

#[test]
fn ad_hoc_rules_from_a_newer_build_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.toml");
    std::fs::write(
        &path,
        "version = 3\n[rules.off]\napp = [\"com.figma.Desktop\"]\n",
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

    assert_eq!(
        adhoc::Store::new(path).load(),
        Ok(vec![(figma(), OnOff::Off)])
    );
}

#[test]
fn saved_state_loads_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("nightcap").join("state.json"));
    let state = Persisted {
        layers: vec![
            layered(
                figma_app(),
                Some(Layer {
                    state: OnOff::On,
                    until: Some(at("2026-10-17T07:00:00+02:00")),
                    with: Some(Place::Everywhere),
                }),
                None,
            ),
            layered(
                Place::Everywhere,
                Some(Layer {
                    state: OnOff::On,
                    until: Some(at("2026-10-17T07:00:00+02:00")),
                    with: Some(figma_app()),
                }),
                Some(layer(OnOff::Off, Some("2026-10-17T00:00:00+02:00"))),
            ),
        ],
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
        schedule_off_accepted: true,
        last_version: Some("1.0.0".to_owned()),
    };

    store.save(&state).unwrap();

    assert_eq!(store.load(), state);
}

#[test]
fn state_from_an_earlier_build_loads_without_its_manual_state_snoozes_or_timed_disable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(
        &path,
        r#"{"version":1,"manual_state":"off","snoozes":["app:com.figma.Desktop"],"timed_disable":"2026-10-16T22:00:00Z","websites":true,"onboarding":1}"#,
    )
    .unwrap();

    let state = Store::new(path).load();

    assert_eq!(state.layers, Vec::new());
    assert!(state.websites);
    assert_eq!(state.onboarding, 1);
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
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    exchange(&core, &socket, &wakes, Request::Reload);
    assert_eq!(config(&core), stands(CheckState::Ok, "Valid, with 1 rule."));
    write_config(&socket, "[rules.off]\ndomian = [\"figma.com\"]\n");
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
    send(&core, &socket, &wakes, add_rule(figma()));
    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::AdHocRules),
        stands(CheckState::Ok, "1 ad-hoc rule.")
    );

    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let rules_file = socket.parent().unwrap().with_file_name("rules.toml");
    std::fs::write(&rules_file, "version = 3\n").unwrap();
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
        sun_schedule_set_aside: false,
    });
    assert_eq!(
        check_of(&core, &socket, &wakes, CheckKind::NightShift),
        stands(CheckState::Ok, "Available, with no schedule.")
    );
    core.handle(Event::StatusChanged {
        enabled: false,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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

fn status(enabled: bool, schedule: Schedule, sun_schedule_set_aside: bool) -> Event {
    Event::StatusChanged {
        enabled,
        schedule,
        sun_schedule_set_aside,
    }
}

fn night_shift_check(state: CheckState, text: &str) -> Option<SettingsCheck> {
    Some(SettingsCheck {
        kind: CheckKind::NightShift,
        state,
        text: text.to_owned(),
        restore: None,
        accept: None,
    })
}

fn set_aside() -> Option<SettingsCheck> {
    Some(SettingsCheck {
        restore: button("Open Location Services…", Fix::OpenLocationServices),
        accept: button("Keep the Schedule Off", Fix::KeepScheduleOff),
        ..night_shift_check(
            CheckState::Drifted,
            "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on.",
        )
        .unwrap()
    })
}

#[test]
fn sunset_to_sunrise_set_aside_is_a_problem_until_location_services_is_back_on() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T01:40:00+02:00");
    report_zurich_sun(&core);
    core.handle(status(true, Schedule::SunsetToSunrise, false));

    let effects = core.handle(status(false, Schedule::Off, true));
    assert!(effects.contains(&Effect::Notify {
        title: "Sunset to sunrise needs Location Services".to_owned(),
        body: "macOS turned Night Shift’s schedule off. Open Nightcap’s settings to turn Location Services back on, or to keep the schedule off.".to_owned(),
        opens: None,
    }));
    assert_eq!(settings_check(&core, CheckKind::NightShift), set_aside());
    assert_eq!(
        core.menu().problem_line,
        Some(ProblemLine {
            title: "1 Thing Needs Attention…".to_owned(),
            subtitle: Some("Night Shift".to_owned()),
        })
    );

    core.handle(status(true, Schedule::SunsetToSunrise, false));
    assert_eq!(
        settings_check(&core, CheckKind::NightShift),
        night_shift_check(
            CheckState::Ok,
            "Available, following its schedule: sunset to sunrise."
        )
    );
}

#[test]
fn nightcap_opening_with_sunset_to_sunrise_set_aside_shows_the_problem() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T01:40:00+02:00");

    core.handle(status(false, Schedule::Off, true));

    assert_eq!(settings_check(&core, CheckKind::NightShift), set_aside());
}

#[test]
fn the_schedule_set_aside_can_be_kept_off_until_location_services_is_back_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes, _clock) = start_at(&socket, "2026-10-16T01:40:00+02:00");
    core.handle(status(false, Schedule::Off, true));

    core.handle(Event::FixChosen {
        fix: Fix::KeepScheduleOff,
    });

    let no_schedule = night_shift_check(CheckState::Ok, "Available, with no schedule.");
    assert_eq!(settings_check(&core, CheckKind::NightShift), no_schedule);
    let (core, _socket, _wakes, _clock) = restart_at(&socket, "2026-10-16T01:45:00+02:00");
    assert!(!notifies(&core.handle(status(false, Schedule::Off, true))));
    assert_eq!(settings_check(&core, CheckKind::NightShift), no_schedule);

    core.handle(status(true, Schedule::SunsetToSunrise, false));
    assert!(notifies(&core.handle(status(false, Schedule::Off, true))));
    assert_eq!(settings_check(&core, CheckKind::NightShift), set_aside());
}

#[test]
fn a_schedule_chosen_while_sunset_to_sunrise_is_set_aside_is_no_problem() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, _clock) = start_at(&socket_in(&dir), "2026-10-16T01:40:00+02:00");
    core.handle(status(false, Schedule::Off, true));

    core.handle(status(true, custom_hours(), true));

    assert_eq!(
        settings_check(&core, CheckKind::NightShift),
        night_shift_check(
            CheckState::Ok,
            "Available, following its schedule: 22:00 to 07:00."
        )
    );
}

#[test]
fn sunset_to_sunrise_without_sun_times_for_ten_minutes_is_a_problem() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _wakes, clock) = start_at(&socket_in(&dir), "2026-10-16T01:40:00+02:00");
    core.handle(status(true, Schedule::SunsetToSunrise, false));
    let following = night_shift_check(
        CheckState::Ok,
        "Available, following its schedule: sunset to sunrise.",
    );

    clock.set("2026-10-16T01:49:00+02:00");
    assert!(!notifies(&core.handle(Event::TimerFired)));
    assert_eq!(settings_check(&core, CheckKind::NightShift), following);

    clock.set("2026-10-16T01:50:00+02:00");
    assert!(core.handle(Event::TimerFired).contains(&Effect::Notify {
        title: "Nightcap can’t follow sunset to sunrise".to_owned(),
        body: "macOS hasn’t worked out sunrise and sunset. Custom hours in Night Shift’s settings work without them.".to_owned(),
        opens: None,
    }));
    assert_eq!(
        settings_check(&core, CheckKind::NightShift),
        night_shift_check(
            CheckState::Broken,
            "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. Custom hours work without them.",
        )
    );

    clock.set("2026-10-16T01:51:00+02:00");
    report_zurich_sun(&core);
    core.handle(Event::TimerFired);
    assert_eq!(settings_check(&core, CheckKind::NightShift), following);
}

#[test]
fn settings_offer_to_reload_a_broken_config() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\ndomian = [\"figma.com\"]\n");
    let (core, _wakes) = start(&socket);

    let broken = settings_check(&core, CheckKind::ConfigFile).unwrap();
    assert_eq!(broken.state, CheckState::Broken);
    assert_eq!(broken.restore, button("Reload", Fix::Reload));
    write_config(&socket, "[rules.off]\ndomain = [\"figma.com\"]\n");
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
                opens: None,
            })
    );
}

#[test]
fn the_rules_tab_lists_every_rule_with_what_can_be_done_about_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(
        &socket,
        "[rules.off]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n",
    );
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    let photos = RuleIdentity::App("com.apple.Photos".parse().unwrap());
    send(&core, &socket, &wakes, add_rule(photos.clone()));
    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule {
            identity: domain("youtube.com"),
            says: OnOff::On,
        },
    );
    let figma_site = Place::Domain("figma.com".parse().unwrap());
    send(
        &core,
        &socket,
        &wakes,
        set(figma_site.clone(), OnOff::On, Length::UntilScheduleTurns),
    );
    let action = |title: &str, command| {
        Some(RuleAction {
            title: title.to_owned(),
            command,
        })
    };
    let rule = |identity: RuleIdentity, says, label: &str, snoozed, action| SettingsRule {
        identity,
        says,
        label: label.to_owned(),
        snoozed,
        action,
    };

    assert_eq!(
        core.settings().rules,
        vec![
            rule(figma(), OnOff::Off, "From your config", false, None),
            rule(
                domain("figma.com"),
                OnOff::Off,
                "Snoozed until 07:00",
                true,
                action("Put Back", Command::Reset(figma_site))
            ),
            rule(
                photos.clone(),
                OnOff::Off,
                "Ad-hoc",
                false,
                action("Remove", Command::RemoveRule(photos))
            ),
            rule(
                domain("youtube.com"),
                OnOff::On,
                "Ad-hoc",
                false,
                action("Remove", Command::RemoveRule(domain("youtube.com")))
            ),
        ]
    );
}

#[test]
fn the_rules_tab_lists_what_holds_for_now_where_no_rule_does() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    core.handle(Event::StatusChanged {
        enabled: true,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
    });
    let mail_app = Place::from(mail());
    send(
        &core,
        &socket,
        &wakes,
        set(mail_app.clone(), OnOff::Off, Length::UntilScheduleTurns),
    );
    send(
        &core,
        &socket,
        &wakes,
        set(mail_app.clone(), OnOff::On, minutes(30)),
    );
    // A snooze shows on its rule, and everywhere's own on the Night Shift item.
    send(
        &core,
        &socket,
        &wakes,
        set(figma_app(), OnOff::On, minutes(30)),
    );
    send(&core, &socket, &wakes, Command::Off);

    assert_eq!(
        core.settings().for_now,
        vec![SettingsLayers {
            place: mail_app.clone(),
            label: "Night Shift on until 23:30, then off until 22:00".to_owned(),
            action: RuleAction {
                title: "End".to_owned(),
                command: Command::Reset(mail_app),
            },
        }]
    );
}

#[test]
fn the_rules_tab_shows_where_the_config_is_and_copies_the_ad_hoc_rules_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes) = start(&socket);
    send(&core, &socket, &wakes, add_rule(figma()));
    send(&core, &socket, &wakes, add_rule(domain("figma.com")));

    assert_eq!(
        core.settings().config_path,
        config_beside(&socket).display().to_string()
    );
    assert_eq!(
        core.export(ExportFormat::Nix),
        "{ off = { app = [ \"com.figma.Desktop\" ]; domain = [ \"figma.com\" ]; }; on = { app = [ ]; domain = [ ]; }; }\n"
    );
    assert_eq!(
        core.export(ExportFormat::Toml),
        "[rules.off]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n"
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

// The tour (ADR-0016).

const SAMPLE: &str = "io.github.choco.nightcap.tour-sample";

fn sample_rule() -> Rule {
    Rule {
        identity: RuleIdentity::App(SAMPLE.parse().unwrap()),
        source: RuleSource::Tour,
        says: OnOff::Off,
    }
}

fn start_tour(core: &Core) -> Vec<Effect> {
    core.handle(Event::TourStarted)
}

fn report(core: &Core, enabled: bool) -> Vec<Effect> {
    core.handle(Event::StatusChanged {
        enabled,
        schedule: custom_hours(),
        sun_schedule_set_aside: false,
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

/// Night Shift on for the tour, over the custom hours' off until 22:00.
fn on_for_the_tour() -> Decision {
    Decision {
        state: Some(OnOff::On),
        place: Place::Everywhere,
        reasons: vec![
            Reason::Tour,
            Reason::Schedule {
                state: OnOff::Off,
                until: Some(at("2026-10-16T22:00:00+02:00")),
            },
        ],
    }
}

#[test]
fn a_tour_started_with_night_shift_off_by_the_schedule_turns_it_on_for_the_tour() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_off_by_the_schedule(&socket);

    assert_eq!(
        start_tour(&core),
        vec![Effect::SetNightShift { enabled: true }]
    );
    assert_eq!(report(&core, true), Vec::new());

    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.decision, on_for_the_tour());
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn the_tours_end_turns_night_shift_back_off_following_the_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_off_by_the_schedule(&socket);
    start_tour(&core);
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
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn the_tour_sets_aside_everywheres_hold_and_puts_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    send(&core, &socket, &wakes, Command::Off);
    report(&core, false);

    assert_eq!(
        start_tour(&core),
        vec![Effect::SetNightShift { enabled: true }]
    );
    report(&core, true);
    assert_eq!(
        core.handle(Event::TourEnded),
        vec![Effect::SetNightShift { enabled: false }]
    );

    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            None
        )]
    );
}

#[test]
fn the_tour_sets_aside_everywheres_timer_and_puts_it_back_with_its_end() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, minutes(60)),
    );
    report(&core, false);

    assert_eq!(
        start_tour(&core),
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
        status_of(&core, &socket, &wakes).decision.reasons[0],
        Reason::Timer(layer(OnOff::Off, Some("2026-10-17T00:00:00+02:00")))
    );
}

#[test]
fn while_the_tour_stands_in_everywheres_row_offers_no_way_back_to_the_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_off_by_the_schedule(&socket);
    start_tour(&core);
    report(&core, true);

    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::On, Length::UntilScheduleTurns),
    );
    send(
        &core,
        &socket,
        &wakes,
        set(Place::Everywhere, OnOff::Off, minutes(30)),
    );

    // With nothing else to offer, the row is left out.
    assert!(
        everywhere_row(&core)
            .is_none_or(|row| !titles(&row).contains(&"Back to the Schedule".to_owned()))
    );
}

#[test]
fn while_the_tour_stands_in_a_tap_off_and_on_returns_to_the_tours_on() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_off_by_the_schedule(&socket);
    start_tour(&core);
    report(&core, true);

    let (effects, _) = send(&core, &socket, &wakes, Command::Toggle);
    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    report(&core, false);
    let (effects, response) = send(&core, &socket, &wakes, Command::Toggle);

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert_eq!(response, Response::Ok(Payload::Decision(on_for_the_tour())));
    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn a_tour_started_with_night_shift_on_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);

    assert_eq!(start_tour(&core), Vec::new());
    assert!(matches!(
        status_of(&core, &socket, &wakes).decision.reasons[..],
        [Reason::Schedule { .. }]
    ));
    assert_eq!(core.handle(Event::TourEnded), Vec::new());
}

#[test]
fn adding_the_samples_rule_mid_tour_forces_night_shift_off_while_it_is_in_front() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    start_tour(&core);
    assert_eq!(activate(&core, SAMPLE), Vec::new());

    let (effects, response) = send(&core, &socket, &wakes, add_rule(sample_rule().identity));

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: false }]);
    assert_eq!(
        response,
        Response::Ok(Payload::Decision(Decision {
            state: Some(OnOff::Off),
            place: Place::App(SAMPLE.parse().unwrap()),
            reasons: vec![Reason::Rule(sample_rule())],
        }))
    );
    report(&core, false);
    assert_eq!(
        activate(&core, "com.apple.mail"),
        vec![Effect::SetNightShift { enabled: true }]
    );
}

#[test]
fn outside_a_tour_the_sample_matches_no_rule() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    start_tour(&core);
    send(&core, &socket, &wakes, add_rule(sample_rule().identity));
    activate(&core, "com.apple.mail");
    core.handle(Event::TourEnded);

    assert_eq!(activate(&core, SAMPLE), Vec::new());
}

#[test]
fn removing_the_samples_rule_mid_tour_drops_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    start_tour(&core);
    activate(&core, SAMPLE);
    send(&core, &socket, &wakes, add_rule(sample_rule().identity));
    report(&core, false);

    let (effects, response) = send(
        &core,
        &socket,
        &wakes,
        Command::RemoveRule(sample_rule().identity),
    );

    assert_eq!(effects, vec![Effect::SetNightShift { enabled: true }]);
    assert!(matches!(response, Response::Ok(_)), "{response:?}");
    assert_eq!(status_of(&core, &socket, &wakes).rules, Vec::new());
}

#[test]
fn the_menu_offers_to_add_the_samples_rule_and_then_to_remove_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    start_tour(&core);
    core.handle(Event::AppActivated {
        bundle_id: Some(SAMPLE.to_owned()),
        name: Some("Tour Sample".to_owned()),
        is_browser: false,
    });
    let row = app_row(&core).unwrap();
    assert_eq!(row.title, "Turn Off in Tour Sample");
    assert_eq!(
        choice_titled(&row, "Always").command,
        add_rule(sample_rule().identity)
    );

    send(&core, &socket, &wakes, add_rule(sample_rule().identity));

    let row = app_row(&core).unwrap();
    assert_eq!(row.title, "Turn On in Tour Sample");
    assert_eq!(row.subtitle.as_deref(), Some("Always off: the tour’s rule"));
    assert_eq!(
        choice_titled(&row, "Remove Your Rule").command,
        Command::RemoveRule(sample_rule().identity)
    );
}

#[test]
fn rules_added_mid_tour_stay_out_of_settings_and_files_and_go_with_their_snoozes() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    start_tour(&core);
    let site = RuleIdentity::Domain("choco.me".parse().unwrap());
    for identity in [sample_rule().identity, site.clone()] {
        let (_, response) = send(&core, &socket, &wakes, add_rule(identity));
        assert!(matches!(response, Response::Ok(_)), "{response:?}");
    }
    let (_, response) = send(
        &core,
        &socket,
        &wakes,
        set(
            Place::App(SAMPLE.parse().unwrap()),
            OnOff::On,
            Length::UntilScheduleTurns,
        ),
    );
    assert!(matches!(response, Response::Ok(_)), "{response:?}");

    assert_eq!(
        status_of(&core, &socket, &wakes).rules,
        vec![
            sample_rule(),
            Rule {
                identity: site,
                source: RuleSource::Tour,
                says: OnOff::Off,
            },
        ]
    );
    assert!(core.settings().rules.is_empty());
    assert!(!core.export(ExportFormat::Toml).contains(SAMPLE));
    let rules_file = socket.parent().unwrap().with_file_name("rules.toml");
    let kept = std::fs::read_to_string(rules_file).unwrap_or_default();
    assert!(
        !kept.contains(SAMPLE) && !kept.contains("choco.me"),
        "{kept}"
    );
    let saved = std::fs::read_to_string(socket.with_file_name("state.json")).unwrap();
    assert!(!saved.contains(SAMPLE), "{saved}");

    core.handle(Event::TourEnded);
    let status = status_of(&core, &socket, &wakes);
    assert_eq!(status.rules, Vec::new());
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn adding_a_rule_that_exists_mid_tour_leaves_it_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    report(&core, true);
    start_tour(&core);

    send(&core, &socket, &wakes, add_rule(figma()));

    assert_eq!(
        status_of(&core, &socket, &wakes).rules,
        vec![app_rule("com.figma.Desktop")]
    );
    core.handle(Event::TourEnded);
    assert_eq!(
        status_of(&core, &socket, &wakes).rules,
        vec![app_rule("com.figma.Desktop")]
    );
}

#[test]
fn removing_your_own_rule_mid_tour_lasts_only_while_it_runs() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    let mail = RuleIdentity::App("com.apple.mail".parse().unwrap());
    send(&core, &socket, &wakes, add_rule(mail.clone()));
    start_tour(&core);

    send(&core, &socket, &wakes, Command::RemoveRule(mail.clone()));
    assert_eq!(status_of(&core, &socket, &wakes).rules, Vec::new());
    let kept = adhoc::Store::new(socket.parent().unwrap().with_file_name("rules.toml"));
    assert_eq!(kept.load(), Ok(vec![(mail.clone(), OnOff::Off)]));
    core.handle(Event::TourEnded);

    assert_eq!(status_of(&core, &socket, &wakes).rules, vec![ad_hoc(mail)]);
}

#[test]
fn turning_your_own_rule_the_other_way_mid_tour_lasts_only_while_it_runs() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, wakes, _clock) = start_on_by_the_schedule(&socket);
    let mail = RuleIdentity::App("com.apple.mail".parse().unwrap());
    send(&core, &socket, &wakes, add_rule(mail.clone()));
    start_tour(&core);

    send(
        &core,
        &socket,
        &wakes,
        Command::AddRule {
            identity: mail.clone(),
            says: OnOff::On,
        },
    );
    assert_eq!(
        status_of(&core, &socket, &wakes).rules,
        vec![Rule {
            says: OnOff::On,
            ..ad_hoc(mail.clone())
        }]
    );
    assert!(core.export(ExportFormat::Toml).contains("[rules.off]"));
    core.handle(Event::TourEnded);

    assert_eq!(status_of(&core, &socket, &wakes).rules, vec![ad_hoc(mail)]);
}

#[test]
fn snoozes_and_the_switch_mid_tour_last_only_while_it_runs() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    write_config(&socket, "[rules.off]\napp = [\"com.figma.Desktop\"]\n");
    let (core, wakes, _clock) = start_at(&socket, "2026-10-16T23:00:00+02:00");
    report(&core, true);
    start_tour(&core);

    let figma = Place::App("com.figma.Desktop".parse().unwrap());
    send(&core, &socket, &wakes, set(figma, OnOff::On, minutes(30)));
    send(&core, &socket, &wakes, Command::Off);
    assert_eq!(status_of(&core, &socket, &wakes).layers.len(), 2);
    core.handle(Event::TourEnded);

    assert_eq!(status_of(&core, &socket, &wakes).layers, Vec::new());
}

#[test]
fn a_restart_after_a_crash_mid_tour_puts_night_shift_back_where_the_tour_found_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (crashed, wakes, _clock) = start_on_by_the_schedule(&socket);
    send(&crashed, &socket, &wakes, Command::Off);
    report(&crashed, false);
    start_tour(&crashed);
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
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            None
        )]
    );
}

#[test]
fn quitting_mid_tour_ends_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = socket_in(&dir);
    let (core, _wakes, _clock) = start_off_by_the_schedule(&socket);
    start_tour(&core);
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
    start_tour(&core);
    report(&core, true);

    assert_eq!(start_tour(&core), Vec::new());
    core.handle(Event::TourEnded);

    assert_eq!(
        status_of(&core, &socket, &wakes).layers,
        vec![layered(
            Place::Everywhere,
            Some(layer(OnOff::Off, None)),
            None
        )]
    );
}
