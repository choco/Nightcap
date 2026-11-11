//! The schedule math behind reconciliation (ADR-0004): what the schedule says at a moment, when
//! it next changes, and how macOS fades it (ADR-0021).

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, civil};
use nightcap_protocol::{OnOff, Schedule, TimeOfDay};

use crate::TintPoint;
use crate::tint;

/// The schedule placed in time, from what the shell reports.
pub(crate) struct Timeline {
    /// None until the shell first reports Night Shift's status, and while its custom hours
    /// aren't times of day.
    pub schedule: Option<CheckedSchedule>,
    pub sun_times: SunTimes,
    pub zone: TimeZone,
}

/// Night Shift's schedule with custom hours as times on the clock, checked once where they
/// come in, so the schedule math never meets one out of range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckedSchedule {
    Off,
    SunsetToSunrise,
    Custom { from: civil::Time, to: civil::Time },
}

impl TryFrom<Schedule> for CheckedSchedule {
    type Error = OutOfRange;

    fn try_from(schedule: Schedule) -> Result<Self, Self::Error> {
        Ok(match schedule {
            Schedule::Off => Self::Off,
            Schedule::SunsetToSunrise => Self::SunsetToSunrise,
            Schedule::Custom { from, to } => Self::Custom {
                from: clock_time(from)?,
                to: clock_time(to)?,
            },
        })
    }
}

impl From<CheckedSchedule> for Schedule {
    fn from(schedule: CheckedSchedule) -> Self {
        match schedule {
            CheckedSchedule::Off => Self::Off,
            CheckedSchedule::SunsetToSunrise => Self::SunsetToSunrise,
            CheckedSchedule::Custom { from, to } => Self::Custom {
                from: time_of_day(from),
                to: time_of_day(to),
            },
        }
    }
}

/// Custom hours with an end that isn't a time of day.
#[derive(Debug, thiserror::Error)]
#[error("custom hours are times of day, from 00:00 to 23:59, not {0}")]
pub(crate) struct OutOfRange(pub(crate) TimeOfDay);

/// One end of custom hours on the clock.
fn clock_time(time: TimeOfDay) -> Result<civil::Time, OutOfRange> {
    let hour = i8::try_from(time.hour).ok();
    let minute = i8::try_from(time.minute).ok();
    hour.zip(minute)
        .and_then(|(hour, minute)| civil::Time::new(hour, minute, 0, 0).ok())
        .ok_or(OutOfRange(time))
}

