//! What the places show: what Night Shift should be from the most specific place in front that
//! says something, and why, and how each place in front stands (ADR-0018, ADR-0006).

use jiff::Timestamp;
use nightcap_protocol::{Decision, Domain, OnOff, Place, Reason, Rule};

use super::{Context, Places, World, rank};
use crate::rules::domain;

/// Everything the places show, as of one call.
#[derive(Debug)]
pub(crate) struct Look<'a> {
    pub decision: Decision,
    pub everywhere: PlaceLook,
    /// The front app's, when it has a bundle ID a rule could name.
    pub app: Option<PlaceLook>,
    /// The site in front, while website checking knows the host: the most specific domain with
    /// a rule covering it, else the tab domain (ADR-0017).
    pub site: Option<PlaceLook>,
    /// Whether a browser's host could change anything: not on the way out, where only
    /// everywhere's rules decide (ADR-0008), nor while the front app's own place says off and
    /// no site could say on (ADR-0012).
    pub host_matters: bool,
    /// Every rule at every place, as they decide now, each with its snooze: the shortest rule
    /// at its place, where that's shorter and says the other way. The indefinite rules at apps
    /// and sites come first: the config's, then the ad-hoc ones, then the trial's.
    pub rules: Vec<(&'a Rule, Option<&'a Rule>)>,
}

/// One place's rules (ADR-0018), and what it gives with the places in front after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlaceLook {
    pub place: Place,
    /// Its rules over its base, shortest first: its temporary rules, and for everywhere its
    /// indefinite one too.
    pub rules: Vec<Rule>,
    /// What its rules sit on: an app's or site's indefinite rule, or for everywhere the stand-in,
    /// else the schedule once it's known.
    pub base: Option<Reason>,
    /// Its own say, else the one it inherits, else Night Shift as last reported.
    pub gives: OnOff,
    /// The first less specific place in front that says something, and what.
    pub inherits: Option<(Place, OnOff)>,
}

impl Places {
    pub(crate) fn look<'a>(&'a self, world: &World<'_>) -> Look<'a> {
        Look {
            decision: self.decide(world),
            everywhere: self.place_look(Place::Everywhere, world),
            app: world
                .context
                .app
                .clone()
                .map(|app| self.place_look(Place::App(app), world)),
            site: self
                .site_in_front(world)
                .map(|site| self.place_look(site, world)),
            host_matters: self.host_matters(world),
            rules: self
                .indefinite
                .iter()
                .chain(&self.live.rules)
                .map(|rule| (rule, self.snooze(rule)))
                .collect(),
        }
    }

    pub(super) fn decide(&self, world: &World<'_>) -> Decision {
        decide(&self.inputs(world))
    }

    /// What a place itself says, or None when nothing there says anything.
    pub(super) fn says(&self, place: &Place, world: &World<'_>) -> Option<OnOff> {
        reasons_at(&self.inputs(world), place).first().map(says)
    }

    /// A place's rules and what it gives, from them and the places in front after it.
    fn place_look(&self, place: Place, world: &World<'_>) -> PlaceLook {
        let inputs = self.inputs(world);
        let from: Vec<(Place, Vec<Reason>)> = self
            .places_from(&place, world)
            .into_iter()
            .map(|each| {
                let reasons = reasons_at(&inputs, &each);
                (each, reasons)
            })
            .collect();
        let inherits = from
            .iter()
            .skip(1)
            .find_map(|(each, reasons)| reasons.first().map(|top| (each.clone(), says(top))));
        let gives = from
            .iter()
            .find_map(|(_, reasons)| reasons.first().map(says))
            .unwrap_or(world.observed);
        let base = match &place {
            Place::Everywhere if self.stands_in() => Some(Reason::StandIn),
            Place::Everywhere => inputs.scheduled_state.map(|state| Reason::Schedule {
                state,
                until: inputs.next_transition,
            }),
            Place::App(_) | Place::Domain(_) => {
                self.indefinite_at(&place).cloned().map(Reason::Rule)
            }
        };
        PlaceLook {
            rules: self.rules_at(&place).cloned().collect(),
            place,
            base,
            gives,
            inherits,
        }
    }

