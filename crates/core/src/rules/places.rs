//! The rules at every place, and what Night Shift should be from the most specific place in front
//! that says something, and why (ADR-0018, ADR-0006).

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    BundleId, Decision, Domain, Lasting, Length, OnOff, Place, Reason, Rule, RuleIdentity, SetBy,
};

use crate::rules::domain::{self, Host};
use crate::rules::schedule::Timeline;

/// What rules match against, and the front app's name, which the menu calls it by.
#[derive(Default)]
pub(crate) struct Context {
    /// The frontmost app's bundle ID, when it has one a rule could name.
    pub app: Option<BundleId>,
    /// The frontmost app's name as the user sees it.
    pub app_name: Option<String>,
    /// The frontmost browser's host, while it's watched and known.
    pub host: Option<Host>,
}

/// What the places read but don't own, lent for one call.
pub(crate) struct World<'a> {
    /// The front app and host.
    pub context: &'a Context,
    pub timeline: &'a Timeline,
    /// Where the schedule is read: every transition up to here has been applied.
    pub applied_until: Timestamp,
    /// What rules until a time count from.
    pub now: Timestamp,
    /// Night Shift as last reported, which a place gives where nothing says anything.
    pub observed: OnOff,
    /// Website checking, without which no site is in front for its row (ADR-0012).
    pub website_checking: bool,
}

/// What `state.json` and `rules.toml` keep: every rule but the indefinite ones at apps and
/// sites, Adoption's link between two of them, and the ad-hoc rules.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Kept {
    /// Each place's temporary rules and everywhere's indefinite one, a place's together and its
    /// shortest first: at most one of each length at a place, where everywhere's indefinite rule
    /// and its rule until the schedule turns take each other's place.
    pub rules: Vec<Rule>,
    /// The place an external change set a rule until the schedule turns at together with
    /// everywhere's: taking off either takes off both (ADR-0018).
    pub linked: Option<Place>,
    pub ad_hoc: Vec<(RuleIdentity, OnOff)>,
}

/// The rules at every place (ADR-0018), with what a tour found, which its end puts back
/// (ADR-0016), and the release on the way out (ADR-0008).
///
/// Nothing ends by itself: a caller expires what ended by its `now` before it changes
/// anything, so no change lands on a rule that already ended, and `look` shows the places as of
/// the last `expire`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Places {
    /// What decides now, and outside a tour what's kept.
    live: Kept,
    config: Vec<(RuleIdentity, OnOff)>,
    /// The indefinite rules at apps and sites: the config's, then the ad-hoc ones, then the
    /// tour's, one per place, the config's where it has one (ADR-0006).
    indefinite: Vec<Rule>,
    tour: Option<Tour>,
    /// On the way out, the only rules that still decide: everywhere's, short of one until a
    /// time.
    released: Option<Vec<Rule>>,
}

/// A tour in progress (ADR-0016).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tour {
    /// The ad-hoc rules and every other rule as the tour found them, which its end puts back,
    /// short of what ended meanwhile, and which `state.json` and `rules.toml` keep.
    found: Kept,
    /// The rules the user added during the tour, which it keeps instead of ad-hoc ones.
    rules: Vec<(RuleIdentity, OnOff)>,
    /// Whether the tour has Night Shift on for itself, standing in for the schedule: it was off
    /// as the tour started.
    stands_in: bool,
}

/// Everything the places show, as of one call.
#[derive(Debug)]
pub(crate) struct Look<'a> {
    pub decision: Decision,
    /// What's on screen: the decision's state, else Night Shift as last reported.
    pub screen: OnOff,
    pub everywhere: PlaceLook,
    /// The front app's, when it has a bundle ID a rule could name.
    pub app: Option<PlaceLook>,
    /// The site in front, while website checking knows the host: the most specific domain with
    /// a rule covering it, else the tab domain (ADR-0017).
    pub site: Option<PlaceLook>,
    /// Everywhere's rules after the switch the other way from what it gives, by the switch
    /// itself, run on a copy. The switch never leaves a rule until a time.
    pub after_switch: Vec<Rule>,
    /// Whether a browser's host could change anything: not on the way out, where only
    /// everywhere's rules decide (ADR-0008), nor while the front app's own place says off and
    /// no site could say on (ADR-0012).
    pub host_matters: bool,
    /// Every rule at every place, as they decide now, each with its snooze: the shortest rule
    /// at its place, where that's shorter and says the other way. The indefinite rules at apps
    /// and sites come first: the config's, then the ad-hoc ones, then the tour's.
    pub rules: Vec<(&'a Rule, Option<&'a Rule>)>,
}

/// One place's rules (ADR-0018), and what it gives with the places in front after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlaceLook {
    pub place: Place,
    /// What a reset takes off, shortest first: its temporary rules, and everywhere's indefinite
    /// one.
    pub rules: Vec<Rule>,
    /// What a reset leaves: an app's or site's indefinite rule, or for everywhere the tour
    /// standing in, else the schedule.
    pub base: Option<Reason>,
    /// Its own say, else the one it inherits, else Night Shift as last reported.
    pub gives: OnOff,
    /// The first less specific place in front that says something, and what.
    pub inherits: Option<(Place, OnOff)>,
    /// What it gives under its rule until a time, if anything says something there.
    pub after_until: Option<OnOff>,
    /// What it gives once reset: its base's say, else the one it inherits.
    pub after_reset: Option<OnOff>,
}

/// What an external change was adopted as (ADR-0018): the deciding place set to `state` until
/// the schedule turns, and the rule that snoozes, if one decided.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Adoption {
    pub place: Place,
    pub snoozed: Option<RuleIdentity>,
    pub state: OnOff,
    pub until: Option<Timestamp>,
}