/// A time on the clock, to the minute, as custom hours write it.
pub(crate) fn time_of_day(time: civil::Time) -> TimeOfDay {
    // jiff's hours and minutes are never negative, so these are their values.
    TimeOfDay {
        hour: time.hour().unsigned_abs(),
        minute: time.minute().unsigned_abs(),
    }
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
    pub(crate) fn scheduled_state(&self, at: Timestamp) -> Option<OnOff> {
        match self.schedule? {
            CheckedSchedule::Off => Some(OnOff::Off),
            CheckedSchedule::SunsetToSunrise => self
                .sun_times
                .changes
                .iter()
                .rev()
                .find(|&&(change, _)| change <= at)
                .map(|&(_, on)| OnOff::from(on)),
            CheckedSchedule::Custom { from, to } => {
                let local = self.zone.to_datetime(at).time();
                Some(OnOff::from(if from <= to {
                    from <= local && local < to
                } else {
                    from <= local || local < to
                }))
            }
        }
    }

    /// The first schedule transition after `after`.
    pub(crate) fn next_transition(&self, after: Timestamp) -> Option<Timestamp> {
        self.transitions_around(after)
            .into_iter()
            .filter(|&transition| transition > after)
            .min()
    }

    /// The last schedule transition at or before `at`.
    pub(crate) fn last_transition(&self, at: Timestamp) -> Option<Timestamp> {
        self.transitions_around(at)
            .into_iter()
            .filter(|&transition| transition <= at)
            .max()
    }

    /// The schedule transitions on either side of `at`: every sun time, or custom hours' starts
    /// and ends from the day before `at` to the day after, since each day has one of each.
    #[expect(
        clippy::expect_used,
        reason = "schedules stay millennia from jiff's limits"
    )]
    fn transitions_around(&self, at: Timestamp) -> Vec<Timestamp> {
        match self.schedule {
            None | Some(CheckedSchedule::Off) => Vec::new(),
            Some(CheckedSchedule::SunsetToSunrise) => self
                .sun_times
                .changes
                .iter()
                .map(|&(change, _)| change)
                .collect(),
            Some(CheckedSchedule::Custom { from, to }) => {
                let today = self.zone.to_datetime(at).date();
                [today.yesterday(), Ok(today), today.tomorrow()]
                    .into_iter()
                    .map(|day| day.expect("schedules stay far from jiff's limits"))
                    .flat_map(|day| [from, to].map(|moment| day.to_datetime(moment)))
                    .map(|local| {
                        self.zone
                            .to_timestamp(local)
                            .expect("schedules stay far from jiff's limits")
                    })
                    .collect()
            }
        }
    }

    /// What's still to come at `at` of macOS's fades while the schedule has Night Shift on: over
    /// the half hour after sunset and before sunrise, or the two minutes after custom hours start
    /// and before they end, as `-[NightModeControl setMode:]` sets them. None between them, and
    /// while the schedule has Night Shift off.
    pub(crate) fn fade(&self, at: Timestamp) -> Option<Vec<TintPoint>> {
        let length = match self.schedule? {
            CheckedSchedule::Off => return None,
            CheckedSchedule::SunsetToSunrise => SignedDuration::from_mins(30),
            CheckedSchedule::Custom { .. } => SignedDuration::from_mins(2),
        };
        if self.scheduled_state(at)? == OnOff::Off {
            return None;
        }
        tint::schedule_fade(
            self.last_transition(at)?,
            self.next_transition(at)?,
            length,
            at,
        )
    }

    /// When the schedule next turns Night Shift away from `state` after `after`: sunrise ends
    /// an on, sunset an off. None without a schedule.
    pub(crate) fn next_turn(&self, after: Timestamp, state: OnOff) -> Option<Timestamp> {
        let first = self.next_transition(after)?;
        if self.scheduled_state(first) != Some(state) {
            return Some(first);
        }
        self.next_transition(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(instant: &str) -> Timestamp {
        instant.parse().unwrap()
    }

    fn custom(from: (i8, i8), to: (i8, i8)) -> Timeline {
        Timeline {
            schedule: Some(CheckedSchedule::Custom {
                from: civil::time(from.0, from.1, 0, 0),
                to: civil::time(to.0, to.1, 0, 0),
            }),
            sun_times: SunTimes::default(),
            zone: TimeZone::get("Europe/Zurich").unwrap(),
        }
    }

    /// Sunset to sunrise in Zurich, as CoreBrightness reported it in the early hours of
    /// 16 October 2026.
    fn sunset_to_sunrise() -> Timeline {
        Timeline {
            schedule: Some(CheckedSchedule::SunsetToSunrise),
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
    fn custom_hours_that_arent_times_of_day_are_out_of_range_from_00_00_to_23_59() {
        let error = CheckedSchedule::try_from(Schedule::Custom {
            from: TimeOfDay {
                hour: 22,
                minute: 0,
            },
            to: TimeOfDay {
                hour: 24,
                minute: 5,
            },
        })
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "custom hours are times of day, from 00:00 to 23:59, not 24:05"
        );
    }

    #[test]
    fn custom_hours_across_midnight_are_on_from_their_start_until_their_end() {
        let timeline = custom((22, 0), (7, 0));

        assert_eq!(
            timeline.scheduled_state(at("2026-10-16T21:59:00+02:00")),
            Some(OnOff::Off)
        );
        assert_eq!(
            timeline.scheduled_state(at("2026-10-16T22:00:00+02:00")),
            Some(OnOff::On)
        );
        assert_eq!(
            timeline.scheduled_state(at("2026-10-17T06:59:00+02:00")),
            Some(OnOff::On)
        );
        assert_eq!(
            timeline.scheduled_state(at("2026-10-17T07:00:00+02:00")),
            Some(OnOff::Off)
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

        assert_eq!(timeline.scheduled_state(night), Some(OnOff::On));
        assert_eq!(
            timeline.next_transition(night),
            Some(at("2026-10-16T05:50:47Z"))
        );
        assert_eq!(timeline.scheduled_state(day), Some(OnOff::Off));
        assert_eq!(
            timeline.next_transition(day),
            Some(at("2026-10-16T16:31:32Z"))
        );
    }
}
