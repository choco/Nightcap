//! What the reads answer from: the core as its thread left it after its last input, published
//! whole, so neither the shell's reads nor the CLI's queries wait on the mailbox.

use std::sync::{Arc, Mutex, MutexGuard};

use nightcap_protocol::{ErrorCode, OnOff, Payload, Query, Response, RuleIdentity, Status};

use crate::commands::refused;
use crate::server::state::Inner;
use crate::storage::ad_hoc_rules;
use crate::storage::file::File;
use crate::{Menu, OnboardingSteps, Settings, TourSnapshot};

/// The core after one input, as each read sees it.
#[derive(Debug)]
pub(crate) struct Snapshot {
    pub(crate) menu: Menu,
    pub(crate) settings: Settings,
    pub(crate) onboarding: OnboardingSteps,
    /// None while no tour runs.
    pub(crate) tour: Option<TourSnapshot>,
    /// The CLI's report, which lists every rule too.
    pub(crate) status: Status,
    /// The ad-hoc rules, for an export (ADR-0007).
    pub(crate) ad_hoc: Vec<(RuleIdentity, OnOff)>,
}

impl Snapshot {
    pub(crate) fn of(inner: &Inner) -> Self {
        let now = inner.clock.now();
        Self {
            menu: Menu::new(inner, now),
            settings: Settings::new(inner, now, &inner.files.path(File::Config)),
            onboarding: OnboardingSteps::new(inner),
            tour: inner
                .tour
                .as_ref()
                .map(|tour| tour.report(&inner.places.look(&inner.facts.world(now)))),
            status: inner.status(),
            ad_hoc: inner.places.kept().ad_hoc.clone(),
        }
    }

    /// The CLI's query, answered as of this snapshot (ADR-0010).
    pub(crate) fn answer(&self, query: &Query) -> Response {
        match query {
            Query::Status => Response::Ok(Payload::Status(Box::new(self.status.clone()))),
            Query::Rules => Response::Ok(Payload::Rules(self.status.rules.clone())),
            Query::Export(format) => match ad_hoc_rules::export(&self.ad_hoc, *format) {
                Ok(text) => Response::Ok(Payload::Export(text)),
                Err(error) => Response::Error(refused(
                    ErrorCode::Internal,
                    format!("couldn't write the rules as TOML: {error}"),
                )),
            },
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
