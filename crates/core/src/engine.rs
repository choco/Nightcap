//! The rule engine: what Night Shift should be, from the most specific place in front that
//! says something, and why (ADR-0024, ADR-0025).

use jiff::Timestamp;
use nightcap_protocol::{Decision, OnOff, Place, PlaceLayers, Reason, Rule, RuleIdentity};

use crate::domain;

/// What rules match against.
#[derive(Default)]
pub(crate) struct Context {
    /// The frontmost app's bundle ID, when it has one.
    pub app: Option<String>,
    /// The frontmost browser's host, normalized, while it's watched and known.
    pub host: Option<String>,
}

pub(crate) struct Inputs<'a> {
    pub context: &'a Context,
    pub rules: &'a [Rule],
    /// Every place with a hold or a timer.
    pub layers: &'a [PlaceLayers],
    /// A tour has Night Shift on for itself, in place of the schedule (ADR-0022).
    pub tour_stands_in: bool,
    pub scheduled_state: Option<OnOff>,
    pub next_transition: Option<Timestamp>,
}

pub(crate) fn decide(inputs: &Inputs<'_>) -> Decision {
    for place in in_front(inputs) {
        let reasons = layers_of(inputs, &place);
        if let Some(top) = reasons.first() {
            return Decision {
                state: Some(says(top)),
                place,
                reasons,
            };
        }
    }
    Decision {
        state: None,
        place: Place::Everywhere,
        reasons: Vec::new(),
    }
}

/// The places in front, most specific first: the domains covering the host, longest first,
/// then the app, then everywhere.
pub(crate) fn in_front(inputs: &Inputs<'_>) -> Vec<Place> {
    let mut domains: Vec<String> = Vec::new();
    if let Some(host) = &inputs.context.host {
        let ruled = inputs.rules.iter().filter_map(|rule| match &rule.identity {
            RuleIdentity::Domain(domain) => Some(domain),
            RuleIdentity::App(_) => None,
        });
        let layered = inputs
            .layers
            .iter()
            .filter_map(|layers| match &layers.place {
                Place::Domain(domain) => Some(domain),
                _ => None,
            });
        for domain in ruled.chain(layered) {
            if domain::covers(domain, host) && !domains.contains(domain) {
                domains.push(domain.clone());
            }
        }
        domains.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    }
    domains
        .into_iter()
        .map(Place::Domain)
        .chain(inputs.context.app.clone().map(Place::App))
        .chain([Place::Everywhere])
        .collect()
}

/// What a place holds, top first: its timer, its hold, then its rule, or for everywhere the
/// tour and the schedule. A place with none of them says nothing.
pub(crate) fn layers_of(inputs: &Inputs<'_>, place: &Place) -> Vec<Reason> {
    let layers = inputs
        .layers
        .iter()
        .find(|layers| same_place(&layers.place, place));
    let mut reasons: Vec<Reason> = layers
        .into_iter()
        .flat_map(|layers| {
            let timer = layers.timer.clone().map(Reason::Timer);
            let hold = layers.hold.clone().map(Reason::Hold);
            timer.into_iter().chain(hold)
        })
        .collect();
    match place {
        Place::Everywhere => {
            if inputs.tour_stands_in {
                reasons.push(Reason::Tour);
            }
            if let Some(state) = inputs.scheduled_state {
                reasons.push(Reason::Schedule {
                    state,
                    until: inputs.next_transition,
                });
            }
        }
        Place::App(_) | Place::Domain(_) => {
            if let Some(rule) = inputs
                .rules
                .iter()
                .find(|rule| same_place(&Place::from(rule.identity.clone()), place))
            {
                reasons.push(Reason::Rule(rule.clone()));
            }
        }
    }
    reasons
}

/// What a layer says.
pub(crate) fn says(reason: &Reason) -> OnOff {
    match reason {
        Reason::Timer(layer) | Reason::Hold(layer) => layer.state,
        Reason::Rule(rule) => rule.says,
        Reason::Tour => OnOff::On,
        Reason::Schedule { state, .. } => *state,
    }
}

