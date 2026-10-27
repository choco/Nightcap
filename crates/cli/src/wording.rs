//! The CLI's own words for what Nightcap reports, which sends facts alone: its decisions and its
//! rules.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{Decision, OnOff, Place, Reason, Rule, RuleSource};

/// What Nightcap decided and why, as it reads after "Night Shift", with times in `zone`.
pub(crate) fn decision(decision: &Decision, zone: &TimeZone) -> String {
    let until = |end: Option<Timestamp>, state: OnOff| match end {
        Some(end) => format!(" until {}", end.to_zoned(zone.clone()).strftime("%H:%M")),
        None => format!(" until you turn it {}", state.other()),
    };
    let Some(top) = decision.reasons.first() else {
        return "left alone".to_owned();
    };
    let at = match &decision.place {
        Place::Everywhere => String::new(),
        place => format!(" at {place}"),
    };
    let mut description = match top {
        Reason::Timer(layer) | Reason::Hold(layer) => {
            format!("{}{at}{}", layer.state, until(layer.until, layer.state))
        }
        Reason::Rule(rule) => format!(
            "{} by the {} rule for {}",
            rule.says,
            source(rule.source),
            rule.identity
        ),
        Reason::Tour => "on for the tour".to_owned(),
        Reason::Schedule { .. } => "following the schedule".to_owned(),
    };
    let snoozed = decision
        .reasons
        .iter()
        .skip(1)
        .find_map(|reason| match reason {
            Reason::Rule(rule) if Some(rule.says) != decision.state => Some(rule),
            _ => None,
        });
    if let Some(rule) = snoozed {
        description.push_str(&format!(", its {} rule snoozed", source(rule.source)));
    }
    description
}

/// `app:com.figma.Desktop (config rule, off)`.
pub(crate) fn rule(rule: &Rule) -> String {
    format!(
        "{} ({} rule, {})",
        rule.identity,
        source(rule.source),
        rule.says
    )
}

fn source(source: RuleSource) -> &'static str {
    match source {
        RuleSource::Config => "config",
        RuleSource::AdHoc => "ad-hoc",
        RuleSource::Tour => "tour",
    }
}

#[cfg(test)]
mod tests {
    use nightcap_protocol::{Layer, RuleIdentity};

    use super::*;

    #[test]
    fn decisions_read_after_night_shift() {
        let zone = TimeZone::get("Europe/Zurich").unwrap();
        let at = |instant: &str| instant.parse::<Timestamp>().unwrap();
        let figma_app = || Place::App("com.figma.Desktop".parse().unwrap());
        let figma = Rule {
            identity: RuleIdentity::App("com.figma.Desktop".parse().unwrap()),
            source: RuleSource::Config,
            says: OnOff::Off,
        };
        let layer = |state, until| Layer {
            state,
            until,
            with: None,
        };
        let described = |state, place, reasons| {
            decision(
                &Decision {
                    state: Some(state),
                    place,
                    reasons,
                },
                &zone,
            )
        };

        assert_eq!(
            described(
                OnOff::Off,
                Place::Everywhere,
                vec![
                    Reason::Timer(layer(OnOff::Off, Some(at("2026-10-16T22:00:00Z")))),
                    Reason::Schedule {
                        state: OnOff::On,
                        until: None
                    },
                ]
            ),
            "off until 00:00"
        );
        assert_eq!(
            described(
                OnOff::Off,
                Place::Everywhere,
                vec![Reason::Hold(layer(OnOff::Off, None))]
            ),
            "off until you turn it on"
        );
        assert_eq!(
            described(
                OnOff::On,
                figma_app(),
                vec![
                    Reason::Hold(layer(OnOff::On, Some(at("2026-10-17T05:00:00Z")))),
                    Reason::Rule(figma.clone()),
                ]
            ),
            "on at app:com.figma.Desktop until 07:00, its config rule snoozed"
        );
        assert_eq!(
            described(OnOff::Off, figma_app(), vec![Reason::Rule(figma)]),
            "off by the config rule for app:com.figma.Desktop"
        );
        assert_eq!(
            described(
                OnOff::On,
                Place::Domain("figma.com".parse().unwrap()),
                vec![Reason::Rule(Rule {
                    identity: RuleIdentity::Domain("figma.com".parse().unwrap()),
                    source: RuleSource::AdHoc,
                    says: OnOff::On,
                })]
            ),
            "on by the ad-hoc rule for domain:figma.com"
        );
        assert_eq!(
            described(OnOff::On, Place::Everywhere, vec![Reason::Tour]),
            "on for the tour"
        );
        assert_eq!(
            described(
                OnOff::On,
                Place::Everywhere,
                vec![Reason::Schedule {
                    state: OnOff::On,
                    until: None
                }]
            ),
            "following the schedule"
        );
        assert_eq!(
            decision(
                &Decision {
                    state: None,
                    place: Place::Everywhere,
                    reasons: Vec::new(),
                },
                &zone
            ),
            "left alone"
        );
    }

    #[test]
    fn a_rule_reads_with_its_source_and_what_it_says() {
        let worded = |identity: &str, source, says| {
            rule(&Rule {
                identity: identity.parse().unwrap(),
                source,
                says,
            })
        };

        assert_eq!(
            worded("app:com.figma.Desktop", RuleSource::Config, OnOff::Off),
            "app:com.figma.Desktop (config rule, off)"
        );
        assert_eq!(
            worded("domain:figma.com", RuleSource::AdHoc, OnOff::On),
            "domain:figma.com (ad-hoc rule, on)"
        );
        assert_eq!(
            worded(
                "app:io.github.choco.nightcap.TourSample",
                RuleSource::Tour,
                OnOff::Off
            ),
            "app:io.github.choco.nightcap.TourSample (tour rule, off)"
        );
    }
}
