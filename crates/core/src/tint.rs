//! The tint, how much of the strength Night Shift shows, and the fades it moves along as Night
//! Shift switches (ADR-0021).

use jiff::{SignedDuration, Timestamp};

/// A point a fade passes: the tint at a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Record)]
pub struct TintPoint {
    pub at: Timestamp,
    /// From 0, none, to 100, the whole strength.
    pub tint: u8,
}

/// What's still to come at `at` of the schedule's fades over a stretch it holds Night Shift on,
/// from `on` to `off`: up over `length` after `on`, and down over as long before `off`, meeting
/// halfway where they'd overlap. None between them, where the tint stays whole.
pub(crate) fn schedule_fade(
    on: Timestamp,
    off: Timestamp,
    length: SignedDuration,
    at: Timestamp,
) -> Option<Vec<TintPoint>> {
    let halfway = on + off.duration_since(on) / 2;
    let rise_ends = (on + length).min(halfway);
    let fall_starts = (off - length).max(halfway);
    let start = TintPoint { at: on, tint: 0 };
    let top = TintPoint {
        at: rise_ends,
        tint: share(rise_ends.duration_since(on), length),
    };
    let end = TintPoint { at: off, tint: 0 };
    if at < rise_ends {
        // Fades that meet halfway leave the tint no time at the top.
        Some(if rise_ends == fall_starts {
            vec![start, top, end]
        } else {
            vec![start, top]
        })
    } else if at >= fall_starts {
        Some(vec![
            TintPoint {
                at: fall_starts,
                ..top
            },
            end,
        ])
    } else {
        None
    }
}

/// How much of `whole` `part` is, from 0 to 100.
#[expect(clippy::expect_used, reason = "a share clamped to 0 to 100 fits a u8")]
fn share(part: SignedDuration, whole: SignedDuration) -> u8 {
    let hundredths = (part.as_millis() * 100 / whole.as_millis()).clamp(0, 100);
    u8::try_from(hundredths).expect("0 to 100 fits a u8")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(instant: &str) -> Timestamp {
        instant.parse().unwrap()
    }

    fn points(fade: &[(&str, u8)]) -> Vec<TintPoint> {
        fade.iter()
            .map(|&(instant, tint)| TintPoint {
                at: at(instant),
                tint,
            })
            .collect()
    }

    #[test]
    fn fades_that_would_overlap_meet_halfway() {
        let on = at("2026-10-16T22:00:00Z");
        let off = at("2026-10-16T22:03:00Z");

        let rising = schedule_fade(
            on,
            off,
            SignedDuration::from_mins(2),
            at("2026-10-16T22:01:00Z"),
        );
        let falling = schedule_fade(
            on,
            off,
            SignedDuration::from_mins(2),
            at("2026-10-16T22:02:00Z"),
        );

        assert_eq!(
            rising,
            Some(points(&[
                ("2026-10-16T22:00:00Z", 0),
                ("2026-10-16T22:01:30Z", 75),
                ("2026-10-16T22:03:00Z", 0)
            ]))
        );
        assert_eq!(
            falling,
            Some(points(&[
                ("2026-10-16T22:01:30Z", 75),
                ("2026-10-16T22:03:00Z", 0)
            ]))
        );
    }
}
