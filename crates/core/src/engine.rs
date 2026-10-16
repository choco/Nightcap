//! The rule engine: whether Night Shift is forced off or left alone, and why (ADR-0008).

use jiff::Timestamp;
use nightcap_protocol::{Decision, OnOff, Reason, Rule, RuleIdentity, Verdict};

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
    /// Rules muted until the next schedule transition.
    pub snoozes: &'a [RuleIdentity],
    /// When the timed disable ends, if one is running.
    pub timed_disable: Option<Timestamp>,
    pub manual_state: Option<OnOff>,
    pub scheduled_state: Option<OnOff>,
    pub next_transition: Option<Timestamp>,
}

pub(crate) fn decide(inputs: &Inputs<'_>) -> Decision {
    let (snoozed, forcing): (Vec<_>, Vec<_>) = inputs
        .rules
        .iter()
        .filter(|rule| matches(rule, inputs.context))
        .partition(|rule| inputs.snoozes.contains(&rule.identity));
    let forces: Vec<_> = inputs
        .timed_disable
        .map(|until| Reason::TimedDisable { until })
        .into_iter()
        .chain(forcing.into_iter().cloned().map(Reason::Rule))
        .collect();
    if !forces.is_empty() {
        return Decision {
            verdict: Verdict::ForceOff,
            reasons: forces,
        };
    }
    let snoozes = snoozed.into_iter().map(|rule| Reason::Snooze {
        identity: rule.identity.clone(),
        until: inputs.next_transition,
    });
    let followed = match (inputs.manual_state, inputs.scheduled_state) {
        (Some(state), _) => Some(Reason::ManualState {
            state,
            until: inputs.next_transition,
        }),
        (None, Some(state)) => Some(Reason::Schedule {
            state,
            until: inputs.next_transition,
        }),
        (None, None) => None,
    };
    Decision {
        verdict: Verdict::HandsOff,
        reasons: snoozes.chain(followed).collect(),
    }
}

fn matches(rule: &Rule, context: &Context) -> bool {
    match &rule.identity {
        RuleIdentity::App(bundle_id) => context
            .app
            .as_ref()
            .is_some_and(|app| app.eq_ignore_ascii_case(bundle_id)),
        RuleIdentity::Domain(domain) => context
            .host
            .as_ref()
            .is_some_and(|host| domain::covers(domain, host)),
    }
}

#[cfg(test)]
mod tests {
    use nightcap_protocol::RuleSource;

    use super::*;

    fn at(instant: &str) -> Timestamp {
        instant.parse().unwrap()
    }

    fn app_rule(bundle_id: &str) -> Rule {
        Rule {
            identity: RuleIdentity::App(bundle_id.to_owned()),
            source: RuleSource::Config,
        }
    }

    fn frontmost(bundle_id: &str) -> Context {
        Context {
            app: Some(bundle_id.to_owned()),
            host: None,
        }
    }

    fn domain_rule(domain: &str) -> Rule {
        Rule {
            identity: RuleIdentity::Domain(domain.to_owned()),
            source: RuleSource::Config,
        }
    }

    #[test]
    fn with_nothing_forcing_it_night_shift_follows_the_schedule() {
        let decision = decide(&Inputs {
            context: &frontmost("com.apple.Terminal"),
            rules: &[],
            snoozes: &[],
            timed_disable: None,
            manual_state: None,
            scheduled_state: Some(OnOff::On),
            next_transition: Some(at("2026-10-16T05:50:47Z")),
        });

        assert_eq!(
            decision,
            Decision {
                verdict: Verdict::HandsOff,
                reasons: vec![Reason::Schedule {
                    state: OnOff::On,
                    until: Some(at("2026-10-16T05:50:47Z")),
                }],
            }
        );
    }

    #[test]
    fn the_manual_state_beats_the_schedule() {
        let decision = decide(&Inputs {
            context: &frontmost("com.apple.Terminal"),
            rules: &[],
            snoozes: &[],
            timed_disable: None,
            manual_state: Some(OnOff::Off),
            scheduled_state: Some(OnOff::On),
            next_transition: Some(at("2026-10-16T05:50:47Z")),
        });

        assert_eq!(
            decision,
            Decision {
                verdict: Verdict::HandsOff,
                reasons: vec![Reason::ManualState {
                    state: OnOff::Off,
                    until: Some(at("2026-10-16T05:50:47Z")),
                }],
            }
        );
    }

