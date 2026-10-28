//! What the reads answer from: the core as its thread left it after its last input, published
//! whole, so neither the shell's reads nor the CLI's queries wait on the mailbox.

use std::sync::{Arc, Mutex, MutexGuard};

use nightcap_protocol::{OnOff, Payload, Query, Response, RuleIdentity};

use crate::queries::Answers;
use crate::server::state::Inner;
use crate::storage::ad_hoc_rules;

/// The core after one input, as each read sees it.
#[derive(Debug)]
pub(crate) struct Snapshot {
    /// What the queries answer, in process and over the socket.
    pub(crate) answers: Answers,
    /// The ad-hoc rules, for an export (ADR-0007).
    pub(crate) ad_hoc: Vec<(RuleIdentity, OnOff)>,
}

impl Snapshot {
    pub(crate) fn of(inner: &Inner) -> Self {
        let now = inner.clock.now();
        Self {
            answers: Answers::new(inner, now),
            ad_hoc: inner.places.kept().ad_hoc.clone(),
        }
    }

    /// The CLI's query, answered as of this snapshot (ADR-0010).
    pub(crate) fn answer(&self, query: &Query) -> Response {
        let answers = &self.answers;
        match query {
            Query::NightShift => Response::Ok(Payload::NightShift(answers.night_shift)),
            Query::Front => Response::Ok(Payload::Front(Box::new(answers.front.clone()))),
            Query::Rules => Response::Ok(Payload::Rules(answers.rules.clone())),
            Query::Config => Response::Ok(Payload::Config(answers.config.clone())),
            Query::Preferences => Response::Ok(Payload::Preferences(answers.preferences)),
            Query::Checks => Response::Ok(Payload::Checks(answers.checks.clone())),
            Query::Tour => Response::Ok(Payload::Tour(answers.tour)),
            Query::About => Response::Ok(Payload::About(answers.about.clone())),
            Query::Export(format) => {
                Response::Ok(Payload::Export(ad_hoc_rules::export(&self.ad_hoc, *format)))
            }
        }
    }
}

/// The snapshot last published, which the core's thread replaces after each input. A read can
/// lag one input behind; the outcome that follows tells the shell to read again.
#[derive(Debug)]
pub(crate) struct Published(Mutex<Arc<Snapshot>>);

impl Published {
    pub(crate) fn new(snapshot: Snapshot) -> Self {
        Self(Mutex::new(Arc::new(snapshot)))
    }

    pub(crate) fn publish(&self, snapshot: Snapshot) {
        *self.lock() = Arc::new(snapshot);
    }

    pub(crate) fn latest(&self) -> Arc<Snapshot> {
        self.lock().clone()
    }

    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    fn lock(&self) -> MutexGuard<'_, Arc<Snapshot>> {
        self.0.lock().expect("no panic happens while it's held")
    }
}
