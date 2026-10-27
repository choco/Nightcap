//! Watching the front browser's host, which website checking allows (ADR-0012), and only while
//! the host could change what Night Shift does (ADR-0018). Only the host ever reaches the core
//! (ADR-0009).

use nightcap_protocol::BundleId;

use crate::{Core, Effect, State};

impl Core {
    /// Watches the frontmost browser while website checking is on and its host could change
    /// anything: not on the way out, nor while its own place says off and no site could say on
    /// (ADR-0008, ADR-0012, ADR-0018).
    pub(crate) fn rewatch(&self, state: &mut State, effects: &mut Vec<Effect>) {
        let host_matters = state
            .places
            .look(&state.facts.world(self.clock.now()))
            .host_matters;
        let wanted = state
            .facts
            .context
            .app
            .clone()
            .filter(|_| state.is_browser && state.facts.website_checking() && host_matters);
        match (&state.watch, wanted) {
            (Some(watch), Some(bundle_id)) if watch.bundle_id == bundle_id => {}
            (_, Some(bundle_id)) => {
                state.generation += 1;
                let generation = state.generation;
                state.facts.context.host = None;
                state.watch = Some(Watch {
                    bundle_id: bundle_id.clone(),
                    generation,
                    reported: false,
                });
                effects.push(Effect::WatchBrowser {
                    bundle_id: bundle_id.into(),
                    generation,
                });
            }
            (Some(_), None) => {
                state.facts.context.host = None;
                state.watch = None;
                effects.push(Effect::StopWatching);
            }
            (None, None) => {}
        }
    }
}

/// The browser being watched, and whether its first read under this generation has arrived.
pub(crate) struct Watch {
    pub(crate) bundle_id: BundleId,
    pub(crate) generation: u64,
    pub(crate) reported: bool,
}
