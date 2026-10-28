//! What the reads answer from: the core as its thread left it after its last input, published
//! whole, so neither the shell's reads nor the CLI's queries wait on the mailbox.

use std::sync::{Arc, Mutex, MutexGuard};

use nightcap_protocol::{
    About, Answer, Checks, ConfigState, ExportFormat, Front, NightShiftState, OnOff, Payload,
    Preferences, Query, Response, Rule, RuleIdentity, TrialState,
};

use crate::queries::Answers;
use crate::server::state::Inner;
use crate::storage::ad_hoc_rules;

/// The core as it stood after one input, which a read takes whole (ADR-0002): every query and
/// the export answer from it as of that input, whatever the core decides after.
#[derive(Debug, uniffi::Object)]
pub struct Snapshot {
    /// What the queries answer, in process and over the socket.
    pub(crate) answers: Answers,
    /// The ad-hoc rules, for an export (ADR-0007).
    pub(crate) ad_hoc: Vec<(RuleIdentity, OnOff)>,
}

#[uniffi::export]
impl Snapshot {
    /// Night Shift as the shell last reported it, and its schedule.
    pub fn night_shift(&self) -> NightShiftState {
        self.answers.night_shift
    }

    /// What's in front, and what decides Night Shift there (ADR-0018).
    pub fn front(&self) -> Front {
        self.answers.front.clone()
    }

    /// Every rule at every place (ADR-0018).
    pub fn rules(&self) -> Vec<Rule> {
        self.answers.rules.clone()
    }

    /// The config as Nightcap last read it, and why it or `rules.toml` can't be used.
    pub fn config(&self) -> ConfigState {
        self.answers.config.clone()
    }

    /// The user's choices about how Nightcap runs.
    pub fn preferences(&self) -> Preferences {
        self.answers.preferences
    }

    /// The checks, and the grants behind them.
    pub fn checks(&self) -> Checks {
        self.answers.checks.clone()
    }

    /// The trial, while one runs (ADR-0016).
    pub fn trial(&self) -> Option<TrialState> {
        self.answers.trial
    }

    /// The Nightcap that answers: its version and its install (ADR-0011).
    pub fn about(&self) -> About {
        self.answers.about.clone()
    }

    /// The ad-hoc rules, written for the config (ADR-0007).
    pub fn export(&self, format: ExportFormat) -> String {
        ad_hoc_rules::export(&self.ad_hoc, format)
    }
}

impl Snapshot {
    pub(crate) fn of(inner: &Inner) -> Self {
        let now = inner.clock.now();
        Self {
            answers: Answers::new(inner, now),
            ad_hoc: inner.places.kept().ad_hoc.clone(),
        }
    }

    /// The CLI's queries, answered in the order asked, all as of this snapshot (ADR-0010).
    pub(crate) fn answer(&self, queries: &[Query]) -> Response {
        Response::Ok(Payload::Answers(
            queries.iter().map(|query| self.answer_to(query)).collect(),
        ))
    }

    fn answer_to(&self, query: &Query) -> Answer {
        match query {
            Query::NightShift => Answer::NightShift(self.night_shift()),
            Query::Front => Answer::Front(Box::new(self.front())),
            Query::Rules => Answer::Rules(self.rules()),
            Query::Config => Answer::Config(self.config()),
            Query::Preferences => Answer::Preferences(self.preferences()),
            Query::Checks => Answer::Checks(self.checks()),
            Query::Trial => Answer::Trial(self.trial()),
            Query::About => Answer::About(self.about()),
            Query::Export(format) => Answer::Export(self.export(*format)),
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
