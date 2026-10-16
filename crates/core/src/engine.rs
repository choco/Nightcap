//! The rule engine: whether Night Shift is forced off or left alone, and why (ADR-0008).

use jiff::Timestamp;
use nightcap_protocol::{Decision, OnOff, Reason, Rule, RuleIdentity, Verdict};

/// What rules match against.
#[derive(Default)]
pub(crate) struct Context {
    /// The frontmost app's bundle ID, when it has one.
    pub app: Option<String>,
}

pub(crate) struct Inputs<'a> {
    pub context: &'a Context,
    pub rules: &'a [Rule],
    pub manual_state: Option<OnOff>,
    pub scheduled_state: Option<OnOff>,
    pub next_transition: Option<Timestamp>,
}

pub(crate) fn decide(inputs: &Inputs<'_>) -> Decision {
    let forcing: Vec<_> = inputs
        .rules
        .iter()
        .filter(|rule| matches(rule, inputs.context))
        .cloned()
        .map(Reason::Rule)
        .collect();
    if !forcing.is_empty() {
        return Decision {
            verdict: Verdict::ForceOff,
            reasons: forcing,
        };
    }
    Decision {
        verdict: Verdict::HandsOff,
        reasons: match (inputs.manual_state, inputs.scheduled_state) {
            (Some(state), _) => vec![Reason::ManualState {
                state,
                until: inputs.next_transition,
            }],
            (None, Some(state)) => vec![Reason::Schedule {
                state,
                until: inputs.next_transition,
            }],
            (None, None) => Vec::new(),
        },
    }
}

fn matches(rule: &Rule, context: &Context) -> bool {
    match &rule.identity {
        RuleIdentity::App(bundle_id) => context
            .app
            .as_ref()
            .is_some_and(|app| app.eq_ignore_ascii_case(bundle_id)),
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
        }
    }

    #[test]
    fn with_nothing_forcing_it_night_shift_follows_the_schedule() {
        let decision = decide(&Inputs {
            context: &frontmost("com.apple.Terminal"),
            rules: &[],
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
            manual_state: None,
            scheduled_state: Some(OnOff::On),
            next_transition: None,
        });

        assert_eq!(decision.verdict, Verdict::ForceOff);
    }
}