/// Why a change can't be made. A refused change leaves the places as they were.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Refusal {
    #[error("a length is longer than zero")]
    TooShort,
    #[error("{0:#} is too long")]
    TooLong(SignedDuration),
    #[error("that time has passed")]
    Passed,
    #[error("there's no schedule to wait for")]
    NoSchedule,
    #[error("only everywhere's indefinite rule is set: an app's or a site's is added as a rule")]
    IndefiniteIsAdded,
    #[error("{identity} comes from the config, which says {says}; change it there")]
    ConfigSays { identity: RuleIdentity, says: OnOff },
    #[error("{0} comes from the config; change it there")]
    ConfigRule(RuleIdentity),
    #[error("{0} isn't a rule")]
    NoSuchRule(RuleIdentity),
}

impl Places {
    /// The places as `state.json` and `rules.toml` kept them, with no config rules until
    /// `set_config`.
    pub(crate) fn new(kept: Kept) -> Self {
        let mut places = Self {
            live: kept,
            config: Vec::new(),
            indefinite: Vec::new(),
            tour: None,
            released: None,
        };
        places.tidy();
        places.lay_out();
        places
    }

    /// What `state.json` and `rules.toml` keep: mid-tour, what the tour found.
    pub(crate) fn kept(&self) -> &Kept {
        match &self.tour {
            Some(tour) => &tour.found,
            None => &self.live,
        }
    }

    /// Takes the config's rules, dropping the ad-hoc and tour rules for the places it now has,
    /// which have graduated (ADR-0007), from what the tour's end puts back too.
    pub(crate) fn set_config(&mut self, rules: Vec<(RuleIdentity, OnOff)>) {
        let outside = |(identity, _): &(RuleIdentity, OnOff)| {
            !rules.iter().any(|(config, _)| config == identity)
        };
        self.live.ad_hoc.retain(outside);
        if let Some(tour) = &mut self.tour {
            tour.found.ad_hoc.retain(outside);
            tour.rules.retain(outside);
        }
        self.config = rules;
        self.lay_out();
    }

    /// The switch (ADR-0018): takes off everywhere's rules that say otherwise, shortest first,
    /// and if the tour or the schedule still says otherwise, sets an indefinite rule of the
    /// user's saying `to`.
    pub(crate) fn switch(&mut self, to: OnOff, world: &World<'_>) {
        if self.switched(to, world) {
            tracing::info!("switched {to} everywhere, indefinitely");
        }
    }

    /// The switch, which `look` runs on a copy too: says whether it set a rule.
    fn switched(&mut self, to: OnOff, world: &World<'_>) -> bool {
        self.peel_everywhere(to, world);
        if self.says(&Place::Everywhere, world) == Some(to) {
            return false;
        }
        self.put(Rule {
            place: Place::Everywhere,
            says: to,
            length: Length::Indefinite,
            set_by: SetBy::You,
        });
        true
    }

    /// Sets a rule of the user's at a place, saying `state` for `length` (ADR-0018). It takes
    /// off the place's rules of its length and shorter.
    pub(crate) fn set(
        &mut self,
        place: &Place,
        state: OnOff,
        length: Lasting,
        world: &World<'_>,
    ) -> Result<(), Refusal> {
        let length = match length {
            Lasting::For(duration) => {
                if duration <= SignedDuration::ZERO {
                    return Err(Refusal::TooShort);
                }
                let until = world
                    .now
                    .checked_add(duration)
                    .map_err(|_| Refusal::TooLong(duration))?;
                Length::Until { until }
            }
            Lasting::Until(until) => {
                if until <= world.now {
                    return Err(Refusal::Passed);
                }
                Length::Until { until }
            }
            Lasting::UntilScheduleTurns => {
                let until = world
                    .timeline
                    .next_turn(world.applied_until, state)
                    .ok_or(Refusal::NoSchedule)?;
                Length::UntilScheduleTurns { until: Some(until) }
            }
            Lasting::Indefinite => {
                if *place != Place::Everywhere {
                    return Err(Refusal::IndefiniteIsAdded);
                }
                Length::Indefinite
            }
        };
        tracing::info!("{} {state} {length:?}", loggable_place(place));
        self.put(Rule {
            place: place.clone(),
            says: state,
            length,
            set_by: SetBy::You,
        });
        Ok(())
    }

    /// Takes off a place's temporary rules, which puts back its indefinite rule, and for
    /// everywhere its indefinite one too, which puts back the schedule.
    pub(crate) fn reset(&mut self, place: &Place) {
        self.clear(place);
        tracing::info!("reset {}", loggable_place(place));
    }

    /// Takes an external change as flipping what the user sees (ADR-0018): takes off what the
    /// deciding place says, and sets `state` until the schedule next turns it back over an
    /// indefinite rule or the schedule, everywhere too where an app or a site decides and
    /// everywhere says otherwise. Gives what it set, unless taking off what said otherwise was
    /// enough.
    pub(crate) fn adopt(&mut self, state: OnOff, world: &World<'_>) -> Option<Adoption> {
        loop {
            let decision = self.decide(world);
            if decision.state == Some(state) || !self.peel(&decision.place, state) {
                break;
            }
        }
        let decision = self.decide(world);
        if decision.state == Some(state) {
            return None;
        }
        let until = world.timeline.next_turn(world.applied_until, state);
        let deciding = decision.place;
        let mut places = vec![deciding.clone()];
        if deciding != Place::Everywhere {
            self.peel_everywhere(state, world);
            if self.says(&Place::Everywhere, world) != Some(state) {
                places.push(Place::Everywhere);
            }
        }
        for place in &places {
            self.put(Rule {
                place: place.clone(),
                says: state,
                length: Length::UntilScheduleTurns { until },
                set_by: SetBy::OutsideChange,
            });
        }
        if places.len() > 1 {
            self.live.linked = Some(deciding.clone());
        }
        let snoozed = decision
            .reasons
            .into_iter()
            .find_map(|reason| match reason {
                Reason::Rule(rule) if rule.length == Length::Indefinite => {
                    place_identity(&rule.place)
                }
                _ => None,
            });
        if let Some(identity) = &snoozed {
            tracing::info!("snoozed {} after an external change", loggable(identity));
        }
        Some(Adoption {
            place: deciding,
            snoozed,
            state,
            until,
        })
    }

