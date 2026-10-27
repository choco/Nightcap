//! The core's one mailbox, which every sender shares: the shell, the socket's thread and the
//! shell's quit.

use std::sync::mpsc;

use nightcap_protocol::{Request, Response};

use crate::Outcome;
use crate::server::react::Input;

/// What the mailbox holds, each decided in the order it arrived.
#[derive(Debug)]
pub(crate) enum Mail {
    /// A fact or a command from the shell, whose outcome goes to the listener.
    Input(Input),
    /// A request from the CLI that isn't a query, answered on `reply` once decided. Its outcome
    /// goes to the listener like any other (ADR-0010).
    Request {
        request: Request,
        reply: mpsc::Sender<Response>,
    },
    /// The app is quitting, and waits on `reply` for what releases every force (ADR-0008).
    Stop { reply: mpsc::Sender<Outcome> },
}

/// Where mail for the core's thread goes.
#[derive(Debug, Clone)]
pub(crate) struct Mailbox(mpsc::Sender<Mail>);

/// The core's thread has stopped, so nothing reads the mailbox.
#[derive(Debug, thiserror::Error)]
#[error("the core has stopped")]
pub(crate) struct Stopped;

impl Mailbox {
    /// A mailbox, and where the core's thread takes its mail.
    pub(crate) fn new() -> (Self, mpsc::Receiver<Mail>) {
        let (sender, receiver) = mpsc::channel();
        (Self(sender), receiver)
    }

    pub(crate) fn post(&self, mail: Mail) -> Result<(), Stopped> {
        self.0.send(mail).map_err(|_| Stopped)
    }
}
