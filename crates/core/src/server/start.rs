//! How a core starts: where it lives and what kind of install it serves, the clock it reads,
//! what it finds in its files, and what the shell's launch does.

use std::path::PathBuf;
use std::sync::Arc;

use jiff::tz::TimeZone;
use nightcap_protocol::{Install, Instance, NightShift, NotificationPermission};

use crate::clock::Clock;
use crate::commands::Loaded;
use crate::rules::places::{Context, Kept, Places};
use crate::rules::schedule::{SunTimes, Timeline};
use crate::server::state::{Facts, Inner};
use crate::server::turn::Reconciling;
use crate::server::watch::Watching;
use crate::setup::grants::Grants;
use crate::storage::config::{self, NightShiftConfig};
use crate::storage::file::Files;
use crate::storage::{ad_hoc_rules, saved_state};
use crate::{CoreError, Effect};

/// Where a core lives and what kind of install it serves.
pub(crate) struct Setup {
    pub(crate) instance: Instance,
    pub(crate) install: Install,
    /// The socket the IPC thread serves, which the core removes on the way out.
    pub(crate) socket: Option<PathBuf>,
    pub(crate) files: Arc<dyn Files>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) zone: TimeZone,
}

impl Inner {
    /// The core as it finds its files, with nothing reported yet.
    pub(crate) fn start(setup: Setup) -> Self {
        let store = saved_state::Store::new(setup.files.clone());
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
        let ad_hoc_store = ad_hoc_rules::Store::new(setup.files.clone());
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
        let mut inner = Self {
            instance: setup.instance,
            install: setup.install,
            socket: setup.socket,
            files: setup.files,
            store,
            ad_hoc_store,
            clock: setup.clock.clone(),
            manages_itself: setup.install == Install::SelfManaged
                && setup.instance == Instance::Release,
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
                rules: persisted.rules.clone(),
                linked: persisted.linked.clone(),
                ad_hoc,
            }),
            strength: None,
            reconciling: Reconciling::new(persisted.night_shift),
            watching: Watching::default(),
            grants: Grants {
                accessibility: None,
                notifications: NotificationPermission::NotRequested,
                notifications_wanted: persisted.notifications_wanted,
                login_item: None,
                open_at_login: persisted.open_at_login,
                schedule_set_aside: false,
                schedule_off_accepted: persisted.schedule_off_accepted,
            },
            loaded: Loaded {
                config_error,
                config_found: config.found,
                lengths: config.lengths,
                config_night_shift: config.night_shift,
                applied_from_config: persisted.applied_from_config,
                ad_hoc_error,
            },
            legacy_onboarding: persisted.onboarding,
            saved: persisted,
        };
        inner.graduate(config.rules);
        inner
    }

    /// The shell's launch: starts the updater where this install updates itself, and applies the
    /// config's `[night_shift]`.
    pub(crate) fn launched(&mut self, effects: &mut Vec<Effect>) {
        if self.manages_itself {
            effects.push(Effect::StartUpdater);
        }
        self.apply_config_night_shift(effects);
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
