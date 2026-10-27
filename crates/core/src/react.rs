//! What each event the shell reports means for the state, before the turn's pipeline runs
//! (ADR-0002).

use std::time::SystemTime;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::schedule::SunTimes;
use crate::{Core, Effect, Event, State};

impl Core {
    pub(crate) fn react(&self, state: &mut State, event: Event) -> Vec<Effect> {
        match event {
            Event::Launched => self.launched(state),
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
                state.onboarding_finished(websites);
                Vec::new()
            }
            Event::TourStarted {
                sample,
                site,
                example,
            } => {
                state.tour_started(&sample, site.as_ref(), &example);
                Vec::new()
            }
            Event::TourEnded => {
                state.tour_ended(self.clock.now());
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
