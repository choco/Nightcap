//! A staged core for the tests, which they reach as a `Core`.

use std::ops::Deref;

use nightcap_protocol::{
    Command, Decision, ErrorReply, Payload, Query, Request, Response, Rule, Status,
};

use crate::file::File;
use crate::{Core, Effect, Scenario, Staged};

impl Scenario {
    pub(crate) fn start(self) -> Harness {
        Harness(self.stage().unwrap())
    }
}

/// A staged core, which the tests reach as a `Core`.
pub(crate) struct Harness(Staged);

impl Deref for Harness {
    type Target = Core;

    fn deref(&self) -> &Core {
        self.0.core()
    }
}

impl Harness {
    /// Sends a request as the CLI does, and returns the tick's effects and the response. Both
    /// cross the wire format, as over the socket.
    pub(crate) fn request(&self, request: Request) -> (Vec<Effect>, Response) {
        self.0.request(request)
    }

    /// Sends a command as the CLI does.
    pub(crate) fn send(&self, command: Command) -> (Vec<Effect>, Response) {
        self.request(Request::Command(command))
    }

    /// Runs a command as the menu does, with what the core answers.
    pub(crate) fn choose(&self, command: Command) -> (Vec<Effect>, Result<Decision, ErrorReply>) {
        self.0.choose(&command)
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
        self.0.set_now(now.parse().unwrap());
    }

    /// Changes the config, which the core reads on its next reload.
    pub(crate) fn write_config(&self, text: &str) {
        self.0.write_config(Some(text));
    }

    pub(crate) fn file(&self, file: File) -> Option<String> {
        self.0.file(file)
    }

    /// Starts a second core on this one's files, as the same app, as launchd does after a
    /// restart or a crash.
    pub(crate) fn restart(&self) -> Self {
        Self(self.0.restart())
    }

    /// As `restart`, with the clock at `now`, such as "2026-10-16T23:10:00+02:00".
    pub(crate) fn restart_at(&self, now: &str) -> Self {
        Self(self.0.restart_at(now.parse().unwrap()))
    }
}