    /// Makes the indefinite rule for an app or domain say `says`, and takes off the place's
    /// temporary rules, which ends its snooze (ADR-0018): the user's rule, replacing one of
    /// theirs that said the other, or the one that already says so, the config's too. A config
    /// rule changes only in the config (ADR-0006). A rule added during the tour is the tour's,
    /// which nothing keeps past it, and a change to one of the user's own lasts only while it
    /// runs (ADR-0016).
    pub(crate) fn add_rule(&mut self, identity: &RuleIdentity, says: OnOff) -> Result<(), Refusal> {
        let place = Place::from(identity.clone());
        match self.indefinite_at(&place) {
            Some(rule) if rule.says == says => {}
            Some(rule) if rule.set_by == SetBy::Config => {
                return Err(Refusal::ConfigSays {
                    identity: identity.clone(),
                    says: rule.says,
                });
            }
            _ => self.make_rule(identity, says),
        }
        self.clear(&place);
        self.lay_out();
        Ok(())
    }

    /// Makes the user's rule say `says`: the ad-hoc one, else during the tour the tour's.
    fn make_rule(&mut self, identity: &RuleIdentity, says: OnOff) {
        if let Some((_, kept)) = self
            .live
            .ad_hoc
            .iter_mut()
            .find(|(kept, _)| kept == identity)
        {
            *kept = says;
            tracing::info!("ad-hoc rule {}: {says}", loggable(identity));
        } else if let Some(tour) = &mut self.tour {
            tracing::info!("tour rule {}: {says}", loggable(identity));
            tour.rules.retain(|(rule, _)| rule != identity);
            tour.rules.push((identity.clone(), says));
        } else {
            self.live.ad_hoc.push((identity.clone(), says));
            tracing::info!("ad-hoc rule {}: {says}", loggable(identity));
        }
    }

    /// Removes the user's rule or the tour's, and the place's temporary rules; a config rule
    /// changes only in the config (ADR-0006).
    pub(crate) fn remove_rule(&mut self, identity: &RuleIdentity) -> Result<(), Refusal> {
        if let Some(tour) = &mut self.tour
            && let Some(index) = tour.rules.iter().position(|(rule, _)| rule == identity)
        {
            tour.rules.remove(index);
        } else if let Some(index) = self
            .live
            .ad_hoc
            .iter()
            .position(|(kept, _)| kept == identity)
        {
            self.live.ad_hoc.remove(index);
            tracing::info!("removed an ad-hoc rule: {}", loggable(identity));
        } else if self.config.iter().any(|(config, _)| config == identity) {
            return Err(Refusal::ConfigRule(identity.clone()));
        } else {
            return Err(Refusal::NoSuchRule(identity.clone()));
        }
        self.clear(&Place::from(identity.clone()));
        self.lay_out();
        Ok(())
    }

    /// Takes off every rule that ended by `at`, and those the tour's end would put back
    /// likewise.
    pub(crate) fn expire(&mut self, at: Timestamp) {
        let ended = |rule: &Rule| rule.length.end().is_some_and(|end| end <= at);
        for rule in self.live.rules.iter().filter(|rule| ended(rule)) {
            tracing::info!("a rule ended: {}", loggable_place(&rule.place));
        }
        for kept in self.kept_mut() {
            kept.rules.retain(|rule| !ended(rule));
        }
        self.tidy();
    }

    /// An edited schedule ends every rule waiting on its next turn (ADR-0018), and those the
    /// tour's end would put back likewise.
    pub(crate) fn schedule_edited(&mut self) {
        let waiting =
            |rule: &Rule| matches!(rule.length, Length::UntilScheduleTurns { until: Some(_) });
        for kept in self.kept_mut() {
            kept.rules.retain(|rule| !waiting(rule));
        }
        self.tidy();
    }

    /// Starts a tour unless one runs, and says whether it did. What it finds waits for its end;
    /// one that stands in for the schedule sets everywhere's rules aside (ADR-0016).
    pub(crate) fn start_tour(&mut self, stands_in: bool) -> bool {
        if self.tour.is_some() {
            return false;
        }
        self.tour = Some(Tour {
            found: self.live.clone(),
            rules: Vec::new(),
            stands_in,
        });
        if stands_in {
            self.clear(&Place::Everywhere);
        }
        self.lay_out();
        true
    }

    /// Puts back the ad-hoc rules and every other rule as the tour found them, short of what
    /// ran out by `now`, and drops the rules added during it.
    pub(crate) fn end_tour(&mut self, now: Timestamp) {
        let Some(tour) = self.tour.take() else {
            return;
        };
        tracing::info!("tour ended");
        self.live = tour.found;
        self.expire(now);
        self.lay_out();
    }

    /// On the way out, ends a tour, then leaves only everywhere's rules to decide, short of one
    /// until a time: macOS keeps Night Shift where the user put it until its next transition,
    /// but nothing else of Nightcap's may outlive it (ADR-0008). What's kept stays, for the
    /// next run.
    pub(crate) fn release(&mut self, now: Timestamp) {
        self.end_tour(now);
        self.released = Some(
            self.rules_at(&Place::Everywhere)
                .filter(|rule| !matches!(rule.length, Length::Until { .. }))
                .cloned()
                .collect(),
        );
    }

    pub(crate) fn look<'a>(&'a self, world: &World<'_>) -> Look<'a> {
        let decision = self.decide(world);
        let everywhere = self.place_look(Place::Everywhere, world);
        let mut switched = self.clone();
        switched.switched(everywhere.gives.other(), world);
        Look {
            screen: decision.state.unwrap_or(world.observed),
            decision,
            app: world
                .context
                .app
                .clone()
                .map(|app| self.place_look(Place::App(app), world)),
            site: self
                .site_in_front(world)
                .map(|site| self.place_look(site, world)),
            after_switch: switched.rules_at(&Place::Everywhere).cloned().collect(),
            everywhere,
            host_matters: self.host_matters(world),
            rules: self
                .indefinite
                .iter()
                .chain(&self.live.rules)
                .map(|rule| (rule, self.snooze(rule)))
                .collect(),
        }
    }

