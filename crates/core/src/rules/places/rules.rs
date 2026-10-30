//! One set of rules and the invariants every mutation preserves (ADR-0018): one indefinite
//! and one temporary rule at most per place, with Adoption's linked rules removed together.

use jiff::Timestamp;
use nightcap_protocol::{
    Length, ListedRule, OnOff, Place, Refusal, Rule, RuleAdded, RuleIdentity, SetBy,
};

use super::{indefinite, is_turn, loggable, rule};

/// Every rule, as what decides now, or as a trial found them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Rules {
    /// The apps' and sites' indefinite rules, one per place: the config's, then the user's
    /// ad-hoc ones, which `rules.toml` keeps, then a trial's (ADR-0006).
    indefinite: Vec<Rule>,
    /// Everywhere's indefinite rule: the switch's.
    switched: Option<Rule>,
    /// Each place's temporary rule, one at most.
    temporary: Vec<Rule>,
    /// The place an outside change set a temporary rule at together with everywhere's: taking
    /// off either takes off both (ADR-0018).
    linked: Option<Place>,
}

impl Rules {
    pub(super) fn new(
        kept: Vec<Rule>,
        linked: Option<Place>,
        ad_hoc: Vec<(RuleIdentity, OnOff)>,
    ) -> Self {
        let mut live = Self {
            indefinite: ad_hoc
                .into_iter()
                .map(|(identity, says)| indefinite(identity, says, SetBy::You))
                .collect(),
            switched: None,
            temporary: Vec::new(),
            linked,
        };
        for rule in kept {
            if rule.length != Length::Indefinite {
                if live.temporary_at(&rule.place).is_none() {
                    live.temporary.push(rule);
                }
            } else if rule.place == Place::Everywhere {
                live.switched.get_or_insert(rule);
            }
        }
        live.tidy();
        live
    }

    pub(super) fn kept(&self) -> (Vec<Rule>, Option<Place>) {
        (
            self.temporary
                .iter()
                .chain(&self.switched)
                .cloned()
                .collect(),
            self.linked.clone(),
        )
    }

    pub(super) fn ad_hoc(&self) -> Vec<(RuleIdentity, OnOff)> {
        self.indefinite
            .iter()
            .filter(|rule| rule.set_by == SetBy::You)
            .filter_map(|rule| Some((rule.place.identity()?, rule.says)))
            .collect()
    }

    pub(super) fn indefinite(&self) -> &[Rule] {
        &self.indefinite
    }

    pub(super) fn temporary(&self) -> &[Rule] {
        &self.temporary
    }

    pub(super) fn listed(&self) -> nightcap_protocol::Rules {
        nightcap_protocol::Rules {
            rules: self
                .indefinite
                .iter()
                .map(|rule| ListedRule {
                    rule: rule.clone(),
                    snooze: self
                        .temporary_at(&rule.place)
                        .filter(|temporary| temporary.says != rule.says)
                        .cloned(),
                })
                .collect(),
            temporary: self.temporary.clone(),
        }
    }

    pub(super) fn set_config(&mut self, config: &[(RuleIdentity, OnOff)]) {
        let config: Vec<Rule> = config
            .iter()
            .map(|(identity, says)| indefinite(identity.clone(), *says, SetBy::Config))
            .collect();
        self.indefinite.retain(|rule| {
            rule.set_by != SetBy::Config && !config.iter().any(|each| each.place == rule.place)
        });
        self.indefinite.splice(0..0, config);
    }

    /// Input is checked here so the collections never hold an app's indefinite rule in the
    /// switch's slot, even if a new caller forgets the command's constraint.
    pub(super) fn set(
        &mut self,
        place: &Place,
        says: OnOff,
        length: Length,
    ) -> Result<(), Refusal> {
        if length == Length::Indefinite && *place != Place::Everywhere {
            return Err(Refusal::IndefiniteIsAdded);
        }
        tracing::info!("{} {says} {length:?}", loggable(place));
        self.put(rule(place.clone(), says, length, SetBy::You));
        Ok(())
    }

    pub(super) fn set_everywhere(&mut self, says: OnOff) {
        self.put(rule(
            Place::Everywhere,
            says,
            Length::Indefinite,
            SetBy::You,
        ));
    }

    pub(super) fn adopt(&mut self, place: &Place, says: OnOff, until: Option<Timestamp>) {
        self.put(rule(
            place.clone(),
            says,
            Length::UntilScheduleTurns { until },
            SetBy::OutsideChange,
        ));
    }

    /// Adds everywhere's partner and the link in one operation. The place's adoption is
    /// installed too, so a link can never refer to a rule that is absent.
    pub(super) fn link_adoption(&mut self, place: &Place, says: OnOff, until: Option<Timestamp>) {
        self.adopt(place, says, until);
        if *place != Place::Everywhere {
            self.adopt(&Place::Everywhere, says, until);
            self.linked = Some(place.clone());
        }
    }

    pub(super) fn reset(&mut self, place: &Place) {
        while self.peel(place, None) {}
    }

    pub(super) fn add_rule(
        &mut self,
        identity: &RuleIdentity,
        says: OnOff,
        set_by: SetBy,
    ) -> Result<RuleAdded, Refusal> {
        let place = Place::from(identity.clone());
        let rules = &mut self.indefinite;
        let added = match rules.iter_mut().find(|rule| rule.place == place) {
            Some(rule) if rule.says == says => RuleAdded::AlreadySays {
                set_by: rule.set_by,
            },
            Some(rule) if rule.set_by == SetBy::Config => {
                return Err(Refusal::ConfigSays {
                    identity: identity.clone(),
                    says: rule.says,
                });
            }
            Some(rule) => {
                rule.says = says;
                RuleAdded::Changed {
                    set_by: rule.set_by,
                }
            }
            None => {
                rules.push(indefinite(identity.clone(), says, set_by));
                RuleAdded::New { set_by }
            }
        };
        tracing::info!("{} rule {}: {added:?}", loggable(&place), says);
        self.reset(&place);
        Ok(added)
    }

