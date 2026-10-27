#![cfg(debug_assertions)]
//! Following the schedule: its transitions, the wakeups for them, and the sun times (ADR-0004).

mod common;

use std::time::Duration;

use nightcap_core::{Effect, Event, Scenario};
use nightcap_protocol::{NightShift, OnOff, Schedule, TimeOfDay};

use common::{at, custom_hours, late, off_by_the_schedule, report_zurich_sun, reported, status};

#[test]
fn custom_hours_the_shell_reports_out_of_range_leave_the_schedule_unknown() {
    let staged = late().then(reported(true, custom_hours())).stage().unwrap();
    let hours = Schedule::Custom {
        from: TimeOfDay {
            hour: 25,
            minute: 0,
        },
        to: TimeOfDay {
            hour: 7,
            minute: 60,
        },
    };

    staged.core().handle(reported(true, hours));
    staged.set_now(at("2026-10-17T07:30:00+02:00"));
    staged.core().handle(Event::TimerFired);

    let status = status(&staged);
    assert_eq!(status.night_shift, NightShift::On);
    assert_eq!(status.schedule, None);
    assert_eq!(status.next_transition, None);
    assert_eq!(staged.core().menu().night_shift.reason, None);
}

#[test]
fn the_status_reports_the_schedule_its_scheduled_state_and_the_next_transition() {
    let staged = off_by_the_schedule().stage().unwrap();

    let status = status(&staged);
    assert_eq!(status.schedule, Some(custom_hours()));
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(
        status.next_transition,
        Some(at("2026-10-16T22:00:00+02:00"))
    );
}

#[test]
fn the_core_asks_to_be_woken_at_the_next_schedule_transition() {
    let staged = Scenario::at(at("2026-10-16T20:00:00+02:00"))
        .stage()
        .unwrap();

    let effects = staged.core().handle(reported(false, custom_hours()));

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+02:00").into()
        }]
    );
}

#[test]
fn a_timer_that_fires_before_its_transition_is_set_again() {
    let staged = off_by_the_schedule().stage().unwrap();

    staged.set_now(at("2026-10-16T21:59:59+02:00"));

    assert_eq!(
        staged.core().handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+02:00").into()
        }]
    );
}

#[test]
fn a_time_zone_change_moves_the_custom_hours() {
    let staged = off_by_the_schedule().stage().unwrap();

    let effects = staged.core().handle(Event::TimeZoneChanged {
        iana_id: "Europe/London".to_owned(),
    });

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+01:00").into()
        }]
    );
}

#[test]
fn sunset_to_sunrise_follows_the_reported_sun_times() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();

    report_zurich_sun(&staged);
    let effects = staged
        .core()
        .handle(reported(true, Schedule::SunsetToSunrise));

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T05:50:47Z").into()
        }]
    );
    let status = status(&staged);
    assert_eq!(status.scheduled_state, Some(OnOff::On));
    assert_eq!(status.layers, Vec::new());
}

#[test]
fn sun_times_the_shell_reports_out_of_range_count_as_none() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    report_zurich_sun(&staged);
    staged
        .core()
        .handle(reported(true, Schedule::SunsetToSunrise));

    let past_jiff = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(400_000_000_000);
    staged.core().handle(Event::SunTimesChanged {
        sunrises: vec![past_jiff],
        sunsets: Vec::new(),
    });

    let status = status(&staged);
    assert_eq!(status.scheduled_state, None);
    assert_eq!(status.next_transition, None);
}

#[test]
fn until_corebrightness_has_the_sun_times_the_shell_reads_them_again_each_minute() {
    let staged = Scenario::at(at("2026-10-16T01:40:20+02:00"))
        .stage()
        .unwrap();

    let effects = staged
        .core()
        .handle(reported(true, Schedule::SunsetToSunrise));
    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T01:41:00+02:00").into()
        }]
    );
    staged.set_now(at("2026-10-16T01:40:50+02:00"));
    assert_eq!(staged.core().handle(Event::Woke), Vec::new());

    staged.set_now(at("2026-10-16T01:41:00+02:00"));
    assert_eq!(
        staged.core().handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T01:42:00+02:00").into()
        }]
    );

    staged.set_now(at("2026-10-16T01:42:00+02:00"));
    report_zurich_sun(&staged);
    assert_eq!(
        staged.core().handle(Event::TimerFired),
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T05:50:47Z").into()
        }]
    );
    assert_eq!(status(&staged).scheduled_state, Some(OnOff::On));
}

#[test]
fn turning_the_schedule_off_cancels_the_wakeup() {
    let staged = off_by_the_schedule().stage().unwrap();

    let effects = staged.core().handle(reported(false, Schedule::Off));

    assert_eq!(effects, vec![Effect::CancelWakeup]);
    let status = status(&staged);
    assert_eq!(status.scheduled_state, Some(OnOff::Off));
    assert_eq!(status.next_transition, None);
}