    fn decide(&self, world: &World<'_>) -> Decision {
        decide(&self.inputs(world))
    }

    /// What a place itself says, or None when nothing there says anything.
    fn says(&self, place: &Place, world: &World<'_>) -> Option<OnOff> {
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
        let after_until = from.iter().enumerate().find_map(|(index, (_, reasons))| {
            reasons
                .iter()
                .find(|reason| !(index == 0 && is_until(reason)))
                .map(says)
        });
        let base = match &place {
            Place::Everywhere if self.stands_in() => Some(Reason::Tour),
            Place::Everywhere => inputs.scheduled_state.map(|state| Reason::Schedule {
                state,
                until: inputs.next_transition,
            }),
            Place::App(_) | Place::Domain(_) => {
                self.indefinite_at(&place).cloned().map(Reason::Rule)
            }
        };
        let after_reset = base
            .as_ref()
            .map(says)
            .or(inherits.as_ref().map(|(_, says)| *says));
        PlaceLook {
            rules: self.rules_at(&place).cloned().collect(),
            place,
            base,
            gives,
            inherits,
            after_until,
            after_reset,
        }
    }

    /// Whether a tour runs that stands in for the schedule.
    fn stands_in(&self) -> bool {
        self.tour.as_ref().is_some_and(|tour| tour.stands_in)
    }

    /// An app's or site's indefinite rule, if it has one.
    fn indefinite_at(&self, place: &Place) -> Option<&Rule> {
        self.indefinite.iter().find(|rule| rule.place == *place)
    }

    /// A place's rules that a reset takes off, shortest first.
    fn rules_at<'a>(&'a self, place: &'a Place) -> impl Iterator<Item = &'a Rule> {
        self.live
            .rules
            .iter()
            .filter(move |rule| rule.place == *place)
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
    /// until a time, with no app or site rules and no tour (ADR-0008), though each place's look
    /// still shows its own rules.
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
            tour_stands_in: released.is_none() && self.stands_in(),
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

    /// Takes off everywhere's rules that say other than `state`, shortest first.
    fn peel_everywhere(&mut self, state: OnOff, world: &World<'_>) {
        while self.says(&Place::Everywhere, world) != Some(state)
            && self.peel(&Place::Everywhere, state)
        {}
    }

    /// Takes off a place's shortest rule that a reset would, if it says other than `state`,
    /// with the one Adoption set beside it, and says whether it did.
    fn peel(&mut self, place: &Place, state: OnOff) -> bool {
        let Some(index) = self.live.rules.iter().position(|rule| rule.place == *place) else {
            return false;
        };
        if self
            .live
            .rules
            .get(index)
            .is_some_and(|shortest| shortest.says == state)
        {
            return false;
        }
        let taken = self.live.rules.remove(index);
        if matches!(taken.length, Length::UntilScheduleTurns { .. }) {
            self.take_off_partner(place);
        }
        true
    }

    /// Takes off what a reset takes off at `place`, with the rule Adoption set beside one of
    /// them.
    fn clear(&mut self, place: &Place) {
        let until_schedule_turns = self.live.rules.iter().any(|rule| {
            rule.place == *place && matches!(rule.length, Length::UntilScheduleTurns { .. })
        });
        self.live.rules.retain(|rule| rule.place != *place);
        if until_schedule_turns {
            self.take_off_partner(place);
        }
    }

    /// Takes off the rule until the schedule turns that Adoption set beside `place`'s, once
    /// that one is gone, and the link between them.
    fn take_off_partner(&mut self, place: &Place) {
        let partner = match &self.live.linked {
            Some(linked) if linked == place => Place::Everywhere,
            Some(linked) if *place == Place::Everywhere => linked.clone(),
            _ => return,
        };
        self.live.linked = None;
        self.live.rules.retain(|rule| {
            !(rule.place == partner && matches!(rule.length, Length::UntilScheduleTurns { .. }))
        });
    }

    /// Whether Adoption linked `place`'s rule until the schedule turns to another's.
    fn linked_at(&self, place: &Place) -> bool {
        self.live
            .linked
            .as_ref()
            .is_some_and(|linked| linked == place || *place == Place::Everywhere)
    }

    /// Puts a rule among its place's, shortest first, where it takes off the place's rules of
    /// its length and shorter. Taking off one that Adoption linked leaves its partner unlinked.
    fn put(&mut self, rule: Rule) {
        let replaced = |each: &Rule| each.place == rule.place && rank(each) <= rank(&rule);
        let unlinks =
            self.live.rules.iter().any(|each| {
                replaced(each) && matches!(each.length, Length::UntilScheduleTurns { .. })
            });
        if unlinks && self.linked_at(&rule.place) {
            self.live.linked = None;
        }
        let rules = &mut self.live.rules;
        // A place's rules are together, so those before its first are other places' and stay.
        let first = rules.iter().position(|each| each.place == rule.place);
        rules.retain(|each| !replaced(each));
        rules.insert(first.unwrap_or(rules.len()), rule);
    }

    /// What's live and what the tour's end would put back.
    fn kept_mut(&mut self) -> impl Iterator<Item = &mut Kept> {
        std::iter::once(&mut self.live).chain(self.tour.iter_mut().map(|tour| &mut tour.found))
    }

    /// Drops Adoption's link once either of its rules is gone.
    fn tidy(&mut self) {
        for kept in self.kept_mut() {
            let rules = &kept.rules;
            kept.linked.take_if(|linked| {
                ![&*linked, &Place::Everywhere].into_iter().all(|end| {
                    rules.iter().any(|rule| {
                        rule.place == *end
                            && matches!(rule.length, Length::UntilScheduleTurns { .. })
                    })
                })
            });
        }
    }

    /// The config rules, then the ad-hoc rules, then the tour's.
    fn lay_out(&mut self) {
        let rule = |set_by| {
            move |(identity, says): &(RuleIdentity, OnOff)| Rule {
                place: Place::from(identity.clone()),
                says: *says,
                length: Length::Indefinite,
                set_by,
            }
        };
        let config = self.config.iter().map(rule(SetBy::Config));
        let ad_hoc = self.live.ad_hoc.iter().map(rule(SetBy::You));
        let tour = self
            .tour
            .iter()
            .flat_map(|tour| &tour.rules)
            .map(rule(SetBy::Tour));
        self.indefinite = config.chain(ad_hoc).chain(tour).collect();
    }
}

/// Where a rule's length puts it among its place's, shortest first. Everywhere's indefinite rule
/// shares its place with its rule until the schedule turns, so either replaces the other.
fn rank(rule: &Rule) -> u8 {
    match rule.length {
        Length::Until { .. } => 0,
        Length::UntilScheduleTurns { .. } => 1,
        Length::Indefinite if rule.place == Place::Everywhere => 1,
        Length::Indefinite => 2,
    }
}

/// The identity a rule for this place would have; everywhere has none.
pub(crate) fn place_identity(place: &Place) -> Option<RuleIdentity> {
    match place {
        Place::Everywhere => None,
        Place::App(bundle_id) => Some(RuleIdentity::App(bundle_id.clone())),
        Place::Domain(domain) => Some(RuleIdentity::Domain(domain.clone())),
    }
}

/// Whether a rule is an app's or a site's indefinite one: the config's, the user's or the
/// tour's, as Settings' rule list and `nightcap rules` show them.
pub(crate) fn is_app_or_site_rule(rule: &Rule) -> bool {
    rule.place != Place::Everywhere && rule.length == Length::Indefinite
}

/// A place as the log may show it: a domain is named by its kind only (ADR-0009).
fn loggable_place(place: &Place) -> String {
    match place_identity(place) {
        None => "everywhere".to_owned(),
        Some(identity) => loggable(&identity),
    }
}

/// A rule identity as the log may show it: `tracing-oslog` logs everything as public, so a
/// domain rule is named by its kind only (ADR-0009).
fn loggable(identity: &RuleIdentity) -> String {
    match identity {
        RuleIdentity::App(_) => identity.to_string(),
        RuleIdentity::Domain(_) => "a domain rule".to_owned(),
    }
}

/// What deciding reads.
struct Inputs<'a> {
    context: &'a Context,
    /// The indefinite rules at apps and sites.
    indefinite: &'a [Rule],
    /// Every other rule.
    kept: &'a [Rule],
    /// A tour has Night Shift on for itself, in place of the schedule (ADR-0016).
    tour_stands_in: bool,
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

/// What a place rests on, shortest first: its rules, then for everywhere the tour and the
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
            if let Some(rule) = inputs.indefinite.iter().find(|rule| rule.place == *place) {
                reasons.push(Reason::Rule(rule.clone()));
            }
        }
    }
    reasons
}

