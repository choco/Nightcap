//! The schedule math behind reconciliation (ADR-0004): what the schedule says at a moment, and
//! when it next changes.

use jiff::tz::TimeZone;
use jiff::{Timestamp, civil};
use nightcap_protocol::{Schedule, TimeOfDay};

/// The schedule placed in time, from what the shell reports.
pub(crate) struct Timeline {
    /// None until the shell first reports Night Shift's status.
    pub schedule: Option<Schedule>,
    pub sun_times: SunTimes,
    pub zone: TimeZone,
}

/// Sunrises and sunsets as CoreBrightness reports them, from the day before to the day after.
#[derive(Default)]
pub(crate) struct SunTimes {
    /// Each sunrise or sunset, in order, with whether Night Shift is scheduled on after it.
    changes: Vec<(Timestamp, bool)>,
}

impl SunTimes {
    pub(crate) fn new(sunrises: &[Timestamp], sunsets: &[Timestamp]) -> Self {
        let mut changes: Vec<_> = sunrises
            .iter()
            .map(|&sunrise| (sunrise, false))
            .chain(sunsets.iter().map(|&sunset| (sunset, true)))
            .collect();
        changes.sort();
        Self { changes }
    }
}

impl Timeline {
    /// Whether the schedule has Night Shift on at `at`, or None while the schedule, or the sun
    /// times it needs, aren't known.
    pub(crate) fn scheduled_state(&self, at: Timestamp) -> Option<bool> {
        match self.schedule? {
            Schedule::Off => Some(false),
            Schedule::SunsetToSunrise => self
                .sun_times
                .changes
                .iter()
                .rev()
                .find(|&&(change, _)| change <= at)
                .map(|&(_, on)| on),
            Schedule::Custom { from, to } => {
                let (from, to) = (time(from), time(to));
                let local = self.zone.to_datetime(at).time();
                Some(if from <= to {
                    from <= local && local < to
                } else {
                    from <= local || local < to
                })
            }
        }
    }

    /// The first schedule transition after `after`.
    #[expect(
        clippy::expect_used,
        reason = "schedules stay millennia from jiff's limits"
    )]
    pub(crate) fn next_transition(&self, after: Timestamp) -> Option<Timestamp> {
        match self.schedule? {
            Schedule::Off => None,
            Schedule::SunsetToSunrise => self
                .sun_times
                .changes
                .iter()
                .map(|&(change, _)| change)
                .find(|&change| change > after),
            Schedule::Custom { from, to } => {
                let today = self.zone.to_datetime(after).date();
                // Tomorrow's start and end both come after anything today, so two days suffice.
                let tomorrow = today
                    .tomorrow()
                    .expect("schedules stay far from jiff's limits");
                [today, tomorrow]
                    .into_iter()
                    .flat_map(|day| [from, to].map(|moment| day.to_datetime(time(moment))))
                    .map(|local| {
                        self.zone
                            .to_timestamp(local)
                            .expect("schedules stay far from jiff's limits")
                    })
                    .filter(|&transition| transition > after)
                    .min()
            }
        }
    }
}

#[expect(
    clippy::as_conversions,
    clippy::expect_used,
    reason = "the shell reports hours and minutes in range"
)]
fn time(time: TimeOfDay) -> civil::Time {
    civil::Time::new(time.hour as i8, time.minute as i8, 0, 0)
        .expect("the shell reports hours and minutes in range")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(instant: &str) -> Timestamp {
        instant.parse().unwrap()
    }

    fn custom(from: (u8, u8), to: (u8, u8)) -> Timeline {
        Timeline {
            schedule: Some(Schedule::Custom {
                from: TimeOfDay {
                    hour: from.0,
                    minute: from.1,
                },
                to: TimeOfDay {
                    hour: to.0,
                    minute: to.1,
                },
            }),
            sun_times: SunTimes::default(),
            zone: TimeZone::get("Europe/Zurich").unwrap(),
        }
    }

    /// Sunset to sunrise in Zurich, as CoreBrightness reported it in the early hours of
    /// 16 October 2026.
    fn sunset_to_sunrise() -> Timeline {
        Timeline {
            schedule: Some(Schedule::SunsetToSunrise),
            sun_times: SunTimes::new(
                &[
                    at("2026-10-15T05:49:20Z"),
                    at("2026-10-16T05:50:47Z"),
                    at("2026-10-17T05:52:14Z"),
                ],
                &[
                    at("2026-10-15T16:33:25Z"),
                    at("2026-10-16T16:31:32Z"),
                    at("2026-10-17T16:29:41Z"),
                ],
            ),
            zone: TimeZone::get("Europe/Zurich").unwrap(),
        }
    }

    #[test]
    fn custom_hours_across_midnight_are_on_from_their_start_until_their_end() {
        let timeline = custom((22, 0), (7, 0));

        assert_eq!(
            timeline.scheduled_state(at("2026-10-16T21:59:00+02:00")),
            Some(false)
        );
        assert_eq!(
            timeline.scheduled_state(at("2026-10-16T22:00:00+02:00")),
            Some(true)
        );
        assert_eq!(
            timeline.scheduled_state(at("2026-10-17T06:59:00+02:00")),
            Some(true)
        );
        assert_eq!(
            timeline.scheduled_state(at("2026-10-17T07:00:00+02:00")),
            Some(false)
        );
    }

    #[test]
    fn the_next_transition_of_custom_hours_is_their_next_start_or_end() {
        let timeline = custom((22, 0), (7, 0));

        assert_eq!(
            timeline.next_transition(at("2026-10-16T20:00:00+02:00")),
            Some(at("2026-10-16T22:00:00+02:00"))
        );
        assert_eq!(
            timeline.next_transition(at("2026-10-16T22:00:00+02:00")),
            Some(at("2026-10-17T07:00:00+02:00"))
        );
        assert_eq!(
            timeline.next_transition(at("2026-10-17T03:00:00+02:00")),
            Some(at("2026-10-17T07:00:00+02:00"))
        );
    }

    #[test]
    fn custom_hours_follow_local_time_across_a_daylight_saving_change() {
        let timeline = custom((22, 0), (7, 0));

        // Clocks go back an hour at 03:00 that night, so the night lasts ten hours, not nine.
        assert_eq!(
            timeline.next_transition(at("2026-10-24T22:00:00+02:00")),
            Some(at("2026-10-25T07:00:00+01:00"))
        );
    }

    #[test]
    fn sunset_to_sunrise_follows_the_reported_sun_times() {
        let timeline = sunset_to_sunrise();
        let night = at("2026-10-15T23:40:00Z");
        let day = at("2026-10-16T12:00:00Z");

        assert_eq!(timeline.scheduled_state(night), Some(true));
        assert_eq!(
            timeline.next_transition(night),
            Some(at("2026-10-16T05:50:47Z"))
        );
        assert_eq!(timeline.scheduled_state(day), Some(false));
        assert_eq!(
            timeline.next_transition(day),
            Some(at("2026-10-16T16:31:32Z"))
        );
    }
}