/// Places are the same when they name the same app, bundle IDs in any case, or the same
/// domain.
pub(crate) fn same_place(a: &Place, b: &Place) -> bool {
    match (a, b) {
        (Place::Everywhere, Place::Everywhere) => true,
        (Place::App(a), Place::App(b)) => a.eq_ignore_ascii_case(b),
        (Place::Domain(a), Place::Domain(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use nightcap_protocol::{Layer, RuleSource};

    use super::*;

    fn at(instant: &str) -> Timestamp {
        instant.parse().unwrap()
    }

    fn app_rule(bundle_id: &str, says: OnOff) -> Rule {
        Rule {
            identity: RuleIdentity::App(bundle_id.to_owned()),
            source: RuleSource::Config,
            says,
        }
    }

    fn domain_rule(domain: &str, says: OnOff) -> Rule {
        Rule {
            identity: RuleIdentity::Domain(domain.to_owned()),
            source: RuleSource::Config,
            says,
        }
    }

    fn layer(state: OnOff, until: Option<&str>) -> Layer {
        Layer {
            state,
            until: until.map(at),
            with: None,
        }
    }

    fn app(bundle_id: &str) -> Place {
        Place::App(bundle_id.to_owned())
    }

    /// What the engine is given, everything left out being nothing: no app or host in front,
    /// no rules or layers, no tour and no schedule.
    #[derive(Default)]
    struct Given {
        app: Option<&'static str>,
        host: Option<&'static str>,
        rules: Vec<Rule>,
        layers: Vec<PlaceLayers>,
        tour: bool,
        scheduled: Option<OnOff>,
        next: Option<&'static str>,
    }

    impl Given {
        fn decide(&self) -> Decision {
            let context = Context {
                app: self.app.map(str::to_owned),
                host: self.host.map(str::to_owned),
            };
            decide(&Inputs {
                context: &context,
                rules: &self.rules,
                layers: &self.layers,
                tour_stands_in: self.tour,
                scheduled_state: self.scheduled,
                next_transition: self.next.map(at),
            })
        }
    }

    #[test]
    fn with_nothing_in_front_saying_anything_everywhere_decides() {
        let decision = Given {
            app: Some("com.apple.Terminal"),
            scheduled: Some(OnOff::On),
            next: Some("2026-10-16T05:50:47Z"),
            ..Given::default()
        }
        .decide();

        assert_eq!(
            decision,
            Decision {
                state: Some(OnOff::On),
                place: Place::Everywhere,
                reasons: vec![Reason::Schedule {
                    state: OnOff::On,
                    until: Some(at("2026-10-16T05:50:47Z")),
                }],
            }
        );
    }

    #[test]
    fn with_the_schedule_unknown_and_nothing_else_the_decision_has_no_state() {
        let decision = Given::default().decide();

        assert_eq!(
            decision,
            Decision {
                state: None,
                place: Place::Everywhere,
                reasons: Vec::new(),
            }
        );
    }

    #[test]
    fn an_app_rule_that_says_off_decides_while_its_app_is_in_front() {
        let decision = Given {
            app: Some("com.figma.Desktop"),
            rules: vec![
                app_rule("com.figma.Desktop", OnOff::Off),
                app_rule("com.adobe.Photoshop", OnOff::Off),
            ],
            scheduled: Some(OnOff::On),
            ..Given::default()
        }
        .decide();

        assert_eq!(
            decision,
            Decision {
                state: Some(OnOff::Off),
                place: app("com.figma.Desktop"),
                reasons: vec![Reason::Rule(app_rule("com.figma.Desktop", OnOff::Off))],
            }
        );
    }

    #[test]
    fn an_app_rule_that_says_on_turns_night_shift_on_by_day() {
        let decision = Given {
            app: Some("com.apple.iBooksX"),
            rules: vec![app_rule("com.apple.iBooksX", OnOff::On)],
            scheduled: Some(OnOff::Off),
            ..Given::default()
        }
        .decide();

        assert_eq!(decision.state, Some(OnOff::On));
        assert_eq!(decision.place, app("com.apple.iBooksX"));
    }

    #[test]
    fn app_rules_match_bundle_ids_in_any_case() {
        let decision = Given {
            app: Some("com.figma.Desktop"),
            rules: vec![app_rule("com.figma.desktop", OnOff::Off)],
            scheduled: Some(OnOff::On),
            ..Given::default()
        }
        .decide();

        assert_eq!(decision.state, Some(OnOff::Off));
    }

    #[test]
    fn a_domain_rule_decides_on_its_domain_and_subdomains() {
        let state_on = |host: &'static str| {
            Given {
                app: Some("com.apple.Safari"),
                host: Some(host),
                rules: vec![domain_rule("figma.com", OnOff::Off)],
                scheduled: Some(OnOff::On),
                ..Given::default()
            }
            .decide()
            .state
        };

        assert_eq!(state_on("figma.com"), Some(OnOff::Off));
        assert_eq!(state_on("www.figma.com"), Some(OnOff::Off));
        assert_eq!(state_on("notfigma.com"), Some(OnOff::On));
        assert_eq!(state_on("figma.com.example.net"), Some(OnOff::On));
    }

    #[test]
    fn a_site_decides_over_its_browsers_app_rule() {
        let decide_on = |host: &'static str| {
            Given {
                app: Some("com.apple.Safari"),
                host: Some(host),
                rules: vec![
                    app_rule("com.apple.Safari", OnOff::Off),
                    domain_rule("youtube.com", OnOff::On),
                ],
                scheduled: Some(OnOff::Off),
                ..Given::default()
            }
            .decide()
        };

        let decision = decide_on("www.youtube.com");
        assert_eq!(decision.state, Some(OnOff::On));
        assert_eq!(decision.place, Place::Domain("youtube.com".to_owned()));
        let decision = decide_on("apple.com");
        assert_eq!(decision.state, Some(OnOff::Off));
        assert_eq!(decision.place, app("com.apple.Safari"));
    }

    #[test]
    fn the_longest_covering_domain_decides() {
        let decide_on = |host: &'static str| {
            Given {
                app: Some("com.apple.Safari"),
                host: Some(host),
                rules: vec![
                    domain_rule("google.com", OnOff::Off),
                    domain_rule("docs.google.com", OnOff::On),
                ],
                scheduled: Some(OnOff::On),
                ..Given::default()
            }
            .decide()
        };

        assert_eq!(
            decide_on("docs.google.com").place,
            Place::Domain("docs.google.com".to_owned())
        );
        assert_eq!(decide_on("docs.google.com").state, Some(OnOff::On));
        assert_eq!(
            decide_on("mail.google.com").place,
            Place::Domain("google.com".to_owned())
        );
        assert_eq!(decide_on("mail.google.com").state, Some(OnOff::Off));
    }

    #[test]
    fn a_layer_on_a_site_without_a_rule_decides_on_its_subdomains() {
        let decision = Given {
            app: Some("com.apple.Safari"),
            host: Some("a.docs.google.com"),
            rules: vec![domain_rule("google.com", OnOff::Off)],
            layers: vec![PlaceLayers {
                place: Place::Domain("docs.google.com".to_owned()),
                hold: None,
                timer: Some(layer(OnOff::On, Some("2026-10-16T22:00:00Z"))),
            }],
            scheduled: Some(OnOff::On),
            ..Given::default()
        }
        .decide();

        assert_eq!(decision.state, Some(OnOff::On));
        assert_eq!(decision.place, Place::Domain("docs.google.com".to_owned()));
    }

    #[test]
    fn a_places_timer_beats_its_hold_and_its_hold_beats_its_rule() {
        let figma = app_rule("com.figma.Desktop", OnOff::Off);
        let decide_with = |timer: Option<Layer>| {
            Given {
                app: Some("com.figma.Desktop"),
                rules: vec![figma.clone()],
                layers: vec![PlaceLayers {
                    place: app("com.figma.Desktop"),
                    hold: Some(layer(OnOff::On, Some("2026-10-17T05:00:00Z"))),
                    timer,
                }],
                scheduled: Some(OnOff::On),
                ..Given::default()
            }
            .decide()
        };

        assert_eq!(
            decide_with(None),
            Decision {
                state: Some(OnOff::On),
                place: app("com.figma.Desktop"),
                reasons: vec![
                    Reason::Hold(layer(OnOff::On, Some("2026-10-17T05:00:00Z"))),
                    Reason::Rule(figma.clone()),
                ],
            }
        );
        assert_eq!(
            decide_with(Some(layer(OnOff::Off, Some("2026-10-16T21:30:00Z")))),
            Decision {
                state: Some(OnOff::Off),
                place: app("com.figma.Desktop"),
                reasons: vec![
                    Reason::Timer(layer(OnOff::Off, Some("2026-10-16T21:30:00Z"))),
                    Reason::Hold(layer(OnOff::On, Some("2026-10-17T05:00:00Z"))),
                    Reason::Rule(figma),
                ],
            }
        );
    }

    #[test]
    fn everywheres_timer_and_hold_beat_the_schedule() {
        let decision = Given {
            app: Some("com.apple.mail"),
            layers: vec![PlaceLayers {
                place: Place::Everywhere,
                hold: Some(layer(OnOff::Off, None)),
                timer: Some(layer(OnOff::On, Some("2026-10-16T21:00:00Z"))),
            }],
            scheduled: Some(OnOff::On),
            ..Given::default()
        }
        .decide();

        assert_eq!(
            decision,
            Decision {
                state: Some(OnOff::On),
                place: Place::Everywhere,
                reasons: vec![
                    Reason::Timer(layer(OnOff::On, Some("2026-10-16T21:00:00Z"))),
                    Reason::Hold(layer(OnOff::Off, None)),
                    Reason::Schedule {
                        state: OnOff::On,
                        until: None,
                    },
                ],
            }
        );
    }

    #[test]
    fn the_tour_stands_in_for_the_schedule_under_everywheres_hold_and_timer() {
        let decide_with = |layers: Vec<PlaceLayers>| {
            Given {
                layers,
                tour: true,
                scheduled: Some(OnOff::Off),
                ..Given::default()
            }
            .decide()
        };

        assert_eq!(
            decide_with(Vec::new()).reasons,
            vec![
                Reason::Tour,
                Reason::Schedule {
                    state: OnOff::Off,
                    until: None,
                },
            ]
        );
        assert_eq!(decide_with(Vec::new()).state, Some(OnOff::On));
        let held_off = decide_with(vec![PlaceLayers {
            place: Place::Everywhere,
            hold: Some(layer(OnOff::Off, None)),
            timer: None,
        }]);
        assert_eq!(held_off.state, Some(OnOff::Off));
    }
}