/// Whether a reason is a rule until a time.
fn is_until(reason: &Reason) -> bool {
    matches!(
        reason,
        Reason::Rule(Rule {
            length: Length::Until { .. },
            ..
        })
    )
}

/// What a reason says.
fn says(reason: &Reason) -> OnOff {
    match reason {
        Reason::Rule(rule) => rule.says,
        Reason::Tour => OnOff::On,
        Reason::Schedule { state, .. } => *state,
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil;
    use jiff::tz::TimeZone;

    use super::*;
    use crate::rules::schedule::{CheckedSchedule, SunTimes};

    const DAY: &str = "2026-10-16T12:00:00+02:00";
    const NIGHT: &str = "2026-10-16T23:00:00+02:00";
    const EVENING: &str = "2026-10-16T22:00:00+02:00";
    const MORNING: &str = "2026-10-17T07:00:00+02:00";

    fn at(instant: &str) -> Timestamp {
        instant.parse().unwrap()
    }

    /// What the places are lent: Zurich, custom hours from 22:00 to 07:00 applied up to `now`,
    /// Night Shift last reported on, website checking on, and nothing in front.
    struct Lent {
        context: Context,
        timeline: Timeline,
        now: Timestamp,
    }

    impl Lent {
        fn at(now: &str) -> Self {
            Self {
                context: Context::default(),
                timeline: Timeline {
                    schedule: Some(CheckedSchedule::Custom {
                        from: civil::time(22, 0, 0, 0),
                        to: civil::time(7, 0, 0, 0),
                    }),
                    sun_times: SunTimes::default(),
                    zone: TimeZone::get("Europe/Zurich").unwrap(),
                },
                now: at(now),
            }
        }

        /// With the schedule not yet reported.
        fn unscheduled(now: &str) -> Self {
            let mut lent = Self::at(now);
            lent.timeline.schedule = None;
            lent
        }

        fn app(mut self, bundle_id: &str) -> Self {
            self.context.app = Some(bundle_id.parse().unwrap());
            self
        }

        fn host(mut self, host: &str) -> Self {
            self.context.host = Host::new(host);
            self
        }

        fn world(&self) -> World<'_> {
            World {
                context: &self.context,
                timeline: &self.timeline,
                applied_until: self.now,
                now: self.now,
                observed: OnOff::On,
                website_checking: true,
            }
        }
    }

    fn identity(text: &str) -> RuleIdentity {
        text.parse().unwrap()
    }

    fn place(text: &str) -> Place {
        Place::from(identity(text))
    }

    /// A config rule, as the config gives it.
    fn config(text: &str, says: OnOff) -> (RuleIdentity, OnOff) {
        (identity(text), says)
    }

    /// A config rule, as it decides.
    fn config_rule(text: &str, says: OnOff) -> Rule {
        Rule {
            place: place(text),
            says,
            length: Length::Indefinite,
            set_by: SetBy::Config,
        }
    }

    fn places(config: Vec<(RuleIdentity, OnOff)>, kept: Kept) -> Places {
        let mut places = Places::new(kept);
        places.set_config(config);
        places
    }

    fn with_config(rules: Vec<(RuleIdentity, OnOff)>) -> Places {
        places(rules, Kept::default())
    }

    /// A rule of the user's.
    fn yours(place: Place, says: OnOff, length: Length) -> Rule {
        Rule {
            place,
            says,
            length,
            set_by: SetBy::You,
        }
    }

    /// A rule an external change set, until the schedule turns at `until`.
    fn adopted(place: Place, says: OnOff, until: &str) -> Rule {
        Rule {
            place,
            says,
            length: until_turn(until),
            set_by: SetBy::OutsideChange,
        }
    }

    fn until(instant: &str) -> Length {
        Length::Until { until: at(instant) }
    }

    fn until_turn(instant: &str) -> Length {
        Length::UntilScheduleTurns {
            until: Some(at(instant)),
        }
    }

    fn kept_rules(rules: Vec<Rule>) -> Kept {
        Kept {
            rules,
            linked: None,
            ad_hoc: Vec::new(),
        }
    }

    fn schedule(state: OnOff, until: &str) -> Reason {
        Reason::Schedule {
            state,
            until: Some(at(until)),
        }
    }

    fn minutes(count: i64) -> Lasting {
        Lasting::For(SignedDuration::from_mins(count))
    }

    /// The places' invariants: one indefinite rule per app or site; in what decides and in
    /// what's kept, each place's other rules together, shortest first, one of each length, and
    /// none indefinite but everywhere's; and a link only between two rules an external change
    /// set until the schedule turns, everywhere's and another place's.
    fn assert_sound(places: &Places, world: &World<'_>) {
        let indefinite = &places.indefinite;
        for (index, rule) in indefinite.iter().enumerate() {
            assert!(
                !indefinite[index + 1..]
                    .iter()
                    .any(|other| other.place == rule.place),
                "{} has two indefinite rules",
                rule.place
            );
        }
        places.look(world);
        for kept in [&places.live, places.kept()] {
            let rules = &kept.rules;
            for (index, rule) in rules.iter().enumerate() {
                assert!(
                    rule.length != Length::Indefinite || rule.place == Place::Everywhere,
                    "{rule:?} is kept"
                );
                let later = &rules[index + 1..];
                match later.first() {
                    Some(next) if next.place == rule.place => {
                        assert!(rank(rule) < rank(next), "{rule:?} before {next:?}");
                    }
                    _ => assert!(
                        !later.iter().any(|other| other.place == rule.place),
                        "{}'s rules are apart",
                        rule.place
                    ),
                }
            }
            if let Some(linked) = &kept.linked {
                assert_ne!(*linked, Place::Everywhere);
                for end in [linked, &Place::Everywhere] {
                    assert!(
                        rules.iter().any(|rule| rule.place == *end
                            && rule.set_by == SetBy::OutsideChange
                            && matches!(rule.length, Length::UntilScheduleTurns { .. })),
                        "{end} has no rule for the link to {linked}"
                    );
                }
            }
        }
    }

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
    fn with_the_schedule_unknown_and_nothing_else_the_screen_is_as_last_reported() {
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
        assert_eq!(look.screen, OnOff::On);
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
    fn the_tour_stands_in_for_the_schedule_under_everywheres_rules() {
        let lent = Lent::at(DAY);
        let world = lent.world();
        let mut places = with_config(Vec::new());

        places.start_tour(true);

        let look = places.look(&world);
        assert_eq!(
            look.decision.reasons,
            vec![Reason::Tour, schedule(OnOff::Off, EVENING)]
        );
        assert_eq!(look.decision.state, Some(OnOff::On));
        assert_eq!(look.everywhere.base, Some(Reason::Tour));
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
        assert_eq!(site.after_reset, Some(OnOff::Off));
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
    fn an_external_on_in_front_of_a_config_rule_snoozes_it_until_the_schedule_turns() {
        let lent = Lent::at(DAY).app("com.figma.Desktop");
        let world = lent.world();
        let mut places = with_config(vec![config("app:com.figma.Desktop", OnOff::Off)]);

        let adoption = places.adopt(OnOff::On, &world);

        assert_eq!(
            adoption,
            Some(Adoption {
                place: place("app:com.figma.Desktop"),
                snoozed: Some(identity("app:com.figma.Desktop")),
                state: OnOff::On,
                until: Some(at(MORNING)),
            })
        );
        let look = places.look(&world);
        assert_eq!(look.decision.state, Some(OnOff::On));
        let figma = look.app.unwrap();
        assert_eq!(
            figma.rules,
            vec![adopted(place("app:com.figma.Desktop"), OnOff::On, MORNING)]
        );
        assert_eq!(figma.after_reset, Some(OnOff::Off));
        assert_sound(&places, &world);
    }

    #[test]
    fn a_reset_puts_a_snoozed_rule_back_and_takes_off_the_rule_linked_to_its_snooze() {
        let lent = Lent::at(DAY).app("com.figma.Desktop");
        let world = lent.world();
        let mut places = with_config(vec![config("app:com.figma.Desktop", OnOff::Off)]);
        places.adopt(OnOff::On, &world);

        places.reset(&place("app:com.figma.Desktop"));

        let look = places.look(&world);
        assert_eq!(look.decision.state, Some(OnOff::Off));
        assert_eq!(places.live, Kept::default());
    }

    #[test]
    fn linked_rules_come_and_go_in_pairs() {
        let lent = Lent::at(DAY).app("com.figma.Desktop");
        let world = lent.world();
        let figma = place("app:com.figma.Desktop");
        let snoozed = || {
            let mut places = with_config(vec![config("app:com.figma.Desktop", OnOff::Off)]);
            places.adopt(OnOff::On, &world);
            places
        };

        let linked = snoozed();
        assert_eq!(
            linked.live,
            Kept {
                rules: vec![
                    adopted(figma.clone(), OnOff::On, MORNING),
                    adopted(Place::Everywhere, OnOff::On, MORNING),
                ],
                linked: Some(figma.clone()),
                ad_hoc: Vec::new(),
            }
        );
        let mut flipped_back = snoozed();
        flipped_back.adopt(OnOff::Off, &world);
        let mut switched = snoozed();
        switched.switch(OnOff::Off, &world);
        let mut ended = snoozed();
        ended.expire(at(MORNING));
        let mut edited = snoozed();
        edited.schedule_edited();
        let mut replaced = snoozed();
        replaced
            .set(&figma, OnOff::Off, Lasting::UntilScheduleTurns, &world)
            .unwrap();

        for places in [&flipped_back, &switched, &ended, &edited] {
            assert_eq!(places.live, Kept::default());
        }
        assert_eq!(replaced.live.linked, None);
        assert!(
            replaced
                .live
                .rules
                .contains(&adopted(Place::Everywhere, OnOff::On, MORNING))
        );
        for places in [linked, flipped_back, switched, ended, edited, replaced] {
            assert_sound(&places, &world);
        }
    }

    #[test]
    fn a_place_has_one_rule_of_each_length_whatever_changes_it() {
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");
        let world = lent.world();
        let figma = place("app:com.figma.Desktop");
        let mut places = with_config(vec![config("app:com.figma.Desktop", OnOff::Off)]);

        places.set(&figma, OnOff::On, minutes(30), &world).unwrap();
        places
            .set(&figma, OnOff::On, Lasting::UntilScheduleTurns, &world)
            .unwrap();
        places.set(&figma, OnOff::Off, minutes(60), &world).unwrap();
        places.switch(OnOff::Off, &world);
        places.adopt(OnOff::On, &world);
        places
            .set(&Place::Everywhere, OnOff::Off, Lasting::Indefinite, &world)
            .unwrap();

        assert_eq!(places.live.rules.len(), 2);
        assert_sound(&places, &world);
        places.reset(&figma);
        places.reset(&Place::Everywhere);
        assert_eq!(places.live.rules, Vec::new());
    }

    #[test]
    fn the_config_wins_and_keeps_one_rule_per_identity() {
        let lent = Lent::at(NIGHT);
        let world = lent.world();
        let mail = identity("app:com.apple.mail");
        let sample = identity("app:io.github.choco.nightcap.tour-sample");
        let mut places = places(
            Vec::new(),
            Kept {
                rules: Vec::new(),
                linked: None,
                ad_hoc: vec![(mail.clone(), OnOff::Off)],
            },
        );
        places.start_tour(false);
        places.add_rule(&sample, OnOff::Off).unwrap();
        places.add_rule(&mail, OnOff::On).unwrap();

        places.set_config(vec![
            config("app:com.apple.mail", OnOff::Off),
            config("app:io.github.choco.nightcap.tour-sample", OnOff::On),
        ]);

        let rules: Vec<_> = places
            .look(&world)
            .rules
            .into_iter()
            .map(|(rule, _)| rule.clone())
            .collect();
        assert_eq!(
            rules,
            vec![
                config_rule("app:com.apple.mail", OnOff::Off),
                config_rule("app:io.github.choco.nightcap.tour-sample", OnOff::On),
            ]
        );
        assert_eq!(places.kept().ad_hoc, Vec::new());
        assert_sound(&places, &world);
        places.end_tour(lent.now);
        assert_eq!(places.kept().ad_hoc, Vec::new());
    }

    #[test]
    fn a_refused_change_leaves_the_places_as_they_were() {
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");
        let world = lent.world();
        let unscheduled = Lent::unscheduled(NIGHT);
        let figma = place("app:com.figma.Desktop");
        let before = places(
            vec![config("app:com.figma.Desktop", OnOff::Off)],
            kept_rules(vec![yours(
                figma.clone(),
                OnOff::On,
                until("2026-10-16T23:30:00+02:00"),
            )]),
        );
        type Change<'a> = &'a dyn Fn(&mut Places) -> Result<(), Refusal>;
        let refused: [(Change<'_>, &str); 8] = [
            (
                &|places| places.set(&figma, OnOff::On, minutes(0), &world),
                "a length is longer than zero",
            ),
            (
                &|places| places.set(&figma, OnOff::On, Lasting::For(SignedDuration::MAX), &world),
                "is too long",
            ),
            (
                &|places| places.set(&figma, OnOff::On, Lasting::Until(at(DAY)), &world),
                "that time has passed",
            ),
            (
                &|places| {
                    places.set(
                        &figma,
                        OnOff::On,
                        Lasting::UntilScheduleTurns,
                        &unscheduled.world(),
                    )
                },
                "there's no schedule to wait for",
            ),
            (
                &|places| places.set(&figma, OnOff::On, Lasting::Indefinite, &world),
                "an app's or a site's is added as a rule",
            ),
            (
                &|places| places.add_rule(&identity("app:com.figma.Desktop"), OnOff::On),
                "comes from the config, which says off; change it there",
            ),
            (
                &|places| places.remove_rule(&identity("app:com.figma.Desktop")),
                "comes from the config; change it there",
            ),
            (
                &|places| places.remove_rule(&identity("app:com.apple.mail")),
                "isn't a rule",
            ),
        ];

        for (change, why) in refused {
            let mut places = before.clone();
            let refusal = change(&mut places).unwrap_err();
            assert!(refusal.to_string().ends_with(why), "{refusal}");
            assert_eq!(places, before);
        }
    }

    #[test]
    fn mid_tour_only_expiry_the_edited_schedule_and_the_config_reach_what_is_kept() {
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");
        let world = lent.world();
        let figma = place("app:com.figma.Desktop");
        let mail = identity("app:com.apple.mail");
        let late = yours(
            Place::Everywhere,
            OnOff::On,
            until("2026-10-17T01:00:00+02:00"),
        );
        let found = Kept {
            rules: vec![
                yours(figma.clone(), OnOff::On, until("2026-10-16T23:30:00+02:00")),
                late.clone(),
                yours(Place::Everywhere, OnOff::Off, until_turn(MORNING)),
            ],
            linked: None,
            ad_hoc: vec![(mail.clone(), OnOff::Off)],
        };
        let mut places = places(
            vec![config("app:com.figma.Desktop", OnOff::Off)],
            found.clone(),
        );
        places.start_tour(false);

        places.switch(OnOff::Off, &world);
        places.reset(&figma);
        places
            .set(&figma, OnOff::On, Lasting::UntilScheduleTurns, &world)
            .unwrap();
        places.remove_rule(&mail).unwrap();
        places
            .add_rule(&identity("domain:choco.me"), OnOff::Off)
            .unwrap();
        places.adopt(OnOff::Off, &world);
        assert_eq!(places.kept(), &found);
        places.expire(at("2026-10-16T23:30:00+02:00"));
        places.schedule_edited();
        places.set_config(vec![
            config("app:com.figma.Desktop", OnOff::Off),
            config("app:com.apple.mail", OnOff::On),
        ]);

        assert_eq!(places.kept(), &kept_rules(vec![late]));
        assert_sound(&places, &world);
        places.end_tour(at("2026-10-17T01:00:00+02:00"));
        assert_eq!(places.kept(), &Kept::default());
        assert_eq!(places.look(&world).rules.len(), 2);
    }

    #[test]
    fn the_tours_end_puts_back_what_it_found_short_of_what_ended() {
        let lent = Lent::at(NIGHT);
        let world = lent.world();
        let indefinitely = yours(Place::Everywhere, OnOff::Off, Length::Indefinite);
        let mut places = places(
            Vec::new(),
            kept_rules(vec![
                yours(
                    Place::Everywhere,
                    OnOff::On,
                    until("2026-10-16T23:30:00+02:00"),
                ),
                indefinitely.clone(),
            ]),
        );
        places.start_tour(true);
        assert_eq!(places.live.rules, Vec::new());
        places.switch(OnOff::Off, &world);

        places.end_tour(at("2026-10-16T23:45:00+02:00"));

        assert_eq!(places.live.rules, vec![indefinitely]);
        let look = places.look(&world);
        assert_eq!(look.everywhere.base, Some(schedule(OnOff::On, MORNING)));
    }

    #[test]
    fn after_the_release_only_everywheres_rules_short_of_one_until_a_time_decide() {
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");
        let world = lent.world();
        let indefinitely = yours(Place::Everywhere, OnOff::Off, Length::Indefinite);
        let kept = kept_rules(vec![
            yours(
                place("app:com.figma.Desktop"),
                OnOff::On,
                until_turn(MORNING),
            ),
            yours(
                Place::Everywhere,
                OnOff::On,
                until("2026-10-16T23:30:00+02:00"),
            ),
            indefinitely.clone(),
        ]);
        let mut places = places(
            vec![config("app:com.figma.Desktop", OnOff::Off)],
            kept.clone(),
        );
        places.start_tour(true);
        places.switch(OnOff::On, &world);

        places.release(lent.now);

        assert_eq!(
            places.look(&world).decision,
            Decision {
                state: Some(OnOff::Off),
                place: Place::Everywhere,
                reasons: vec![Reason::Rule(indefinitely), schedule(OnOff::On, MORNING)],
            }
        );
        assert_eq!(places.kept(), &kept);
    }

    #[test]
    fn the_rules_the_switch_would_leave_are_the_ones_it_leaves() {
        let soon = "2026-10-16T23:30:00+02:00";
        let everywhere = |says: OnOff, length: Length| yours(Place::Everywhere, says, length);
        let cases = [
            (Vec::new(), false),
            (vec![everywhere(OnOff::Off, Length::Indefinite)], false),
            (vec![everywhere(OnOff::On, Length::Indefinite)], false),
            (vec![everywhere(OnOff::Off, until(soon))], false),
            (
                vec![
                    everywhere(OnOff::On, until(soon)),
                    everywhere(OnOff::Off, Length::Indefinite),
                ],
                false,
            ),
            (
                vec![
                    everywhere(OnOff::Off, until(soon)),
                    everywhere(OnOff::On, Length::Indefinite),
                ],
                false,
            ),
            (Vec::new(), true),
            (vec![everywhere(OnOff::Off, Length::Indefinite)], true),
        ];

        for lent in [Lent::at(NIGHT), Lent::at(DAY), Lent::unscheduled(NIGHT)] {
            let world = lent.world();
            for (rules, tour) in &cases {
                let mut places = places(Vec::new(), kept_rules(rules.clone()));
                if *tour {
                    places.start_tour(true);
                    places.switch(OnOff::Off, &world);
                }
                let look = places.look(&world);
                let (to, foreseen) = (look.everywhere.gives.other(), look.after_switch);

                places.switch(to, &world);

                let everywhere = places.look(&world).everywhere;
                assert_eq!(everywhere.rules, foreseen, "{rules:?}");
                assert!(
                    !everywhere
                        .rules
                        .iter()
                        .any(|rule| matches!(rule.length, Length::Until { .. })),
                    "{rules:?}"
                );
                assert_eq!(everywhere.gives, to, "{rules:?}");
            }
        }
    }

    #[test]
    fn a_look_says_what_the_tour_sets() {
        let lent = Lent::at(DAY).app("io.github.choco.nightcap.tour-sample");
        let world = lent.world();
        let sample = identity("app:io.github.choco.nightcap.tour-sample");
        let mut places = with_config(Vec::new());
        places.start_tour(true);
        places.add_rule(&sample, OnOff::Off).unwrap();
        places
            .add_rule(&identity("domain:nightcap.choco.me"), OnOff::Off)
            .unwrap();
        places
            .set(&Place::from(sample.clone()), OnOff::On, minutes(30), &world)
            .unwrap();

        let look = places.look(&world);

        assert_eq!(look.everywhere.base, Some(Reason::Tour));
        let tour_rule = |place: &Place| {
            look.rules
                .iter()
                .find(|(rule, _)| rule.place == *place && rule.set_by == SetBy::Tour)
        };
        let snooze = tour_rule(&Place::from(sample.clone())).and_then(|(_, snooze)| *snooze);
        assert_eq!(snooze.map(|snooze| snooze.says), Some(OnOff::On));
        assert!(look.rules.iter().any(|(rule, _)| {
            rule.set_by == SetBy::Tour && matches!(rule.place, Place::Domain(_))
        }));
        places.end_tour(lent.now);
        assert_eq!(places.look(&world).rules, Vec::new());
    }
}