    #[test]
    fn an_app_rule_forces_night_shift_off_while_its_app_is_frontmost() {
        let rules = [
            app_rule("com.figma.Desktop"),
            app_rule("com.adobe.Photoshop"),
        ];

        let decision = decide(&Inputs {
            context: &frontmost("com.figma.Desktop"),
            rules: &rules,
            snoozes: &[],
            timed_disable: None,
            manual_state: Some(OnOff::On),
            scheduled_state: Some(OnOff::On),
            next_transition: None,
        });

        assert_eq!(
            decision,
            Decision {
                verdict: Verdict::ForceOff,
                reasons: vec![Reason::Rule(app_rule("com.figma.Desktop"))],
            }
        );
    }

    #[test]
    fn app_rules_match_bundle_ids_in_any_case() {
        let rules = [app_rule("com.figma.desktop")];

        let decision = decide(&Inputs {
            context: &frontmost("com.figma.Desktop"),
            rules: &rules,
            snoozes: &[],
            timed_disable: None,
            manual_state: None,
            scheduled_state: Some(OnOff::On),
            next_transition: None,
        });

        assert_eq!(decision.verdict, Verdict::ForceOff);
    }

    #[test]
    fn a_snoozed_rule_leaves_night_shift_alone_and_says_so() {
        let rules = [app_rule("com.figma.Desktop")];

        let decision = decide(&Inputs {
            context: &frontmost("com.figma.Desktop"),
            rules: &rules,
            snoozes: &[RuleIdentity::App("com.figma.Desktop".to_owned())],
            timed_disable: None,
            manual_state: Some(OnOff::On),
            scheduled_state: Some(OnOff::On),
            next_transition: Some(at("2026-10-16T05:50:47Z")),
        });

        assert_eq!(
            decision,
            Decision {
                verdict: Verdict::HandsOff,
                reasons: vec![
                    Reason::Snooze {
                        identity: RuleIdentity::App("com.figma.Desktop".to_owned()),
                        until: Some(at("2026-10-16T05:50:47Z")),
                    },
                    Reason::ManualState {
                        state: OnOff::On,
                        until: Some(at("2026-10-16T05:50:47Z")),
                    },
                ],
            }
        );
    }

    #[test]
    fn a_timed_disable_forces_night_shift_off_alongside_any_rule() {
        fn disabled_with(app: &str) -> Decision {
            decide(&Inputs {
                context: &frontmost(app),
                rules: &[app_rule("com.figma.Desktop")],
                snoozes: &[],
                timed_disable: Some(at("2026-10-16T01:00:00Z")),
                manual_state: Some(OnOff::On),
                scheduled_state: Some(OnOff::On),
                next_transition: None,
            })
        }
        let disabled = Reason::TimedDisable {
            until: at("2026-10-16T01:00:00Z"),
        };

        assert_eq!(
            disabled_with("com.apple.Terminal"),
            Decision {
                verdict: Verdict::ForceOff,
                reasons: vec![disabled.clone()],
            }
        );
        assert_eq!(
            disabled_with("com.figma.Desktop"),
            Decision {
                verdict: Verdict::ForceOff,
                reasons: vec![disabled, Reason::Rule(app_rule("com.figma.Desktop"))],
            }
        );
    }

    #[test]
    fn a_domain_rule_forces_night_shift_off_on_its_domain_and_subdomains() {
        fn verdict_on(host: &str) -> Verdict {
            decide(&Inputs {
                context: &Context {
                    app: Some("com.apple.Safari".to_owned()),
                    host: Some(host.to_owned()),
                },
                rules: &[domain_rule("figma.com")],
                snoozes: &[],
                timed_disable: None,
                manual_state: None,
                scheduled_state: Some(OnOff::On),
                next_transition: None,
            })
            .verdict
        }

        assert_eq!(verdict_on("figma.com"), Verdict::ForceOff);
        assert_eq!(verdict_on("www.figma.com"), Verdict::ForceOff);
        assert_eq!(verdict_on("notfigma.com"), Verdict::HandsOff);
        assert_eq!(verdict_on("figma.com.example.net"), Verdict::HandsOff);
    }
}