    pub(super) fn remove_rule(&mut self, identity: &RuleIdentity) -> Result<(), Refusal> {
        let place = Place::from(identity.clone());
        let rules = &mut self.indefinite;
        let identity = identity.clone();
        match rules
            .iter()
            .find(|rule| rule.place == place)
            .map(|rule| rule.set_by)
        {
            None => return Err(Refusal::NoSuchRule { identity }),
            Some(SetBy::Config) => return Err(Refusal::ConfigRule { identity }),
            Some(_) => rules.retain(|rule| rule.place != place),
        }
        tracing::info!("removed the rule for {}", loggable(&place));
        self.reset(&place);
        Ok(())
    }

    pub(super) fn expire(&mut self, at: Timestamp) {
        self.remove_temporary_where(|rule| rule.length.end().is_some_and(|end| end <= at));
    }

    pub(super) fn schedule_edited(&mut self) {
        self.remove_temporary_where(|rule| {
            matches!(rule.length, Length::UntilScheduleTurns { until: Some(_) })
        });
    }

    fn remove_temporary_where(&mut self, remove: impl Fn(&Rule) -> bool) {
        self.temporary.retain(|rule| !remove(rule));
        self.tidy();
    }

    /// Takes off the first of a place's rules that a reset would, its temporary one, then for
    /// everywhere its indefinite one, unless it says `keep`, with the one Adoption linked to it,
    /// and says whether it did.
    pub(super) fn peel(&mut self, place: &Place, keep: Option<OnOff>) -> bool {
        let switched = self
            .switched
            .as_ref()
            .filter(|_| *place == Place::Everywhere);
        let Some(rule) = self.temporary_at(place).or(switched) else {
            return false;
        };
        if Some(rule.says) == keep {
            return false;
        }
        if rule.length == Length::Indefinite {
            self.switched = None;
            return true;
        }
        let turn = is_turn(rule);
        self.temporary.retain(|rule| rule.place != *place);
        if turn
            && let Some(linked) =
                (self.linked).take_if(|linked| linked == place || *place == Place::Everywhere)
        {
            let partner = if *place == Place::Everywhere {
                linked
            } else {
                Place::Everywhere
            };
            (self.temporary).retain(|rule| !(rule.place == partner && is_turn(rule)));
        }
        true
    }

    /// Puts a rule at its place: a temporary one in place of the one there, and everywhere's
    /// indefinite one in place of both of everywhere's. Taking off one that Adoption linked
    /// leaves its partner unlinked.
    fn put(&mut self, rule: Rule) {
        if rule.length == Length::Indefinite {
            self.temporary.retain(|each| each.place != rule.place);
            self.switched = Some(rule);
        } else if let Some(each) = (self.temporary.iter_mut()).find(|each| each.place == rule.place)
        {
            *each = rule;
        } else {
            self.temporary.push(rule);
        }
        self.tidy();
    }

    pub(super) fn temporary_at(&self, place: &Place) -> Option<&Rule> {
        self.temporary.iter().find(|rule| rule.place == *place)
    }

    /// An app's or site's indefinite rule, or everywhere's, the switch's.
    pub(super) fn indefinite_at(&self, place: &Place) -> Option<&Rule> {
        match place {
            Place::Everywhere => self.switched.as_ref(),
            Place::App(_) | Place::Domain(_) => {
                self.indefinite.iter().find(|rule| rule.place == *place)
            }
        }
    }

    /// Drops Adoption's link unless both its places still hold the rules it set.
    fn tidy(&mut self) {
        let temporary = &self.temporary;
        self.linked.take_if(|linked| {
            ![&*linked, &Place::Everywhere].into_iter().all(|end| {
                temporary.iter().any(|rule| {
                    rule.place == *end && is_turn(rule) && rule.set_by == SetBy::OutsideChange
                })
            })
        });
    }

    #[cfg(test)]
    pub(super) fn assert_sound(&self) {
        for rules in [&self.indefinite, &self.temporary] {
            for (index, rule) in rules.iter().enumerate() {
                assert!(
                    !rules[index + 1..]
                        .iter()
                        .any(|other| other.place == rule.place),
                    "{} has two rules of a kind",
                    rule.place
                );
            }
        }
        assert!(
            (self.indefinite.iter())
                .all(|rule| rule.length == Length::Indefinite && rule.place != Place::Everywhere)
        );
        assert!(
            (self.switched.iter())
                .all(|rule| rule.length == Length::Indefinite && rule.place == Place::Everywhere)
        );
        assert!(
            (self.temporary.iter()).all(|rule| rule.length != Length::Indefinite),
            "{:?}",
            self.temporary
        );
        let rules = &self.temporary;
        if let Some(linked) = &self.linked {
            assert_ne!(*linked, Place::Everywhere);
            for end in [linked, &Place::Everywhere] {
                assert!(
                    rules.iter().any(|rule| rule.place == *end
                        && rule.set_by == SetBy::OutsideChange
                        && is_turn(rule)),
                    "{end} has no rule for the link to {linked}"
                );
            }
        }
    }
}
