//! What clients can ask: Night Shift, what's in front, the rules, the config, the user's
//! preferences, the checks, the trial and the install, as facts the snapshot carries and each
//! client words itself (ADR-0002, ADR-0010).

use jiff::Timestamp;
use nightcap_protocol::{
    About, Checks, ConfigState, Front, NightShiftState, OnOff, Permissions, Preferences, Rules,
    Schedule, TrialState, WebsiteChecking,
};

use crate::server::state::Inner;
use crate::storage::file::File;

/// Every query's answer as of one input, which the snapshot keeps.
#[derive(Debug)]
pub(crate) struct Answers {
    pub(crate) night_shift: NightShiftState,
    pub(crate) front: Front,
    pub(crate) rules: Rules,
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
        }
    }
}
