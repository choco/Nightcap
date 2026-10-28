//! What clients can ask: Night Shift, what's in front, the rules, the config, the user's
//! preferences, the checks, the tour and the install, as facts the snapshot carries and each
//! client words itself (ADR-0002, ADR-0010).

use jiff::Timestamp;
use nightcap_protocol::{
    About, Checks, ConfigState, FileError, Front, NightShiftState, OnOff, Permissions, Place,
    Preferences, Reason, Rule, Schedule, TourState, Turn, WebsiteChecking,
};

use crate::server::state::Inner;
use crate::storage::file::File;

/// Every query's answer as of one input, which the snapshot keeps.
#[derive(Debug)]
pub(crate) struct Answers {
    pub(crate) night_shift: NightShiftState,
    pub(crate) front: Front,
    pub(crate) rules: Vec<Rule>,
    pub(crate) config: ConfigState,
    pub(crate) preferences: Preferences,
    pub(crate) checks: Checks,
    pub(crate) tour: Option<TourState>,
    pub(crate) about: About,
}

impl Answers {
    /// The answers as `inner` has them at `now`.
    pub(crate) fn new(inner: &Inner, now: Timestamp) -> Self {
        let facts = &inner.facts;
        let timeline = &facts.timeline;
        let look = inner.places.look(&facts.world(now));
        Self {
            night_shift: NightShiftState {
                state: facts.night_shift,
                strength: inner.strength,
                schedule: timeline.schedule.map(Schedule::from),
                set_aside: inner.grants.schedule_set_aside,
                scheduled: timeline
                    .scheduled_state(facts.applied_until)
                    .map(OnOff::from),
                next_turn: timeline
                    .next_transition(facts.applied_until)
                    .and_then(|at| {
                        timeline.scheduled_state(at).map(|on| Turn {
                            at,
                            to: OnOff::from(on),
                        })
                    }),
            },
            front: Front {
                app: facts.context.app.clone(),
                watching: inner.watching.is_watching(),
                host: facts
                    .context
                    .host
                    .as_ref()
                    .map(|host| host.as_str().to_owned()),
                site: look.site.as_ref().and_then(|site| match &site.place {
                    Place::Domain(domain) => Some(domain.clone()),
                    Place::Everywhere | Place::App(_) => None,
                }),
                decision: look.decision.clone(),
            },
            rules: inner.every_rule(now),
            config: ConfigState {
                path: inner.files.path(File::Config).display().to_string(),
                found: inner.loaded.config_found,
                error: inner.loaded.config_error.as_ref().map(FileError::from),
                strength: inner.loaded.config_night_shift.strength,
                schedule: inner.loaded.config_night_shift.schedule,
                websites: facts.config_websites.map(OnOff::from),
                lengths: inner.loaded.lengths.clone(),
                ad_hoc_error: inner.loaded.ad_hoc_error.as_ref().map(FileError::from),
            },
            preferences: Preferences {
                website_checking: WebsiteChecking {
                    state: OnOff::from(facts.website_checking()),
                    from_config: facts.config_websites.is_some(),
                },
                notifications_wanted: inner.grants.notifications_wanted,
                open_at_login: inner.grants.open_at_login,
                onboarding_completed: inner.onboarding,
                onboarding_finished: inner.onboarding_done(),
            },
            checks: Checks {
                checks: inner.checks(now),
                permissions: Permissions {
                    accessibility: inner.grants.accessibility == Some(true),
                    notifications: inner.grants.notifications,
                },
            },
            tour: inner.tour.as_ref().map(|_| TourState {
                stands_in: look.everywhere.base == Some(Reason::Tour),
            }),
            about: About {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                install: inner.install,
            },
        }
    }
}

impl Inner {
    /// Every rule at every place: the apps' and sites' indefinite ones first, the config's, then
    /// the ad-hoc ones, then the tour's.
    pub(crate) fn every_rule(&self, now: Timestamp) -> Vec<Rule> {
        self.places
            .look(&self.facts.world(now))
            .rules
            .into_iter()
            .map(|(rule, _)| rule.clone())
            .collect()
    }
}
