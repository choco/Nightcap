//! What the core decides from, which its thread alone owns (ADR-0002).

use std::path::PathBuf;
use std::sync::Arc;

use jiff::Timestamp;
use nightcap_protocol::{Install, NightShift, OnOff};

use crate::clock::Clock;
use crate::commands::Loaded;
use crate::rules::places::{Context, Places, World};
use crate::rules::schedule::Timeline;
use crate::server::turn::{Reconciling, wanted};
use crate::server::watch::Watching;
use crate::setup::grants::Grants;
use crate::storage::ad_hoc_rules;
use crate::storage::file::Files;
use crate::storage::saved_state::{Persisted, Store};

/// What the core decides from, which its thread alone owns (ADR-0002): what it believes about
/// the system, as last reported by the shell, what it keeps across restarts, and the files and
/// the clock it keeps them by.
pub(crate) struct Inner {
    /// Whether Nightcap updates itself through Sparkle and opens itself at login if the user
    /// wants (ADR-0011).
    pub(crate) install: Install,
    /// The socket the IPC thread serves, which the core removes on the way out.
    pub(crate) socket: Option<PathBuf>,
    pub(crate) files: Arc<dyn Files>,
    pub(crate) store: Store,
    pub(crate) ad_hoc_store: ad_hoc_rules::Store,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) facts: Facts,
    pub(crate) places: Places,
    pub(crate) strength: Option<u8>,
    pub(crate) reconciling: Reconciling,
    pub(crate) watching: Watching,
    pub(crate) grants: Grants,
    pub(crate) loaded: Loaded,
    /// What was last written to `state.json`.
    pub(crate) saved: Persisted,
}

/// What the places read but don't own, lent to them as a `World` for each call.
pub(crate) struct Facts {
    pub(crate) night_shift: NightShift,
    /// The write CoreBrightness last marked, while it stands (ADR-0004).
    pub(crate) written_at: Option<Timestamp>,
    pub(crate) timeline: Timeline,
    pub(crate) context: Context,
    /// Every schedule transition up to here has been applied.
    pub(crate) applied_until: Timestamp,
    /// Website checking as the user chose it; `websites` in the config wins (ADR-0012).
    pub(crate) websites: bool,
    pub(crate) config_websites: Option<bool>,
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
    pub(crate) fn save(&mut self) {
        let persisted = self.persisted(self.clock.now());
        if persisted == self.saved {
            return;
        }
        match self.store.save(&persisted) {
            Ok(()) => self.saved = persisted,
            Err(error) => tracing::warn!("couldn't save the state: {error}"),
        }
    }
}
