//! Resolves applicable places in order, most specific first (ADR-0018). Decisions, each place's
//! inheritance and reset results all read the same ordering and reasons.

use nightcap_protocol::{
    BundleId, Decision, Domain, Front, Inherited, Length, OnOff, Place, PlaceInFront, Reason,
    ResetTo,
};

use super::{World, set::RuleSet};
use crate::domain;

/// One place's own reasons and what it inherits from the places after it.
#[derive(Debug)]
struct ResolvedPlace {
    place: Place,
    reasons: Vec<Reason>,
    inherits: Option<Inherited>,
    gives: OnOff,
}

/// A read of the active rules at one moment. Less specific domains remain in the ordering
/// even though the front query shows only the most specific site.
#[derive(Debug)]
pub(super) struct Resolution<'a> {
    rules: &'a RuleSet,
    places: Vec<ResolvedPlace>,
    everywhere: ResolvedPlace,
    app: Option<BundleId>,
    host: Option<String>,
    site: Option<Domain>,
    base: Vec<Reason>,
    released: bool,
    observed: OnOff,
}

impl<'a> Resolution<'a> {
    pub(super) fn new(
        rules: &'a RuleSet,
        world: &World<'_>,
        stands_in: bool,
        released: bool,
    ) -> Self {
        let mut domains: Vec<Domain> = Vec::new();
        if let Some(host) = world.context.host.as_ref().filter(|_| !released) {
            for rule in rules.indefinite().iter().chain(rules.temporary()) {
                if let Place::Domain(domain) = &rule.place
                    && domain::covers(domain, host)
                    && !domains.contains(domain)
                {
                    domains.push(domain.clone());
                }
            }
            domains.sort_by(|a, b| b.as_str().len().cmp(&a.as_str().len()).then(a.cmp(b)));
        }
        let site = world
            .context
            .host
            .as_ref()
            .filter(|_| world.website_checking)
            .map(|host| {
                domains
                    .first()
                    .cloned()
                    .unwrap_or_else(|| host.tab_domain())
            });
        // A site without its own rule still has a row, inheriting from its browser or everywhere.
        if domains.is_empty()
            && let Some(site) = &site
        {
            domains.push(site.clone());
        }
        let places = domains
            .into_iter()
            .map(Place::Domain)
            .chain(world.context.app.clone().map(Place::App));
        let schedule = world
            .timeline
            .scheduled_state(world.applied_until)
            .map(|state| Reason::Schedule {
                state: OnOff::from(state),
                until: world.timeline.next_transition(world.applied_until),
            });
        let base: Vec<_> = stands_in
            .then_some(Reason::StandIn)
            .into_iter()
            .chain(schedule)
            .collect();
        let (places, everywhere) = Self::resolve(rules, places, &base, released, world.observed);
        Self {
            places,
            everywhere,
            app: world.context.app.clone(),
            host: world
                .context
                .host
                .as_ref()
                .map(|host| host.as_str().to_owned()),
            rules,
            site,
            base,
            released,
            observed: world.observed,
        }
    }

    /// Each place owns its reasons; inheritance is the first reason supplied after it.
    fn resolve(
        rules: &RuleSet,
        places: impl IntoIterator<Item = Place>,
        base: &[Reason],
        released: bool,
        observed: OnOff,
    ) -> (Vec<ResolvedPlace>, ResolvedPlace) {
        let reasons = Self::reasons(rules, &Place::Everywhere, base, released);
        let says = reasons.first().map(Reason::says);
        let everywhere = ResolvedPlace {
            place: Place::Everywhere,
            reasons,
            inherits: None,
            gives: says.unwrap_or(observed),
        };
        let mut resolved: Vec<_> = places
            .into_iter()
            .map(|place| ResolvedPlace {
                reasons: Self::reasons(rules, &place, base, released),
                place,
                inherits: None,
                gives: observed,
            })
            .collect();
        let mut inherited = says.map(|says| Inherited {
            from: Place::Everywhere,
            says,
        });
        for at in resolved.iter_mut().rev() {
            let says = at.reasons.first().map(Reason::says);
            at.gives = says
                .or(inherited.as_ref().map(|after| after.says))
                .unwrap_or(observed);
            at.inherits = inherited.clone();
            if let Some(says) = says {
                inherited = Some(Inherited {
                    from: at.place.clone(),
                    says,
                });
            }
        }
        (resolved, everywhere)
    }