    /// A rule's snooze: the shortest rule at its place, where that's shorter and says the other
    /// way.
    fn snooze(&self, rule: &Rule) -> Option<&Rule> {
        self.live
            .rules
            .iter()
            .find(|each| each.place == rule.place)
            .filter(|shortest| rank(shortest) < rank(rule) && shortest.says != rule.says)
    }

    /// The site in front, while website checking knows the host: the most specific domain with
    /// a rule covering it, else the tab domain (ADR-0017).
    fn site_in_front(&self, world: &World<'_>) -> Option<Place> {
        let host = world
            .context
            .host
            .as_ref()
            .filter(|_| world.website_checking)?;
        Some(
            in_front(&self.inputs(world))
                .into_iter()
                .find(|place| matches!(place, Place::Domain(_)))
                .unwrap_or_else(|| Place::Domain(host.tab_domain())),
        )
    }

    /// Whether a browser's host could change anything: not on the way out (ADR-0008), nor
    /// while the front app's own place says off and no site could say on (ADR-0012).
    fn host_matters(&self, world: &World<'_>) -> bool {
        if self.released.is_some() {
            return false;
        }
        let Some(app) = &world.context.app else {
            return true;
        };
        let site_could_say_on = self
            .indefinite
            .iter()
            .chain(&self.live.rules)
            .any(|rule| matches!(rule.place, Place::Domain(_)) && rule.says == OnOff::On);
        site_could_say_on || self.says(&Place::App(app.clone()), world) != Some(OnOff::Off)
    }

    /// What deciding reads. On the way out, only everywhere's rules still apply, short of one
    /// until a time, with no app or site rules and no stand-in (ADR-0008), though each place's
    /// look still shows its own rules.
    fn inputs<'a>(&'a self, world: &'a World<'_>) -> Inputs<'a> {
        let released = self.released.as_ref();
        Inputs {
            context: world.context,
            indefinite: if released.is_some() {
                &[]
            } else {
                &self.indefinite
            },
            kept: released.map_or(&self.live.rules, |kept| kept),
            stands_in: released.is_none() && self.stands_in(),
            scheduled_state: world
                .timeline
                .scheduled_state(world.applied_until)
                .map(OnOff::from),
            next_transition: world.timeline.next_transition(world.applied_until),
        }
    }

    /// The places from `place` on, as deciding reads them: the less specific ones in front
    /// after it, or, for a place that isn't in front, everywhere. Deciding leaves out a tab
    /// domain that says nothing, and no other domain is in front then, so the app and
    /// everywhere follow it.
    fn places_from(&self, place: &Place, world: &World<'_>) -> Vec<Place> {
        let in_front = in_front(&self.inputs(world));
        match in_front.iter().position(|front| front == place) {
            Some(index) => in_front.into_iter().skip(index).collect(),
            None if self.site_in_front(world).as_ref() == Some(place) => {
                std::iter::once(place.clone()).chain(in_front).collect()
            }
            None => vec![place.clone(), Place::Everywhere],
        }
    }
}

/// What deciding reads.
struct Inputs<'a> {
    context: &'a Context,
    /// The indefinite rules at apps and sites.
    indefinite: &'a [Rule],
    /// Every other rule.
    kept: &'a [Rule],
    /// A trial has Night Shift on, in place of the schedule (ADR-0016).
    stands_in: bool,
    scheduled_state: Option<OnOff>,
    next_transition: Option<Timestamp>,
}

