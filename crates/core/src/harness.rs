//! A staged core for the tests, with what it runs on in reach: its files, its clock, and the
//! request channel, which the tests fill as the IPC thread would.

use std::ops::Deref;
use std::sync::{Arc, mpsc};

use jiff::Timestamp;
use nightcap_protocol::{
    Command, Decision, ErrorReply, Install, Instance, Payload, Query, Request, Response, Rule,
    Status,
};

use crate::file::{File, Files, Memory};
use crate::ipc::Pending;
use crate::staging::ManualClock;
use crate::{Clock, Core, Effect, Event, Scenario, Step};

impl Scenario {
    /// A managed release in Zurich at `now`, such as "2026-10-16T12:00:00+02:00", with no files.
    pub(crate) fn at(now: &str) -> Self {
        Self {
            now: now.parse().unwrap(),
            time_zone: "Europe/Zurich".to_owned(),
            bundle_id: "io.github.choco.nightcap".to_owned(),
            bundle_writable: false,
            config: None,
            rules: None,
            state: None,
            steps: Vec::new(),
        }
    }

    pub(crate) fn installed(self, instance: Instance, install: Install) -> Self {
        Self {
            bundle_id: match instance {
                Instance::Release => "io.github.choco.nightcap",
                Instance::Dev => "io.github.choco.nightcap.dev",
            }
            .to_owned(),
            bundle_writable: install == Install::SelfManaged,
            ..self
        }
    }

    pub(crate) fn config(self, text: &str) -> Self {
        Self {
            config: Some(text.to_owned()),
            ..self
        }
    }

    pub(crate) fn rules(self, text: &str) -> Self {
        Self {
            rules: Some(text.to_owned()),
            ..self
        }
    }

    pub(crate) fn state(self, text: &str) -> Self {
        Self {
            state: Some(text.to_owned()),
            ..self
        }
    }

    /// The shell reports `event` once the core has started.
    pub(crate) fn then(mut self, event: Event) -> Self {
        self.steps.push(Step::Event(event));
        self
    }

    pub(crate) fn start(self) -> Harness {
        Harness::stage(self, None)
    }
}

/// A staged core, which the tests reach as a `Core`.
pub(crate) struct Harness {
    core: Core,
    files: Arc<Memory>,
    clock: Arc<ManualClock>,
    requests: mpsc::Sender<Pending>,
    time_zone: String,
    bundle_id: String,
    bundle_writable: bool,
}

impl Deref for Harness {
    type Target = Core;

    fn deref(&self) -> &Core {
        &self.core
    }
}

impl Harness {
    fn stage(scenario: Scenario, files: Option<Arc<Memory>>) -> Self {
        let time_zone = scenario.time_zone.clone();
        let bundle_id = scenario.bundle_id.clone();
        let bundle_writable = scenario.bundle_writable;
        let (requests, receiver) = mpsc::channel();
        let (core, files, clock) = scenario.stage(files, receiver).unwrap();
        Self {
            core,
            files,
            clock,
            requests,
            time_zone,
            bundle_id,
            bundle_writable,
        }
    }

    /// Sends a request as the CLI does, and returns the tick's effects and the response. Both
    /// cross the wire format, as over the socket.
    pub(crate) fn request(&self, request: Request) -> (Vec<Effect>, Response) {
        let request = serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
        let (pending, response) = Pending::new(request);
        self.requests.send(pending).unwrap();
        let effects = self.tick();
        let response = response.try_recv().expect("the tick answers the request");
        let response = serde_json::from_str(&serde_json::to_string(&response).unwrap()).unwrap();
        (effects, response)
    }

    /// Sends a command as the CLI does.
    pub(crate) fn send(&self, command: Command) -> (Vec<Effect>, Response) {
        self.request(Request::Command(command))
    }

    /// Runs a command as the menu does, with what the core answers.
    pub(crate) fn choose(&self, command: Command) -> (Vec<Effect>, Result<Decision, ErrorReply>) {
        self.core.choose(&command)
    }

    /// The status, as `nightcap status` asks for it; asking changes nothing.
    pub(crate) fn status(&self) -> Status {
        let (effects, response) = self.request(Request::Query(Query::Status));
        assert_eq!(effects, Vec::new());
        let Response::Ok(Payload::Status(status)) = response else {
            panic!("expected a status, got {response:?}");
        };
        *status
    }

    pub(crate) fn rules(&self) -> Vec<Rule> {
        let (_, response) = self.request(Request::Query(Query::Rules));
        let Response::Ok(Payload::Rules(rules)) = response else {
            panic!("expected rules, got {response:?}");
        };
        rules
    }

    /// Moves the clock to `now`, such as "2026-10-16T23:00:00+02:00".
    pub(crate) fn set_clock(&self, now: &str) {
        self.clock.set(now.parse().unwrap());
    }

    /// Changes the config, which the core reads on its next reload.
    pub(crate) fn write_config(&self, text: &str) {
        self.files
            .put(File::Config, Some(text.as_bytes().to_owned()));
    }

    pub(crate) fn file(&self, file: File) -> Option<String> {
        self.files
            .read(file)
            .unwrap()
            .map(|contents| String::from_utf8(contents).unwrap())
    }

    /// Starts a second core on this one's files, as the same app, as launchd does after a
    /// restart or a crash.
    pub(crate) fn restart(&self) -> Self {
        self.restart_from(self.clock.now())
    }

    /// As `restart`, with the clock at `now`, such as "2026-10-16T23:10:00+02:00".
    pub(crate) fn restart_at(&self, now: &str) -> Self {
        self.restart_from(now.parse().unwrap())
    }

    fn restart_from(&self, now: Timestamp) -> Self {
        let scenario = Scenario {
            now,
            time_zone: self.time_zone.clone(),
            bundle_id: self.bundle_id.clone(),
            bundle_writable: self.bundle_writable,
            config: None,
            rules: None,
            state: None,
            steps: Vec::new(),
        };
        Self::stage(scenario, Some(self.files.clone()))
    }
}
