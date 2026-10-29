#![cfg(debug_assertions)]
//! Following the schedule: its transitions, the wakeups for them, and the sun times (ADR-0004).

mod common;

use std::time::Duration;

use nightcap_core::{Effect, Fact, Scenario};
use nightcap_protocol::{NightShift, OnOff, Schedule, TimeOfDay};

use common::{
    at, custom_hours, late, off_by_the_schedule, on_by_the_schedule, report_zurich_sun, reported,
    resettable, written,
};

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

    staged.report(reported(true, hours));
    staged.set_now(at("2026-10-17T07:30:00+02:00"));
    staged.report(Fact::TimerFired);

    assert_eq!(staged.core().snapshot().night_shift().state, NightShift::On);
    assert_eq!(staged.core().snapshot().night_shift().schedule, None);
    assert_eq!(
        staged.core().snapshot().night_shift().next_transition(),
        None
    );
    assert_eq!(
        staged.core().snapshot().front().decision.reasons,
        Vec::new()
    );
}

#[test]
fn the_core_asks_to_be_woken_at_the_next_schedule_transition() {
    let staged = Scenario::at(at("2026-10-16T20:00:00+02:00"))
        .stage()
        .unwrap();

    let effects = staged.report(reported(false, custom_hours())).effects;

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
        staged.report(Fact::TimerFired).effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T22:00:00+02:00").into()
        }]
    );
}

#[test]
fn a_transition_only_the_schedule_decides_is_left_to_macos() {
    let staged = off_by_the_schedule().stage().unwrap();

    staged.set_now(at("2026-10-16T22:00:01+02:00"));
    let at_the_transition = staged.report(Fact::TimerFired).effects;
    let once_macos_performs_it = staged.report(reported(true, custom_hours())).effects;

    assert_eq!(
        at_the_transition,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
    assert_eq!(once_macos_performs_it, Vec::new());
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_transition_missed_asleep_is_left_to_macos_however_late() {
    let staged = off_by_the_schedule().stage().unwrap();

    staged.set_now(at("2026-10-16T23:00:00+02:00"));
    let on_waking = staged.report(Fact::Woke).effects;

    assert_eq!(
        on_waking,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-17T07:00:00+02:00").into()
        }]
    );
}

#[test]
fn a_rule_ending_at_a_transition_leaves_the_transition_to_macos() {
    let staged = on_by_the_schedule().stage().unwrap();
    // Off from Control Center: a rule until the schedule turns Night Shift back on.
    staged.report(written(false, custom_hours(), "2026-10-16T23:00:30+02:00"));

    staged.set_now(at("2026-10-17T22:00:01+02:00"));
    let at_the_transition = staged.report(Fact::TimerFired).effects;

    assert_eq!(
        at_the_transition,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-18T07:00:00+02:00").into()
        }]
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn a_time_zone_change_moves_the_custom_hours() {
    let staged = off_by_the_schedule().stage().unwrap();

    let effects = staged
        .report(Fact::TimeZoneChanged {
            iana_id: "Europe/London".to_owned(),
        })
        .effects;

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
        .report(reported(true, Schedule::SunsetToSunrise))
        .effects;

    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T05:50:47Z").into()
        }]
    );
    assert_eq!(
        staged.core().snapshot().night_shift().scheduled,
        Some(OnOff::On)
    );
    assert_eq!(resettable(&staged), Vec::new());
}

#[test]
fn sun_times_the_shell_reports_out_of_range_count_as_none() {
    let staged = Scenario::at(at("2026-10-16T01:40:00+02:00"))
        .stage()
        .unwrap();
    report_zurich_sun(&staged);
    staged.report(reported(true, Schedule::SunsetToSunrise));

    let past_jiff = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(400_000_000_000);
    staged.report(Fact::SunTimesChanged {
        sunrises: vec![past_jiff],
        sunsets: Vec::new(),
    });

    assert_eq!(staged.core().snapshot().night_shift().scheduled, None);
    assert_eq!(
        staged.core().snapshot().night_shift().next_transition(),
        None
    );
}

#[test]
fn until_corebrightness_has_the_sun_times_the_shell_reads_them_again_each_minute() {
    let staged = Scenario::at(at("2026-10-16T01:40:20+02:00"))
        .stage()
        .unwrap();

    let effects = staged
        .report(reported(true, Schedule::SunsetToSunrise))
        .effects;
    assert_eq!(
        effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T01:41:00+02:00").into()
        }]
    );
    staged.set_now(at("2026-10-16T01:40:50+02:00"));
    assert_eq!(staged.report(Fact::Woke).effects, Vec::new());

    staged.set_now(at("2026-10-16T01:41:00+02:00"));
    assert_eq!(
        staged.report(Fact::TimerFired).effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T01:42:00+02:00").into()
        }]
    );

    staged.set_now(at("2026-10-16T01:42:00+02:00"));
    report_zurich_sun(&staged);
    assert_eq!(
        staged.report(Fact::TimerFired).effects,
        vec![Effect::ScheduleWakeup {
            at: at("2026-10-16T05:50:47Z").into()
        }]
    );
    assert_eq!(
        staged.core().snapshot().night_shift().scheduled,
        Some(OnOff::On)
    );
}

#[test]
fn turning_the_schedule_off_cancels_the_wakeup() {
    let staged = off_by_the_schedule().stage().unwrap();

    let effects = staged.report(reported(false, Schedule::Off)).effects;

    assert_eq!(effects, vec![Effect::CancelWakeup]);
    assert_eq!(
        staged.core().snapshot().night_shift().scheduled,
        Some(OnOff::Off)
    );
    assert_eq!(
        staged.core().snapshot().night_shift().next_transition(),
        None
    );
}