    fn reasons(rules: &RuleSet, place: &Place, base: &[Reason], released: bool) -> Vec<Reason> {
        if released && *place != Place::Everywhere {
            return Vec::new();
        }
        rules
            .temporary_at(place)
            .filter(|rule| !(released && matches!(rule.length, Length::Until { .. })))
            .into_iter()
            .chain(rules.indefinite_at(place))
            .cloned()
            .map(Reason::Rule)
            .chain(base.iter().filter(|_| *place == Place::Everywhere).cloned())
            .collect()
    }

    pub(super) fn says(&self, place: &Place) -> Option<OnOff> {
        Self::reasons(self.rules, place, &self.base, self.released)
            .first()
            .map(Reason::says)
    }

    pub(super) fn decision(&self) -> Decision {
        self.places
            .iter()
            .chain([&self.everywhere])
            .find_map(|at| {
                Some(Decision {
                    state: Some(at.reasons.first()?.says()),
                    place: at.place.clone(),
                    reasons: at.reasons.clone(),
                })
            })
            .unwrap_or(Decision {
                state: None,
                place: Place::Everywhere,
                reasons: Vec::new(),
            })
    }

    pub(super) fn front(&self, watching: bool) -> Front {
        let places = self
            .places
            .iter()
            .filter(|at| match &at.place {
                Place::Domain(domain) => Some(domain) == self.site.as_ref(),
                Place::App(_) => true,
                Place::Everywhere => false,
            })
            .map(|at| self.standing(at))
            .collect();
        Front {
            app: self.app.clone(),
            watching,
            host: self.host.clone(),
            site: self.site.clone(),
            decision: self.decision(),
            places,
            everywhere: self.standing(&self.everywhere),
        }
    }

    fn standing(&self, at: &ResolvedPlace) -> PlaceInFront {
        PlaceInFront {
            place: at.place.clone(),
            gives: at.gives,
            temporary: self.rules.temporary_at(&at.place).cloned(),
            indefinite: self.rules.indefinite_at(&at.place).cloned(),
            base: (at.place == Place::Everywhere)
                .then(|| self.base.first().cloned())
                .flatten(),
            inherits: at.inherits.clone(),
            reset_to: self.reset_to(&at.place),
        }
    }

    /// Reuses Reset's mutation and this read's resolution with only the active rules copied.
    /// Earlier, more specific places do not decide the result at the requested place.
    fn reset_to(&self, place: &Place) -> Option<ResetTo> {
        let mut reset = self.rules.clone();
        reset.reset(place);
        let (places, everywhere) = Self::resolve(
            &reset,
            self.places.iter().map(|at| at.place.clone()),
            &self.base,
            self.released,
            self.observed,
        );
        places
            .iter()
            .chain([&everywhere])
            .skip_while(|at| at.place != *place)
            .find_map(|at| {
                let reason = at.reasons.first()?;
                if at.place != *place {
                    return Some(ResetTo::Inherited(Inherited {
                        from: at.place.clone(),
                        says: reason.says(),
                    }));
                }
                Some(match reason {
                    Reason::Rule(rule) => ResetTo::Rule { says: rule.says },
                    Reason::StandIn => ResetTo::StandIn,
                    Reason::Schedule { state, until } => ResetTo::Schedule {
                        state: *state,
                        until: *until,
                    },
                })
            })
    }
}
