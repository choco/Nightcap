//! The core's one mailbox, which every sender shares: the shell, the socket's thread and the
//! shell's quit.

use std::sync::mpsc;

use crate::Outcome;
use crate::server::react::Input;

/// What the mailbox holds, each decided in the order it arrived.
#[derive(Debug)]
pub(crate) enum Mail {
    /// A fact or a command, from the shell or the CLI, whose outcome goes to the listener.
    Input(Input),
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
