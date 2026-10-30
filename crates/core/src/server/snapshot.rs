//! What the reads answer from: the core as its thread left it after its last input, published
//! whole, so neither the shell's reads nor the CLI's queries wait on the mailbox.

use std::sync::{Arc, Mutex, MutexGuard};

use jiff::Timestamp;

use nightcap_protocol::{
    About, Answer, Checks, ConfigState, ExportFormat, Front, NightShiftState, OnOff, Payload,
    Permissions, Preferences, Query, Response, RuleIdentity, Rules, Schedule, TrialState,
    WebsiteChecking,
};

use crate::server::state::Inner;
use crate::storage::ad_hoc_rules;
use crate::storage::file::File;

/// The core as it stood after one input, which a read takes whole (ADR-0002): every query, in
/// process and over the socket, and the export answer from it as of that input, whatever the
/// core decides after.
#[derive(Debug, uniffi::Object)]
pub struct Snapshot {
    night_shift: NightShiftState,
    front: Front,
    rules: Rules,
    config: ConfigState,
    preferences: Preferences,
    checks: Checks,
    trial: Option<TrialState>,
    about: About,
    /// The ad-hoc rules, for an export (ADR-0007).
    ad_hoc: Vec<(RuleIdentity, OnOff)>,
}

#[uniffi::export]
impl Snapshot {
    /// Night Shift as the shell last reported it, and its schedule.
    pub fn night_shift(&self) -> NightShiftState {
        self.night_shift
    }

    /// What's in front, and what decides Night Shift there (ADR-0018).
    pub fn front(&self) -> Front {
        self.front.clone()
    }

    /// The apps' and sites' indefinite rules, each with its snooze, apart from every place's
    /// temporary rule (ADR-0018).
    pub fn rules(&self) -> Rules {
        self.rules.clone()
    }

    /// The config as Nightcap last read it, and why it or `rules.toml` can't be used.
    pub fn config(&self) -> ConfigState {
        self.config.clone()
    }

    /// The user's choices about how Nightcap runs.
    pub fn preferences(&self) -> Preferences {
        self.preferences
    }

    /// The checks, and the grants behind them.
    pub fn checks(&self) -> Checks {
        self.checks.clone()
    }

    /// The trial, while one runs (ADR-0016).
    pub fn trial(&self) -> Option<TrialState> {
        self.trial
    }

    /// The Nightcap that answers: its version and its install (ADR-0011).
    pub fn about(&self) -> About {
        self.about.clone()
    }

    /// The ad-hoc rules, written for the config (ADR-0007).
    pub fn export(&self, format: ExportFormat) -> String {
        ad_hoc_rules::export(&self.ad_hoc, format)
    }
}

impl Snapshot {
    /// The core as `inner` stands after the input it decided at `now`.
    pub(crate) fn of(inner: &Inner, now: Timestamp) -> Self {
        let facts = &inner.facts;
        let timeline = &facts.timeline;
        Self {
            night_shift: NightShiftState {
                state: facts.night_shift,
                strength: inner.strength,
                schedule: timeline.schedule.map(Schedule::from),
                set_aside: inner.grants.schedule_set_aside,
                scheduled: timeline
                    .scheduled_state(facts.applied_until)
                    .map(OnOff::from),
                // A turn on is what ends an off, and a turn off what ends an on.
                next_on: timeline.next_turn(facts.applied_until, OnOff::Off),
                next_off: timeline.next_turn(facts.applied_until, OnOff::On),
            },
            front: (inner.places).front(&facts.world(now), inner.watching.is_watching()),
            rules: inner.places.rules(),
            config: ConfigState {
                path: inner.files.path(File::Config).display().to_string(),
                strength: inner.loaded.config_night_shift.strength,
                schedule: inner.loaded.config_night_shift.schedule,
                websites: facts.config_websites.map(OnOff::from),
                lengths: inner.loaded.lengths.clone(),
            },
            preferences: Preferences {
                website_checking: WebsiteChecking {
                    state: OnOff::from(facts.website_checking()),
                    from_config: facts.config_websites.is_some(),
                },
                notifications_wanted: inner.grants.notifications_wanted,
                open_at_login: inner.grants.open_at_login,
            },
            checks: Checks {
                checks: inner.checks(now),
                permissions: Permissions {
                    accessibility: inner.grants.accessibility == Some(true),
                    notifications: inner.grants.notifications,
                    login_item: inner.grants.login_item,
                },
            },
            trial: inner.places.trial(),
            about: About {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                install: inner.install,
            },
            ad_hoc: inner.places.ad_hoc(),
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
            Query::Config => Answer::Config(Box::new(self.config())),
            Query::Preferences => Answer::Preferences(self.preferences()),
            Query::Checks => Answer::Checks(self.checks()),
            Query::Trial => Answer::Trial(self.trial()),
            Query::About => Answer::About(self.about()),
            Query::Export(format) => Answer::Export(self.export(*format)),
        }
    }
}

/// The snapshot last published, which the core's thread replaces after each input. A read can
/// lag one input behind; the outcome that follows carries the snapshot its input left.
#[derive(Debug)]
pub(crate) struct Published(Mutex<Arc<Snapshot>>);

impl Published {
    pub(crate) fn new(snapshot: Snapshot) -> Self {
        Self(Mutex::new(Arc::new(snapshot)))
    }

    /// Publishes `snapshot`, and hands it back as the reads now take it.
    pub(crate) fn publish(&self, snapshot: Snapshot) -> Arc<Snapshot> {
        let published = Arc::new(snapshot);
        *self.lock() = published.clone();
        published
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