fn decide(inputs: &Inputs<'_>) -> Decision {
    for place in in_front(inputs) {
        let reasons = reasons_at(inputs, &place);
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
fn in_front(inputs: &Inputs<'_>) -> Vec<Place> {
    let mut domains: Vec<Domain> = Vec::new();
    if let Some(host) = &inputs.context.host {
        let ruled = inputs
            .indefinite
            .iter()
            .chain(inputs.kept)
            .filter_map(|rule| match &rule.place {
                Place::Domain(domain) => Some(domain),
                _ => None,
            });
        for domain in ruled {
            if domain::covers(domain, host) && !domains.contains(domain) {
                domains.push(domain.clone());
            }
        }
        domains.sort_by(|a, b| {
            let length = |domain: &Domain| domain.as_str().len();
            length(b).cmp(&length(a)).then_with(|| a.cmp(b))
        });
    }
    domains
        .into_iter()
        .map(Place::Domain)
        .chain(inputs.context.app.clone().map(Place::App))
        .chain([Place::Everywhere])
        .collect()
}

/// What a place rests on, shortest first: its rules, then for everywhere the stand-in and the
/// schedule. A place with none of them says nothing.
fn reasons_at(inputs: &Inputs<'_>, place: &Place) -> Vec<Reason> {
    let mut reasons: Vec<Reason> = inputs
        .kept
        .iter()
        .filter(|rule| rule.place == *place)
        .cloned()
        .map(Reason::Rule)
        .collect();
    match place {
        Place::Everywhere => {
            if inputs.stands_in {
                reasons.push(Reason::StandIn);
            }
            if let Some(state) = inputs.scheduled_state {
                reasons.push(Reason::Schedule {
                    state,
                    until: inputs.next_transition,
                });
            }
        }
        Place::App(_) | Place::Domain(_) => {
            if let Some(rule) = inputs.indefinite.iter().find(|rule| rule.place == *place) {
                reasons.push(Reason::Rule(rule.clone()));
            }
        }
    }
    reasons
}

/// What a reason says.
pub(super) fn says(reason: &Reason) -> OnOff {
    match reason {
        Reason::Rule(rule) => rule.says,
        Reason::StandIn => OnOff::On,
        Reason::Schedule { state, .. } => *state,
    }
}

#[cfg(test)]
mod tests {
    use nightcap_protocol::{Length, SetBy};

    use super::*;
    use crate::rules::places::tests::*;

    #[test]
    fn with_nothing_in_front_saying_anything_everywhere_decides() {
        let lent = Lent::at(NIGHT).app("com.apple.Terminal");

        let decision = with_config(Vec::new()).look(&lent.world()).decision;

        assert_eq!(
            decision,
            Decision {
                state: Some(OnOff::On),
                place: Place::Everywhere,
                reasons: vec![schedule(OnOff::On, MORNING)],
            }
        );
    }

    #[test]
    fn with_the_schedule_unknown_and_nothing_else_everywhere_gives_night_shift_as_last_reported() {
        let lent = Lent::unscheduled(NIGHT);
        let places = with_config(Vec::new());

        let look = places.look(&lent.world());

        assert_eq!(
            look.decision,
            Decision {
                state: None,
                place: Place::Everywhere,
                reasons: Vec::new(),
            }
        );
        assert_eq!(look.everywhere.gives, OnOff::On);
    }

    #[test]
    fn an_app_rule_that_says_off_decides_while_its_app_is_in_front() {
        let places = with_config(vec![
            config("app:com.figma.Desktop", OnOff::Off),
            config("app:com.adobe.Photoshop", OnOff::Off),
        ]);
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");

        let decision = places.look(&lent.world()).decision;

        assert_eq!(
            decision,
            Decision {
                state: Some(OnOff::Off),
                place: place("app:com.figma.Desktop"),
                reasons: vec![Reason::Rule(config_rule(
                    "app:com.figma.Desktop",
                    OnOff::Off
                ))],
            }
        );
    }

    #[test]
    fn an_app_rule_that_says_on_turns_night_shift_on_by_day() {
        let places = with_config(vec![config("app:com.apple.iBooksX", OnOff::On)]);
        let lent = Lent::at(DAY).app("com.apple.iBooksX");

        let decision = places.look(&lent.world()).decision;

        assert_eq!(decision.state, Some(OnOff::On));
        assert_eq!(decision.place, place("app:com.apple.iBooksX"));
    }

    #[test]
    fn app_rules_match_bundle_ids_in_any_case() {
        let places = with_config(vec![config("app:com.figma.desktop", OnOff::Off)]);
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");

        let decision = places.look(&lent.world()).decision;

        assert_eq!(decision.state, Some(OnOff::Off));
    }

    #[test]
    fn a_domain_rule_decides_on_its_domain_and_subdomains() {
        let places = with_config(vec![config("domain:figma.com", OnOff::Off)]);
        let state_on = |host: &str| {
            let lent = Lent::at(NIGHT).app("com.apple.Safari").host(host);
            places.look(&lent.world()).decision.state
        };

        assert_eq!(state_on("figma.com"), Some(OnOff::Off));
        assert_eq!(state_on("www.figma.com"), Some(OnOff::Off));
        assert_eq!(state_on("notfigma.com"), Some(OnOff::On));
        assert_eq!(state_on("figma.com.example.net"), Some(OnOff::On));
    }

    #[test]
    fn a_site_decides_over_its_browsers_app_rule() {
        let places = with_config(vec![
            config("app:com.apple.Safari", OnOff::Off),
            config("domain:youtube.com", OnOff::On),
        ]);
        let decide_on = |host: &str| {
            let lent = Lent::at(DAY).app("com.apple.Safari").host(host);
            places.look(&lent.world()).decision
        };

        let decision = decide_on("www.youtube.com");
        assert_eq!(decision.state, Some(OnOff::On));
        assert_eq!(decision.place, place("domain:youtube.com"));
        let decision = decide_on("apple.com");
        assert_eq!(decision.state, Some(OnOff::Off));
        assert_eq!(decision.place, place("app:com.apple.Safari"));
    }

    #[test]
    fn the_longest_covering_domain_decides() {
        let places = with_config(vec![
            config("domain:google.com", OnOff::Off),
            config("domain:docs.google.com", OnOff::On),
        ]);
        let decide_on = |host: &str| {
            let lent = Lent::at(NIGHT).app("com.apple.Safari").host(host);
            places.look(&lent.world()).decision
        };

        assert_eq!(
            decide_on("docs.google.com").place,
            place("domain:docs.google.com")
        );
        assert_eq!(decide_on("docs.google.com").state, Some(OnOff::On));
        assert_eq!(
            decide_on("mail.google.com").place,
            place("domain:google.com")
        );
        assert_eq!(decide_on("mail.google.com").state, Some(OnOff::Off));
    }

    #[test]
    fn a_temporary_rule_on_a_site_without_an_indefinite_one_decides_on_its_subdomains() {
        let docs = place("domain:docs.google.com");
        let places = places(
            vec![config("domain:google.com", OnOff::Off)],
            kept_rules(vec![yours(docs.clone(), OnOff::On, until(EVENING))]),
        );
        let lent = Lent::at(DAY)
            .app("com.apple.Safari")
            .host("a.docs.google.com");

        let look = places.look(&lent.world());

        assert_eq!(look.decision.state, Some(OnOff::On));
        assert_eq!(look.decision.place, docs);
        assert_eq!(look.site.map(|site| site.place), Some(docs));
    }

    #[test]
    fn at_a_place_the_shortest_rule_decides() {
        let figma = config_rule("app:com.figma.Desktop", OnOff::Off);
        let until_morning = yours(
            place("app:com.figma.Desktop"),
            OnOff::On,
            until_turn(MORNING),
        );
        let decide_with = |shorter: Option<Rule>| {
            let places = places(
                vec![config("app:com.figma.Desktop", OnOff::Off)],
                kept_rules(shorter.into_iter().chain([until_morning.clone()]).collect()),
            );
            let lent = Lent::at(NIGHT).app("com.figma.Desktop");
            places.look(&lent.world()).decision
        };

        assert_eq!(
            decide_with(None),
            Decision {
                state: Some(OnOff::On),
                place: place("app:com.figma.Desktop"),
                reasons: vec![
                    Reason::Rule(until_morning.clone()),
                    Reason::Rule(figma.clone())
                ],
            }
        );
        let until_half_past = yours(
            place("app:com.figma.Desktop"),
            OnOff::Off,
            until("2026-10-16T23:30:00+02:00"),
        );
        assert_eq!(
            decide_with(Some(until_half_past.clone())),
            Decision {
                state: Some(OnOff::Off),
                place: place("app:com.figma.Desktop"),
                reasons: vec![
                    Reason::Rule(until_half_past),
                    Reason::Rule(until_morning),
                    Reason::Rule(figma),
                ],
            }
        );
    }

    #[test]
    fn everywheres_rules_beat_the_schedule() {
        let until_one = yours(
            Place::Everywhere,
            OnOff::On,
            until("2026-10-16T13:00:00+02:00"),
        );
        let indefinitely = yours(Place::Everywhere, OnOff::Off, Length::Indefinite);
        let places = places(
            Vec::new(),
            kept_rules(vec![until_one.clone(), indefinitely.clone()]),
        );
        let lent = Lent::at(DAY).app("com.apple.mail");

        let decision = places.look(&lent.world()).decision;

        assert_eq!(
            decision,
            Decision {
                state: Some(OnOff::On),
                place: Place::Everywhere,
                reasons: vec![
                    Reason::Rule(until_one),
                    Reason::Rule(indefinitely),
                    schedule(OnOff::Off, EVENING),
                ],
            }
        );
    }

    #[test]
    fn a_trial_stands_in_for_the_schedule_under_everywheres_rules() {
        let lent = Lent::at(DAY);
        let world = lent.world();
        let mut places = with_config(Vec::new());

        places.begin_trial(true);

        let look = places.look(&world);
        assert_eq!(
            look.decision.reasons,
            vec![Reason::StandIn, schedule(OnOff::Off, EVENING)]
        );
        assert_eq!(look.decision.state, Some(OnOff::On));
        assert_eq!(look.everywhere.base, Some(Reason::StandIn));
        places.switch(OnOff::Off, &world);
        assert_eq!(places.look(&world).decision.state, Some(OnOff::Off));
    }

    #[test]
    fn a_site_that_says_nothing_gives_what_its_browser_says() {
        let places = with_config(vec![
            config("app:com.apple.Safari", OnOff::Off),
            config("domain:youtube.com", OnOff::On),
        ]);
        let lent = Lent::at(NIGHT)
            .app("com.apple.Safari")
            .host("www.example.com");

        let site = places.look(&lent.world()).site.unwrap();

        assert_eq!(site.place, place("domain:example.com"));
        assert_eq!(
            site.inherits,
            Some((place("app:com.apple.Safari"), OnOff::Off))
        );
        assert_eq!(site.gives, OnOff::Off);
    }

    #[test]
    fn without_website_checking_no_site_is_in_front() {
        let places = with_config(vec![config("domain:figma.com", OnOff::Off)]);
        let lent = Lent::at(NIGHT).app("com.apple.Safari").host("figma.com");
        let world = World {
            website_checking: false,
            ..lent.world()
        };

        assert_eq!(places.look(&world).site, None);
    }

    #[test]
    fn a_look_says_what_a_trial_sets() {
        let lent = Lent::at(DAY).app("com.apple.Notes");
        let world = lent.world();
        let notes = identity("app:com.apple.Notes");
        let mut places = with_config(Vec::new());
        places.begin_trial(true);
        places.add_rule(&notes, OnOff::Off).unwrap();
        places
            .add_rule(&identity("domain:example.com"), OnOff::Off)
            .unwrap();
        places
            .set(&Place::from(notes.clone()), OnOff::On, minutes(30), &world)
            .unwrap();

        let look = places.look(&world);

        assert_eq!(look.everywhere.base, Some(Reason::StandIn));
        let trial_rule = |place: &Place| {
            look.rules
                .iter()
                .find(|(rule, _)| rule.place == *place && rule.set_by == SetBy::Trial)
        };
        let snooze = trial_rule(&Place::from(notes.clone())).and_then(|(_, snooze)| *snooze);
        assert_eq!(snooze.map(|snooze| snooze.says), Some(OnOff::On));
        assert!(look.rules.iter().any(|(rule, _)| {
            rule.set_by == SetBy::Trial && matches!(rule.place, Place::Domain(_))
        }));
        places.end_trial(lent.now);
        assert_eq!(places.look(&world).rules, Vec::new());
    }
}
