//! What the core decides from, which its thread alone owns (ADR-0002).

use std::sync::Arc;

use jiff::Timestamp;
use nightcap_protocol::{Install, NightShift, OnOff};

use crate::commands::Loaded;
use crate::rules::places::{Context, Places, World};
use crate::rules::schedule::Timeline;
use crate::server::turn::wanted;
use crate::server::watch::Watching;
use crate::setup::grants::Grants;
use crate::storage::ad_hoc_rules;
use crate::storage::file::Files;
use crate::storage::saved_state::{Persisted, Store};

/// What the core decides from, which its thread alone owns (ADR-0002): what it believes about
/// the system, as last reported by the shell, what it keeps across restarts, and the files it
/// keeps them in. The time comes with each input, from whoever runs the core.
pub(crate) struct Inner {
    /// Whether Nightcap updates itself through Sparkle and opens itself at login if the user
    /// wants (ADR-0011).
    pub(crate) install: Install,
    pub(crate) files: Arc<dyn Files>,
    pub(crate) store: Store,
    pub(crate) ad_hoc_store: ad_hoc_rules::Store,
    pub(crate) facts: Facts,
    pub(crate) places: Places,
    pub(crate) strength: Option<u8>,
    /// The wakeup the shell was last asked for, until it fires.
    pub(crate) wakeup: Option<Timestamp>,
    pub(crate) watching: Watching,
    pub(crate) grants: Grants,
    pub(crate) loaded: Loaded,
    /// What was last written to `state.json`.
    pub(crate) saved: Persisted,
}

/// What the places read but don't own, lent to them as a `World` for each call, and what the
/// core believes about Night Shift and the session from the shell's reports.
pub(crate) struct Facts {
    pub(crate) night_shift: NightShift,
    /// The write CoreBrightness last marked, while it stands (ADR-0004).
    pub(crate) written_at: Option<Timestamp>,
    /// Where the last run left Night Shift, which the first status report shows: not an outside
    /// change (ADR-0008).
    pub(crate) left_by_last_run: Option<OnOff>,
    pub(crate) session: Session,
    pub(crate) timeline: Timeline,
    /// Since when sunset to sunrise has had no transition ahead, for want of sun times.
    pub(crate) sun_times_awaited_since: Option<Timestamp>,
    pub(crate) context: Context,
    /// Every schedule transition up to here has been applied.
    pub(crate) applied_until: Timestamp,
    /// Website checking as the user chose it; `websites` in the config wins (ADR-0012).
    pub(crate) websites: bool,
    pub(crate) config_websites: Option<bool>,
}

/// Whether Night Shift is this user's to change: with fast user switching, another user's
/// session can be in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Session {
    Active,
    /// Another user is in front: status callbacks aren't this user's, and nothing is written.
    Away,
    /// Back in front, waiting for the shell's fresh read of a status that went unwatched.
    Back,
}

impl Facts {
    pub(crate) fn world(&self, now: Timestamp) -> World<'_> {
        World {
            context: &self.context,
            timeline: &self.timeline,
            applied_until: self.applied_until,
            now,
            observed: OnOff::from(self.night_shift == NightShift::On),
            website_checking: self.website_checking(),
        }
    }

    pub(crate) fn website_checking(&self) -> bool {
        self.config_websites.unwrap_or(self.websites)
    }

    pub(crate) fn session_changed(&mut self, active: bool) {
        tracing::info!("session active: {active}");
        self.session = if active { Session::Back } else { Session::Away };
    }
}

impl Inner {
    /// Mid-trial, the state the trial's end would leave, with Night Shift as the trial holds it:
    /// a run after a crash finds it there, as the last run left it, and puts it back (ADR-0016).
    fn persisted(&self, now: Timestamp) -> Persisted {
        let (rules, linked) = self.places.kept();
        Persisted {
            rules,
            linked,
            websites: self.facts.websites,
            notifications_wanted: self.grants.notifications_wanted,
            open_at_login: self.grants.open_at_login,
            night_shift: wanted(&self.places.decide(&self.facts.world(now))).map(OnOff::from),
            until: self
                .facts
                .timeline
                .next_transition(self.facts.applied_until),
            applied_from_config: self.loaded.applied_from_config,
            schedule_off_accepted: self.grants.schedule_off_accepted,
        }
    }

    /// Writes `state.json` whenever what it holds changes, so a crash loses nothing (ADR-0008).
    pub(crate) fn save(&mut self, now: Timestamp) {
        let persisted = self.persisted(now);
        if persisted == self.saved {
            return;
        }
        match self.store.save(&persisted) {
            Ok(()) => self.saved = persisted,
            Err(error) => tracing::warn!("couldn't save the state: {error}"),
        }
    }
}
