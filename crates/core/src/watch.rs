//! The context rules match against: the front app, and the host of the browser in front, which
//! Nightcap reads by watching it while website checking allows (ADR-0012) and the host could
//! change what Night Shift does (ADR-0018). Only the host ever reaches the core (ADR-0009).

use nightcap_protocol::BundleId;

use crate::domain::Host;
use crate::state::State;
use crate::{Core, Effect};

/// The front app's place in the shell's browser list, and the watch on it.
#[derive(Default)]
pub(crate) struct Watching {
    /// Whether the frontmost app is in the shell's browser list.
    is_browser: bool,
    watch: Option<Watch>,
    /// The generation of the last watch started.
    generation: u64,
}

impl Watching {
    /// Whether a browser's host is being read.
    pub(crate) fn is_watching(&self) -> bool {
        self.watch.is_some()
    }

    /// A watch started and its first read hasn't arrived.
    pub(crate) fn awaits_first_read(&self) -> bool {
        self.watch.as_ref().is_some_and(|watch| !watch.reported)
    }
}

impl State {
    /// An app came to the front, by its bundle ID and name if it has them.
    pub(crate) fn app_activated(
        &mut self,
        bundle_id: Option<String>,
        name: Option<String>,
        is_browser: bool,
    ) {
        tracing::debug!("frontmost app: {bundle_id:?}, browser: {is_browser}");
        // An app whose bundle ID no rule could name counts as having none.
        let bundle_id = bundle_id.and_then(|bundle_id| {
            bundle_id
                .parse()
                .inspect_err(|error| tracing::debug!("{error}; it counts as having none"))
                .ok()
        });
        self.facts.context.app = bundle_id;
        self.facts.context.app_name = name;
        self.watching.is_browser = is_browser;
    }

    /// The watched browser's host, read under `generation`, which a stale watch's doesn't
    /// change.
    pub(crate) fn host_changed(&mut self, generation: u64, host: Option<String>) {
        // The host itself is never logged (ADR-0009).
        match &mut self.watching.watch {
            Some(watch) if watch.generation == generation => {
                watch.reported = true;
                self.facts.context.host = host.as_deref().and_then(Host::new);
                tracing::debug!("host reported for watch {generation}");
            }
            _ => tracing::debug!("dropped a host from stale watch {generation}"),
        }
    }
}

impl Core {
    /// Watches the frontmost browser while website checking is on and its host could change
    /// anything: not on the way out, nor while its own place says off and no site could say on
    /// (ADR-0008, ADR-0012, ADR-0018).
    pub(crate) fn rewatch(&self, state: &mut State, effects: &mut Vec<Effect>) {
        let host_matters = state
            .places
            .look(&state.facts.world(self.clock.now()))
            .host_matters;
        let wanted = state.facts.context.app.clone().filter(|_| {
            state.watching.is_browser && state.facts.website_checking() && host_matters
        });
        match (&state.watching.watch, wanted) {
            (Some(watch), Some(bundle_id)) if watch.bundle_id == bundle_id => {}
            (_, Some(bundle_id)) => {
                state.watching.generation += 1;
                let generation = state.watching.generation;
                state.facts.context.host = None;
                state.watching.watch = Some(Watch {
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
                state.watching.watch = None;
                effects.push(Effect::StopWatching);
            }
            (None, None) => {}
        }
    }
}

/// The browser being watched, and whether its first read under this generation has arrived.
struct Watch {
    bundle_id: BundleId,
    generation: u64,
    reported: bool,
}
