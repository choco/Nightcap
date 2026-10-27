//! How a core starts: where it lives and what kind of install it serves, the clock it reads,
//! what it finds in its files, and what the shell's launch does.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{Install, Instance, NightShift, NotificationPermission};

use crate::checks::Grants;
use crate::commands::Loaded;
use crate::config::{self, NightShiftConfig};
use crate::file::Files;
use crate::places::{Context, Kept, Places};
use crate::schedule::{SunTimes, Timeline};
use crate::state::{Facts, State};
use crate::turn::Reconciling;
use crate::watch::Watching;
use crate::{Core, CoreError, Effect, adhoc, ipc, state};

/// Where a core lives and what kind of install it serves.
pub(crate) struct Setup {
    pub(crate) instance: Instance,
    pub(crate) install: Install,
    /// The socket the IPC thread serves, which the core removes on the way out.
    pub(crate) socket: Option<PathBuf>,
    /// The requests to answer on the next tick, which the IPC thread queues.
    pub(crate) requests: mpsc::Receiver<ipc::Pending>,
    pub(crate) files: Arc<dyn Files>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) zone: TimeZone,
}

/// Where the core reads the time, so that tests control it completely.
pub(crate) trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

impl Core {
    pub(crate) fn start(setup: Setup) -> Self {
        let store = state::Store::new(setup.files.clone());
        let mut persisted = store.load();
        persisted.expire(setup.clock.now());
        let (config, config_error) = match config::load(setup.files.as_ref()) {
            Ok(config) => (config, None),
            Err(error) => {
                tracing::warn!(
                    "running without config rules: the config {}; see status",
                    error.kind()
                );
                let config = config::Config {
                    rules: Vec::new(),
                    websites: None,
                    night_shift: NightShiftConfig::default(),
                    lengths: config::default_lengths(),
                    found: true,
                };
                (config, Some(error))
            }
        };
        let ad_hoc_store = adhoc::Store::new(setup.files.clone());
        let (ad_hoc, ad_hoc_error) = match ad_hoc_store.load() {
            Ok(rules) => (rules, None),
            Err(error) => {
                tracing::warn!(
                    "ad-hoc rules are off: rules.toml {}; see status",
                    error.kind()
                );
                (Vec::new(), Some(error))
            }
        };
        let core = Self {
            instance: setup.instance,
            install: setup.install,
            socket: setup.socket,
            files: setup.files,
            store,
            ad_hoc_store,
            clock: setup.clock.clone(),
            requests: Mutex::new(setup.requests),
            state: Mutex::new(State {
                facts: Facts {
                    night_shift: NightShift::Unavailable,
                    timeline: Timeline {
                        schedule: None,
                        sun_times: SunTimes::default(),
                        zone: setup.zone,
                    },
                    context: Context::default(),
                    applied_until: setup.clock.now(),
                    websites: persisted.websites,
                    config_websites: config.websites,
                },
                places: Places::new(Kept {
                    layers: persisted.layers.clone(),
                    ad_hoc,
                }),
                strength: None,
                loaded: Loaded {
                    config_error,
                    config_found: config.found,
                    lengths: config.lengths,
                    config_night_shift: config.night_shift,
                    applied_from_config: persisted.applied_from_config,
                    ad_hoc_error,
                },
                onboarding: persisted.onboarding,
                reconciling: Reconciling::new(persisted.night_shift),
                watching: Watching::default(),
                grants: Grants {
                    accessibility: None,
                    notifications: NotificationPermission::NotRequested,
                    notifications_wanted: persisted.notifications_wanted,
                    login_item: None,
                    open_at_login: persisted.open_at_login,
                    opens_itself_at_login: setup.install == Install::SelfManaged
                        && setup.instance == Instance::Release,
                    schedule_set_aside: false,
                    schedule_off_accepted: persisted.schedule_off_accepted,
                    announced: persisted.announced.clone(),
                },
                last_version: persisted.last_version.clone(),
                tour: None,
                saved: persisted,
            }),
        };
        core.graduate(&mut core.state(), config.rules);
        core
    }

    /// Only a self-managed release opens at login and updates through Sparkle: an installer
    /// owns a managed one, and a dev build would shadow the installed app (ADR-0011).
    pub(crate) fn updates_itself(&self) -> bool {
        self.install == Install::SelfManaged && self.instance == Instance::Release
    }

    /// The shell's first event: says what's new after an update, offers onboarding, starts the
    /// updater where this install updates itself, and applies the config's `[night_shift]`.
    pub(crate) fn launched(&self, state: &mut State) -> Vec<Effect> {
        let mut effects = Vec::new();
        // Updates install silently, so the first launch of a new version says what's new.
        // A first run has nothing to compare; one from before versions were kept has
        // finished onboarding.
        let version = env!("CARGO_PKG_VERSION");
        let updated = match &state.last_version {
            Some(last) => last != version,
            None => state.onboarding > 0,
        };
        if updated {
            tracing::info!("updated to {version}");
            effects.push(Effect::Notify {
                title: format!("Nightcap updated to {version}"),
                body: "Click to see what’s new.".to_owned(),
                opens: Some(format!("https://nightcap.choco.me/changelog/#v{version}")),
            });
        }
        state.last_version = Some(version.to_owned());
        state.offer_onboarding(&mut effects);
        if self.updates_itself() {
            effects.push(Effect::StartUpdater);
        }
        state.apply_config_night_shift(&mut effects);
        effects
    }
}

/// The instance the app with this bundle ID is.
pub(crate) fn instance_of(bundle_id: &str) -> Result<Instance, CoreError> {
    Instance::from_bundle_id(bundle_id).ok_or_else(|| CoreError::UnknownBundleId {
        bundle_id: bundle_id.to_owned(),
    })
}

/// A bundle the user can replace makes a self-managed install (ADR-0011).
pub(crate) fn install_of(bundle_writable: bool) -> Install {
    if bundle_writable {
        Install::SelfManaged
    } else {
        Install::Managed
    }
}

/// The system's time zone, by its IANA ID.
pub(crate) fn zone_of(iana_id: &str) -> Result<TimeZone, CoreError> {
    TimeZone::get(iana_id).map_err(|error| {
        if jiff::tz::db().is_definitively_empty() {
            CoreError::Io {
                message: format!("can't read the time zone database: {error}"),
            }
        } else {
            CoreError::UnknownTimeZone {
                iana_id: iana_id.to_owned(),
            }
        }
    })
}
