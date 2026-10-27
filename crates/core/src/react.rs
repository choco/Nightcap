//! What each event the shell reports means for the state, before the turn's pipeline runs
//! (ADR-0002).

use std::time::SystemTime;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::NightShift;

use crate::onboarding::ONBOARDING;
use crate::schedule::SunTimes;
use crate::{Core, Effect, Event, State, tour};

impl Core {
    pub(crate) fn react(&self, state: &mut State, event: Event) -> Vec<Effect> {
        match event {
            Event::Launched => {
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
                if state.onboarding < ONBOARDING {
                    // The config can answer website checking and Night Shift; opening at login
                    // it can't.
                    let night_shift = state.onboarding_night_shift();
                    let night_shift_answered =
                        !night_shift.ask_strength && !night_shift.ask_schedule;
                    if state.facts.config_websites.is_some()
                        && night_shift_answered
                        && !state.grants.opens_itself_at_login
                    {
                        state.onboarding = ONBOARDING;
                    } else {
                        effects.push(Effect::ShowOnboarding {
                            completed: state.onboarding,
                        });
                    }
                }
                if self.updates_itself() {
                    effects.push(Effect::StartUpdater);
                }
                state.apply_config_night_shift(&mut effects);
                effects
            }
            Event::AppActivated {
                bundle_id,
                name,
                is_browser,
            } => {
                state.app_activated(bundle_id, name, is_browser);
                Vec::new()
            }
            Event::HostChanged { generation, host } => {
                state.host_changed(generation, host);
                Vec::new()
            }
            Event::LoginItemChanged { item } => {
                state.grants.login_item_changed(item);
                Vec::new()
            }
            Event::PermissionsChanged {
                accessibility,
                notifications,
            } => {
                state
                    .grants
                    .permissions_changed(accessibility, notifications);
                Vec::new()
            }
            Event::StatusChanged {
                enabled,
                schedule,
                sun_schedule_set_aside,
            } => self.status_changed(state, enabled, schedule, sun_schedule_set_aside),
            Event::StrengthChanged { value } => {
                state.strength = Some(value);
                Vec::new()
            }
            Event::OnboardingFinished { websites } => {
                state.onboarding = ONBOARDING;
                if state.facts.config_websites.is_none() {
                    tracing::info!("onboarding finished; website checking {websites}");
                    state.facts.websites = websites;
                }
                Vec::new()
            }
            Event::TourStarted {
                sample,
                site,
                example,
            } => {
                let stands_in = state.facts.night_shift == NightShift::Off;
                if !state.places.start_tour(stands_in) {
                    tracing::warn!("a tour started while one runs; keeping the first");
                    return Vec::new();
                }
                state.tour = Some(tour::Tour::new(&sample, site.as_ref(), &example));
                tracing::info!(
                    "tour started{}",
                    if stands_in {
                        ", standing in for the schedule with Night Shift on"
                    } else {
                        ""
                    }
                );
                Vec::new()
            }
            Event::TourEnded => {
                let now = self.clock.now();
                state.places.end_tour(now);
                state.tour = None;
                Vec::new()
            }
            Event::FixChosen { fix } => self.run_fix(state, fix),
            // `handle` runs it through `choose`.
            Event::CommandChosen { .. } => Vec::new(),
            Event::TimerFired => {
                state.reconciling.timer_fired();
                Vec::new()
            }
            Event::SunTimesChanged { sunrises, sunsets } => {
                let timestamps = |times: Vec<SystemTime>| {
                    times
                        .into_iter()
                        .map(Timestamp::try_from)
                        .collect::<Result<Vec<_>, _>>()
                };
                let sun_times = match (timestamps(sunrises), timestamps(sunsets)) {
                    (Ok(sunrises), Ok(sunsets)) => SunTimes::new(&sunrises, &sunsets),
                    // A shell bug, logged rather than a panic, which would stop the app
                    // (ADR-0002).
                    (Err(error), _) | (_, Err(error)) => {
                        tracing::error!("the shell reported a sun time out of range: {error}");
                        SunTimes::default()
                    }
                };
                state.facts.timeline.sun_times = sun_times;
                Vec::new()
            }
            Event::TimeZoneChanged { iana_id } => {
                match TimeZone::get(&iana_id) {
                    Ok(zone) => {
                        tracing::info!("time zone changed to {iana_id}");
                        state.facts.timeline.zone = zone;
                    }
                    Err(error) => {
                        tracing::error!("keeping the old time zone: {iana_id} is unknown: {error}");
                    }
                }
                Vec::new()
            }
            Event::SessionChanged { active } => {
                state.reconciling.session_changed(active);
                Vec::new()
            }
            Event::Woke => {
                state.reconciling.woke(self.clock.now());
                Vec::new()
            }
            Event::Terminating => {
                // Nightcap can't tell a restart from being removed, so it never leaves a force
                // behind (ADR-0008), nor Night Shift on for a tour (ADR-0016).
                let now = self.clock.now();
                state.places.release(now);
                state.tour = None;
                if let Some(socket) = &self.socket
                    && let Err(error) = std::fs::remove_file(socket)
                {
                    tracing::warn!("couldn't remove {}: {error}", socket.display());
                }
                Vec::new()
            }
        }
    }
}
