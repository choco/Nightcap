//! How a core starts: where it lives and what kind of install it serves, the clock it reads,
//! what it finds in its files, and what the shell's launch does.

use std::sync::Arc;

use jiff::tz::TimeZone;
use nightcap_protocol::{Install, Instance, NightShift, NotSelfManaged, NotificationPermission};

use crate::clock::Clock;
use crate::rules::{Context, Rules};
use crate::schedule::{SunTimes, Timeline};
use crate::server::state::{Facts, Inner, Loaded, Session};
use crate::server::watch::Watching;
use crate::setup::SetupState;
use crate::storage::config::{self, NightShiftConfig};
use crate::storage::file::Files;
use crate::storage::{ad_hoc_rules, saved_state};
use crate::{CoreError, Effect};

/// Where a core lives and what kind of install it serves, and the clock it starts by.
pub(crate) struct Setup {
    pub(crate) install: Install,
    pub(crate) files: Arc<dyn Files>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) zone: TimeZone,
}

impl Inner {
    /// The core as it finds its files, with nothing reported yet.
    pub(crate) fn start(setup: Setup) -> Self {
        let now = setup.clock.now();
        let store = saved_state::Store::new(setup.files.clone());
        let mut persisted = store.load();
        persisted.expire(now);
        let (config, config_error) = match config::load(setup.files.as_ref()) {
            Ok(config) => (config, None),
            Err(error) => {
                tracing::warn!("running without config rules: {error}; see status");
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
                tracing::warn!("ad-hoc rules are off: {error}; see status");
                (Vec::new(), Some(error))
            }
        };
        let mut inner = Self {
            install: setup.install,
            files: setup.files,
            store,
            ad_hoc_store,
            facts: Facts {
                night_shift: NightShift::Unavailable,
                written_at: None,
                left_by_last_run: persisted.night_shift,
                session: Session::Active,
                timeline: Timeline {
                    schedule: None,
                    sun_times: SunTimes::default(),
                    zone: setup.zone,
                },
                sun_times_awaited_since: None,
                context: Context::default(),
                applied_until: now,
                websites: persisted.websites,
                config_websites: config.websites,
            },
            rules: Rules::new(persisted.rules.clone(), persisted.linked.clone(), ad_hoc),
            strength: None,
            wakeup: None,
            watching: Watching::default(),
            setup: SetupState {
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
            saved: persisted,
        };
        inner.graduate(config.rules);
        inner
    }

    /// The shell's launch: starts the updater where this install updates itself, and applies the
    /// config's `[night_shift]`.
    pub(crate) fn launched(&mut self) -> Vec<Effect> {
        let updater = (self.install == Install::SelfManaged).then_some(Effect::StartUpdater);
        updater
            .into_iter()
            .chain(self.apply_config_night_shift())
            .collect()
    }
}

/// The instance the app with this bundle ID is.
pub(crate) fn instance_of(bundle_id: &str) -> Result<Instance, CoreError> {
    Instance::from_bundle_id(bundle_id).ok_or_else(|| CoreError::UnknownBundleId {
        bundle_id: bundle_id.to_owned(),
    })
}

/// The install an instance's bundle makes (ADR-0011): a release the user can replace is
/// self-managed, and any bundle the user can't replace a managed install, a dev build's too.
pub(crate) fn install_of(instance: Instance, bundle_writable: bool) -> Install {
    match (instance, bundle_writable) {
        (Instance::Release, true) => Install::SelfManaged,
        (_, false) => Install::NotSelfManaged(NotSelfManaged::ManagedInstall),
        (Instance::Dev, true) => Install::NotSelfManaged(NotSelfManaged::DevBuild),
    }
}

/// The system's time zone, by its IANA ID.
pub(crate) fn zone_of(iana_id: &str) -> Result<TimeZone, CoreError> {
    TimeZone::get(iana_id).map_err(|error| {
        if jiff::tz::db().is_definitively_empty() {
            tracing::error!("can't read the time zone database: {error}");
            CoreError::NoTimeZoneDatabase
        } else {
            CoreError::UnknownTimeZone {
                iana_id: iana_id.to_owned(),
            }
        }
    })
}
