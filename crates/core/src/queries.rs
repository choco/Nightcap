//! What clients can ask: Night Shift, what's in front, the rules, the config, the user's
//! preferences, the checks, the trial and the install, as facts the snapshot carries and each
//! client words itself (ADR-0002, ADR-0010).

use jiff::Timestamp;
use nightcap_protocol::{
    About, Checks, ConfigState, FileError, Front, Inherited, NightShiftState, OnOff, Permissions,
    Place, PlaceInFront, Preferences, Rule, Schedule, TrialState, WebsiteChecking,
};

use crate::rules::places::PlaceLook;
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
    pub(crate) trial: Option<TrialState>,
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
                // A turn on is what ends an off, and a turn off what ends an on.
                next_on: timeline.next_turn(facts.applied_until, OnOff::Off),
                next_off: timeline.next_turn(facts.applied_until, OnOff::On),
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
                places: look.site.iter().chain(&look.app).map(in_front).collect(),
                everywhere: in_front(&look.everywhere),
                after_switch: look.after_switch.clone(),
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
                onboarding_asks: inner.onboarding_asks(),
            },
            checks: Checks {
                checks: inner.checks(now),
                permissions: Permissions {
                    accessibility: inner.grants.accessibility == Some(true),
                    notifications: inner.grants.notifications,
                    login_item: inner.grants.login_item,
                },
            },
            trial: inner.places.trial().map(|trial| TrialState {
                stands_in: trial.stands_in(),
            }),
            about: About {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                install: inner.install,
            },
        }
    }
}

/// A place in front as `front()` answers it.
fn in_front(at: &PlaceLook) -> PlaceInFront {
    PlaceInFront {
        place: at.place.clone(),
        gives: at.gives,
        rules: at.rules.clone(),
        base: at.base.clone(),
        inherits: at
            .inherits
            .clone()
            .map(|(from, says)| Inherited { from, says }),
        after_until: at.after_until,
        after_reset: at.after_reset,
    }
}

impl Inner {
    /// Every rule at every place: the apps' and sites' indefinite ones first, the config's, then
    /// the ad-hoc ones, then the trial's.
    pub(crate) fn every_rule(&self, now: Timestamp) -> Vec<Rule> {
        self.places
            .look(&self.facts.world(now))
            .rules
            .into_iter()
            .map(|(rule, _)| rule.clone())
            .collect()
    }
}
