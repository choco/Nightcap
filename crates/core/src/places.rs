//! The place stack: every place's layers and the rules under them, and what Night Shift should
//! be from the most specific place in front that says something, and why (ADR-0018, ADR-0006).

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    BundleId, Decision, Domain, Layer, Length, OnOff, Place, PlaceLayers, Reason, Rule,
    RuleIdentity, RuleSource,
};

use crate::domain::{self, Host};
use crate::schedule::Timeline;

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

/// What the place stack reads but doesn't own, lent for one call.
pub(crate) struct World<'a> {
    /// The front app and host.
    pub context: &'a Context,
    pub timeline: &'a Timeline,
    /// Where the schedule is read: every transition up to here has been applied.
    pub applied_until: Timestamp,
    /// What timers count from.
    pub now: Timestamp,
    /// Night Shift as last reported, which a place gives where nothing says anything.
    pub observed: OnOff,
    /// Website checking, without which no site is in front for its row (ADR-0012).
    pub website_checking: bool,
}

/// What `state.json` and `rules.toml` keep: every place's hold and timer, and the ad-hoc rules.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Kept {
    /// At most one per place, and none empty.
    pub layers: Vec<PlaceLayers>,
    pub ad_hoc: Vec<(RuleIdentity, OnOff)>,
}

/// Every place's layers and the rules under them (ADR-0018), with what a tour found, which its
/// end puts back (ADR-0016), and the release on the way out (ADR-0008).
///
/// Nothing ends by itself: a caller expires what ended by its `now` before it changes
/// anything, so no change lands on a layer that already ended, and `look` shows the places as
/// of the last `expire`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Places {
    /// What decides now, and outside a tour what's kept.
    live: Kept,
    config: Vec<Rule>,
    /// The config's rules, then the ad-hoc ones, then the tour's: one per identity, the
    /// config's where it has one (ADR-0006).
    rules: Vec<Rule>,
    tour: Option<Tour>,
    /// On the way out, the only layers that still decide: everywhere's hold.
    released: Option<Vec<PlaceLayers>>,
}

/// A tour in progress (ADR-0016).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tour {
    /// The ad-hoc rules and every place's hold and timer as the tour found them, which its end
    /// puts back, short of what ended meanwhile, and which `state.json` and `rules.toml` keep.
    found: Kept,
    /// The rules the user added during the tour, which it holds instead of ad-hoc ones.
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
    pub everywhere: Stack,
    /// The front app's, when it has a bundle ID a rule could name.
    pub app: Option<Stack>,
    /// The site in front, while website checking knows the host: the most specific domain with
    /// a rule or layers covering it, else the tab domain (ADR-0017).
    pub site: Option<Stack>,
    /// Everywhere's hold after the switch the other way from what it gives, by the switch
    /// itself, run on a copy. The switch never leaves a timer that way.
    pub after_switch: Option<Layer>,
    /// Whether the front app's own place says off and no site could say on, so a browser's host
    /// doesn't matter (ADR-0012).
    pub browser_holds_off: bool,
    /// The config's rules, then the ad-hoc ones, then the tour's, each with its snooze: its
    /// place's top layer, where that says the other way.
    pub rules: Vec<(&'a Rule, Option<&'a Layer>)>,
    /// Every place with a hold or a timer, as they decide now.
    pub layers: &'a [PlaceLayers],
}

/// One place's stack (ADR-0018), and what it gives with the places in front after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stack {
    pub place: Place,
    pub timer: Option<Layer>,
    pub hold: Option<Layer>,
    /// Its rule, or for everywhere the tour standing in, else the schedule.
    pub base: Option<Reason>,
    /// Its own say, else the one it inherits, else Night Shift as last reported.
    pub gives: OnOff,
    /// The first less specific place in front that says something, and what.
    pub inherits: Option<(Place, OnOff)>,
    /// What it gives under its timer, if anything says something there.
    pub after_timer: Option<OnOff>,
    /// What it gives once reset: its base's say, else the one it inherits.
    pub after_reset: Option<OnOff>,
}

/// A rule an external change snoozed, which the user is told about (ADR-0018).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Snooze {
    pub rule: RuleIdentity,
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
    #[error("only everywhere holds until turned back: for good in an app or on a site is a rule")]
    ForGoodIsARule,
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
            rules: Vec::new(),
            tour: None,
            released: None,
        };
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

    /// Takes the config's rules, dropping the ad-hoc and tour rules whose identity it now has,
    /// which have graduated (ADR-0007), from what the tour's end puts back too.
    pub(crate) fn set_config(&mut self, rules: Vec<Rule>) {
        let outside = |(identity, _): &(RuleIdentity, OnOff)| {
            !rules.iter().any(|rule| rule.identity == *identity)
        };
        self.live.ad_hoc.retain(outside);
        if let Some(tour) = &mut self.tour {
            tour.found.ad_hoc.retain(outside);
            tour.rules.retain(outside);
        }
        self.config = rules;
        self.lay_out();
    }

    /// The switch (ADR-0018): takes off everywhere's layers that say otherwise, top first, and
    /// if the tour or the schedule still says otherwise, holds `to` until the user turns it
    /// back.
    pub(crate) fn switch(&mut self, to: OnOff, world: &World<'_>) {
        if self.switched(to, world) {
            tracing::info!("switched {to} everywhere until turned back");
        }
    }

    /// The switch, which `look` runs on a copy too: says whether it held `to`.
    fn switched(&mut self, to: OnOff, world: &World<'_>) -> bool {
        self.peel_everywhere(to, world);
        if self.says(&Place::Everywhere, world) == Some(to) {
            return false;
        }
        let layers = self.layers_mut(&Place::Everywhere);
        layers.timer = None;
        layers.hold = Some(Layer {
            state: to,
            until: None,
            with: None,
        });
        true
    }

    /// Puts Night Shift on or off at a place for a length (ADR-0018): a timer for a time or
    /// until a time, which gives back what's under it, and a hold otherwise, replacing the
    /// place's hold and ending its timer.
    pub(crate) fn set(
        &mut self,
        place: &Place,
        state: OnOff,
        length: Length,
        world: &World<'_>,
    ) -> Result<(), Refusal> {
        let layer = |until| Layer {
            state,
            until,
            with: None,
        };
        match length {
            Length::For(duration) => {
                if duration <= SignedDuration::ZERO {
                    return Err(Refusal::TooShort);
                }
                let until = world
                    .now
                    .checked_add(duration)
                    .map_err(|_| Refusal::TooLong(duration))?;
                self.layers_mut(place).timer = Some(layer(Some(until)));
            }
            Length::Until(until) => {
                if until <= world.now {
                    return Err(Refusal::Passed);
                }
                self.layers_mut(place).timer = Some(layer(Some(until)));
            }
            Length::UntilScheduleTurns => {
                let until = world
                    .timeline
                    .next_turn(world.applied_until, state)
                    .ok_or(Refusal::NoSchedule)?;
                self.hold(place, layer(Some(until)));
            }
            Length::UntilTurnedBack => {
                if *place != Place::Everywhere {
                    return Err(Refusal::ForGoodIsARule);
                }
                self.hold(place, layer(None));
            }
        }
        tracing::info!("{} {state} {length:?}", loggable_place(place));
        Ok(())
    }

    /// Replaces a place's hold and ends its timer. A hold an external change linked to the one
    /// replaced stays, no longer linked.
    fn hold(&mut self, place: &Place, hold: Layer) {
        let layers = self.layers_mut(place);
        layers.timer = None;
        if let Some(partner) = layers.hold.replace(hold).and_then(|replaced| replaced.with)
            && let Some(linked) = self
                .layers_at_mut(&partner)
                .and_then(|layers| layers.hold.as_mut())
                .filter(|linked| linked.with.as_ref() == Some(place))
        {
            linked.with = None;
        }
    }

    /// Takes off a place's hold and timer, which puts its rule back, or for everywhere the
    /// schedule.
    pub(crate) fn reset(&mut self, place: &Place) {
        self.clear_layers(place);
        tracing::info!("reset {}", loggable_place(place));
    }

    /// Takes an external change as flipping what the user sees (ADR-0018): takes off what the
    /// deciding place says, and holds `state` over a rule or the schedule until the schedule
    /// next turns it back, everywhere too where a rule decides and everywhere says otherwise.
    /// Gives the rule that snoozes, if one does.
    pub(crate) fn adopt(&mut self, state: OnOff, world: &World<'_>) -> Option<Snooze> {
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
            let with = places.iter().find(|other| *other != place).cloned();
            let layers = self.layers_mut(place);
            layers.timer = None;
            layers.hold = Some(Layer { state, until, with });
        }
        let rule = decision
            .reasons
            .into_iter()
            .find_map(|reason| match reason {
                Reason::Rule(rule) => Some(rule),
                _ => None,
            })?;
        tracing::info!(
            "snoozed {} after an external change",
            loggable(&rule.identity)
        );
        Some(Snooze {
            rule: rule.identity,
            state,
            until,
        })
    }

    /// Makes the user's rule for an app or domain say `says`, replacing one of theirs that said
    /// the other, and takes off the place's hold and timer; a config rule changes only in the
    /// config (ADR-0006). A rule added during the tour is the tour's, which nothing keeps past
    /// it, and a change to one of the user's own lasts only while it runs (ADR-0016).
    pub(crate) fn add_rule(&mut self, identity: &RuleIdentity, says: OnOff) -> Result<(), Refusal> {
        if let Some(rule) = self.rules.iter().find(|rule| rule.identity == *identity) {
            if rule.says == says {
                return Ok(());
            }
            if rule.source == RuleSource::Config {
                return Err(Refusal::ConfigSays {
                    identity: identity.clone(),
                    says: rule.says,
                });
            }
        }
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
        self.clear_layers(&Place::from(identity.clone()));
        self.lay_out();
        Ok(())
    }

    /// Removes the user's rule or the tour's, and the place's hold and timer; a config rule
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
        } else if self.config.iter().any(|rule| rule.identity == *identity) {
            return Err(Refusal::ConfigRule(identity.clone()));
        } else {
            return Err(Refusal::NoSuchRule(identity.clone()));
        }
        self.clear_layers(&Place::from(identity.clone()));
        self.lay_out();
        Ok(())
    }

    /// Drops every timer and hold that ended by `at`, and those the tour's end would put back
    /// likewise.
    pub(crate) fn expire(&mut self, at: Timestamp) {
        let ended = |layer: &Option<Layer>| {
            layer
                .as_ref()
                .is_some_and(|layer| layer.until.is_some_and(|until| until <= at))
        };
        for layers in &mut self.live.layers {
            if ended(&layers.timer) {
                tracing::info!("the timer {} ended", loggable_place(&layers.place));
                layers.timer = None;
            }
            if ended(&layers.hold) {
                layers.hold = None;
            }
        }
        for layers in self.tour.iter_mut().flat_map(|tour| &mut tour.found.layers) {
            if ended(&layers.timer) {
                layers.timer = None;
            }
            if ended(&layers.hold) {
                layers.hold = None;
            }
        }
        self.tidy();
    }

    /// An edited schedule ends every hold waiting on it (ADR-0018), and those the tour's end
    /// would put back likewise.
    pub(crate) fn schedule_edited(&mut self) {
        let waiting = |hold: &Option<Layer>| hold.as_ref().is_some_and(|hold| hold.until.is_some());
        for layers in self
            .live
            .layers
            .iter_mut()
            .chain(self.tour.iter_mut().flat_map(|tour| &mut tour.found.layers))
        {
            if waiting(&layers.hold) {
                layers.hold = None;
            }
        }
        self.tidy();
    }

    /// Starts a tour unless one runs, and says whether it did. What it finds waits for its end;
    /// one that stands in for the schedule sets everywhere's layers aside (ADR-0016).
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
            self.clear_layers(&Place::Everywhere);
        }
        self.lay_out();
        true
    }

    /// Puts back the user's rules and every place's hold and timer as the tour found them, short
    /// of what ran out by `now`, and drops the rules added during it.
    pub(crate) fn end_tour(&mut self, now: Timestamp) {
        let Some(tour) = self.tour.take() else {
            return;
        };
        tracing::info!("tour ended");
        self.live = tour.found;
        self.expire(now);
        self.lay_out();
    }

    /// On the way out, ends a tour, then leaves only everywhere's hold to decide: macOS keeps
    /// Night Shift where the user put it until its next transition, but nothing else of
    /// Nightcap's may outlive it (ADR-0008). What's kept stays, for the next run.
    pub(crate) fn release(&mut self, now: Timestamp) {
        self.end_tour(now);
        self.released = Some(
            self.layers_at(&Place::Everywhere)
                .and_then(|layers| layers.hold.clone())
                .map(|hold| PlaceLayers {
                    place: Place::Everywhere,
                    hold: Some(hold),
                    timer: None,
                })
                .into_iter()
                .collect(),
        );
    }

    pub(crate) fn look<'a>(&'a self, world: &World<'_>) -> Look<'a> {
        let decision = self.decide(world);
        let everywhere = self.stack(Place::Everywhere, world);
        let mut switched = self.clone();
        switched.switched(everywhere.gives.other(), world);
        Look {
            screen: decision.state.unwrap_or(world.observed),
            decision,
            app: world
                .context
                .app
                .clone()
                .map(|app| self.stack(Place::App(app), world)),
            site: self
                .site_in_front(world)
                .map(|site| self.stack(site, world)),
            after_switch: switched
                .layers_at(&Place::Everywhere)
                .and_then(|layers| layers.hold.clone()),
            everywhere,
            browser_holds_off: self.browser_holds_off(world),
            rules: self
                .rules
                .iter()
                .map(|rule| (rule, self.snooze(rule)))
                .collect(),
            layers: &self.live.layers,
        }
    }

    fn decide(&self, world: &World<'_>) -> Decision {
        decide(&self.inputs(world))
    }

    /// What a place itself says, or None when it has no layer.
    fn says(&self, place: &Place, world: &World<'_>) -> Option<OnOff> {
        layers_of(&self.inputs(world), place).first().map(says)
    }

    /// A place's layers and what it gives, from them and the places in front after it.
    fn stack(&self, place: Place, world: &World<'_>) -> Stack {
        let inputs = self.inputs(world);
        let from: Vec<(Place, Vec<Reason>)> = self
            .places_from(&place, world)
            .into_iter()
            .map(|each| {
                let reasons = layers_of(&inputs, &each);
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
        let after_timer = from.iter().enumerate().find_map(|(index, (_, reasons))| {
            reasons
                .iter()
                .find(|reason| !(index == 0 && matches!(reason, Reason::Timer(_))))
                .map(says)
        });
        let base = match &place {
            Place::Everywhere if self.stands_in() => Some(Reason::Tour),
            Place::Everywhere => inputs.scheduled_state.map(|state| Reason::Schedule {
                state,
                until: inputs.next_transition,
            }),
            Place::App(_) | Place::Domain(_) => self.rule_at(&place).cloned().map(Reason::Rule),
        };
        let after_reset = base
            .as_ref()
            .map(says)
            .or(inherits.as_ref().map(|(_, says)| *says));
        let layers = self.layers_at(&place);
        Stack {
            timer: layers.and_then(|layers| layers.timer.clone()),
            hold: layers.and_then(|layers| layers.hold.clone()),
            place,
            base,
            gives,
            inherits,
            after_timer,
            after_reset,
        }
    }

    /// Whether a tour runs that stands in for the schedule.
    fn stands_in(&self) -> bool {
        self.tour.as_ref().is_some_and(|tour| tour.stands_in)
    }

    /// The rule for a place, if one says something there.
    fn rule_at(&self, place: &Place) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|rule| Place::from(rule.identity.clone()) == *place)
    }

    /// A rule's snooze: its place's top layer, where that says the other way.
    fn snooze(&self, rule: &Rule) -> Option<&Layer> {
        self.layers_at(&Place::from(rule.identity.clone()))
            .and_then(|layers| layers.timer.as_ref().or(layers.hold.as_ref()))
            .filter(|layer| layer.state != rule.says)
    }

    fn layers_at(&self, place: &Place) -> Option<&PlaceLayers> {
        self.live
            .layers
            .iter()
            .find(|layers| layers.place == *place)
    }

    /// The site in front, while website checking knows the host: the most specific domain with
    /// a rule or layers covering it, else the tab domain (ADR-0017).
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

    /// Whether the front app's own place says off and no site could say on, so a browser's host
    /// doesn't matter (ADR-0012).
    fn browser_holds_off(&self, world: &World<'_>) -> bool {
        let Some(app) = &world.context.app else {
            return false;
        };
        let site_could_say_on =
            self.rules.iter().any(|rule| {
                matches!(rule.identity, RuleIdentity::Domain(_)) && rule.says == OnOff::On
            }) || self.live.layers.iter().any(|layers| {
                matches!(layers.place, Place::Domain(_))
                    && [&layers.hold, &layers.timer]
                        .into_iter()
                        .flatten()
                        .any(|layer| layer.state == OnOff::On)
            });
        self.says(&Place::App(app.clone()), world) == Some(OnOff::Off) && !site_could_say_on
    }

    /// What deciding reads. On the way out, only everywhere's hold still applies, with no rules
    /// and no tour (ADR-0008), though stacks still show each place's own rule and layers.
    fn inputs<'a>(&'a self, world: &'a World<'_>) -> Inputs<'a> {
        let released = self.released.as_ref();
        Inputs {
            context: world.context,
            rules: if released.is_some() { &[] } else { &self.rules },
            layers: released.map_or(&self.live.layers, |kept| kept),
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

    /// Takes off everywhere's layers that say other than `state`, top first.
    fn peel_everywhere(&mut self, state: OnOff, world: &World<'_>) {
        while self.says(&Place::Everywhere, world) != Some(state)
            && self.peel(&Place::Everywhere, state)
        {}
    }

    /// Takes off a place's top layer if it says other than `state`, with the hold an external
    /// change put beside it, and says whether it did.
    fn peel(&mut self, place: &Place, state: OnOff) -> bool {
        let Some(layers) = self.layers_at_mut(place) else {
            return false;
        };
        let taken = match &layers.timer {
            Some(timer) if timer.state == state => return false,
            Some(_) => layers.timer.take(),
            None => layers.hold.take_if(|hold| hold.state != state),
        };
        let Some(taken) = taken else {
            return false;
        };
        if let Some(with) = &taken.with {
            self.drop_hold_with(with, place);
        }
        self.tidy();
        true
    }

    /// Takes off `place`'s hold if an external change put it there beside `partner`'s.
    fn drop_hold_with(&mut self, place: &Place, partner: &Place) {
        if let Some(layers) = self.layers_at_mut(place)
            && layers
                .hold
                .as_ref()
                .is_some_and(|hold| hold.with.as_ref().is_some_and(|with| with == partner))
        {
            layers.hold = None;
        }
    }

    fn clear_layers(&mut self, place: &Place) {
        let Some(index) = self
            .live
            .layers
            .iter()
            .position(|layers| layers.place == *place)
        else {
            return;
        };
        let removed = self.live.layers.remove(index);
        if let Some(with) = removed.hold.and_then(|hold| hold.with) {
            self.drop_hold_with(&with, place);
            self.tidy();
        }
    }

    fn layers_at_mut(&mut self, place: &Place) -> Option<&mut PlaceLayers> {
        self.live
            .layers
            .iter_mut()
            .find(|layers| layers.place == *place)
    }

    /// A place's layers, made empty if it had none; `tidy` drops it again if it stays empty.
    fn layers_mut(&mut self, place: &Place) -> &mut PlaceLayers {
        let layers = &mut self.live.layers;
        match layers.iter().position(|layers| layers.place == *place) {
            #[expect(clippy::indexing_slicing, reason = "the place's layers are at `index`")]
            Some(index) => &mut layers[index],
            None => layers.push_mut(PlaceLayers {
                place: place.clone(),
                hold: None,
                timer: None,
            }),
        }
    }

    fn tidy(&mut self) {
        let empty = |layers: &PlaceLayers| layers.hold.is_none() && layers.timer.is_none();
        self.live.layers.retain(|layers| !empty(layers));
        if let Some(tour) = &mut self.tour {
            tour.found.layers.retain(|layers| !empty(layers));
        }
    }

    /// The config rules, then the ad-hoc rules, then the tour's.
    fn lay_out(&mut self) {
        let rule = |source| {
            move |(identity, says): &(RuleIdentity, OnOff)| Rule {
                identity: identity.clone(),
                source,
                says: *says,
            }
        };
        let ad_hoc = self.live.ad_hoc.iter().map(rule(RuleSource::AdHoc));
        let tour = self
            .tour
            .iter()
            .flat_map(|tour| &tour.rules)
            .map(rule(RuleSource::Tour));
        self.rules = self
            .config
            .iter()
            .cloned()
            .chain(ad_hoc)
            .chain(tour)
            .collect();
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
    rules: &'a [Rule],
    /// Every place with a hold or a timer.
    layers: &'a [PlaceLayers],
    /// A tour has Night Shift on for itself, in place of the schedule (ADR-0016).
    tour_stands_in: bool,
    scheduled_state: Option<OnOff>,
    next_transition: Option<Timestamp>,
}

fn decide(inputs: &Inputs<'_>) -> Decision {
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
fn in_front(inputs: &Inputs<'_>) -> Vec<Place> {
    let mut domains: Vec<Domain> = Vec::new();
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

/// What a place holds, top first: its timer, its hold, then its rule, or for everywhere the
/// tour and the schedule. A place with none of them says nothing.
fn layers_of(inputs: &Inputs<'_>, place: &Place) -> Vec<Reason> {
    let layers = inputs.layers.iter().find(|layers| layers.place == *place);
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
                .find(|rule| Place::from(rule.identity.clone()) == *place)
            {
                reasons.push(Reason::Rule(rule.clone()));
            }
        }
    }
    reasons
}

/// What a layer says.
fn says(reason: &Reason) -> OnOff {
    match reason {
        Reason::Timer(layer) | Reason::Hold(layer) => layer.state,
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
    use crate::schedule::{CheckedSchedule, SunTimes};

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

    fn config(text: &str, says: OnOff) -> Rule {
        Rule {
            identity: identity(text),
            source: RuleSource::Config,
            says,
        }
    }

    fn places(config: Vec<Rule>, kept: Kept) -> Places {
        let mut places = Places::new(kept);
        places.set_config(config);
        places
    }

    fn with_config(rules: Vec<Rule>) -> Places {
        places(rules, Kept::default())
    }

    fn layer(state: OnOff, until: Option<&str>) -> Layer {
        Layer {
            state,
            until: until.map(at),
            with: None,
        }
    }

    fn layered(place: Place, hold: Option<Layer>, timer: Option<Layer>) -> PlaceLayers {
        PlaceLayers { place, hold, timer }
    }

    fn kept_layers(layers: Vec<PlaceLayers>) -> Kept {
        Kept {
            layers,
            ad_hoc: Vec::new(),
        }
    }

    fn schedule(state: OnOff, until: &str) -> Reason {
        Reason::Schedule {
            state,
            until: Some(at(until)),
        }
    }

    fn minutes(count: i64) -> Length {
        Length::For(SignedDuration::from_mins(count))
    }

    /// The places' invariants: one rule per identity, and at most one place's layers per
    /// place, none empty, in what decides and in what's kept, with every link between holds
    /// named from both ends.
    fn assert_sound(places: &Places, world: &World<'_>) {
        let look = places.look(world);
        let identities: Vec<_> = look.rules.iter().map(|(rule, _)| &rule.identity).collect();
        for (index, identity) in identities.iter().enumerate() {
            assert!(
                !identities[index + 1..].contains(identity),
                "{identity} has two rules"
            );
        }
        for layers in [look.layers, &places.kept().layers[..]] {
            for (index, each) in layers.iter().enumerate() {
                assert!(
                    each.hold.is_some() || each.timer.is_some(),
                    "{each:?} is empty"
                );
                assert!(
                    !layers[index + 1..]
                        .iter()
                        .any(|other| other.place == each.place),
                    "{} has two",
                    each.place
                );
                if let Some(with) = each.hold.as_ref().and_then(|hold| hold.with.as_ref()) {
                    let back = layers
                        .iter()
                        .find(|other| other.place == *with)
                        .and_then(|partner| partner.hold.as_ref())
                        .and_then(|hold| hold.with.as_ref());
                    assert_eq!(
                        back,
                        Some(&each.place),
                        "{}'s hold names {with}, whose hold doesn't name it back",
                        each.place
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
        let figma = config("app:com.figma.Desktop", OnOff::Off);
        let places = with_config(vec![
            figma.clone(),
            config("app:com.adobe.Photoshop", OnOff::Off),
        ]);
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");

        let decision = places.look(&lent.world()).decision;

        assert_eq!(
            decision,
            Decision {
                state: Some(OnOff::Off),
                place: place("app:com.figma.Desktop"),
                reasons: vec![Reason::Rule(figma)],
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
    fn a_layer_on_a_site_without_a_rule_decides_on_its_subdomains() {
        let docs = place("domain:docs.google.com");
        let places = places(
            vec![config("domain:google.com", OnOff::Off)],
            kept_layers(vec![layered(
                docs.clone(),
                None,
                Some(layer(OnOff::On, Some(EVENING))),
            )]),
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
    fn a_places_timer_beats_its_hold_and_its_hold_beats_its_rule() {
        let figma = config("app:com.figma.Desktop", OnOff::Off);
        let hold = layer(OnOff::On, Some(MORNING));
        let decide_with = |timer: Option<Layer>| {
            let places = places(
                vec![figma.clone()],
                kept_layers(vec![layered(
                    place("app:com.figma.Desktop"),
                    Some(hold.clone()),
                    timer,
                )]),
            );
            let lent = Lent::at(NIGHT).app("com.figma.Desktop");
            places.look(&lent.world()).decision
        };

        assert_eq!(
            decide_with(None),
            Decision {
                state: Some(OnOff::On),
                place: place("app:com.figma.Desktop"),
                reasons: vec![Reason::Hold(hold.clone()), Reason::Rule(figma.clone())],
            }
        );
        let timer = layer(OnOff::Off, Some("2026-10-16T23:30:00+02:00"));
        assert_eq!(
            decide_with(Some(timer.clone())),
            Decision {
                state: Some(OnOff::Off),
                place: place("app:com.figma.Desktop"),
                reasons: vec![
                    Reason::Timer(timer),
                    Reason::Hold(hold),
                    Reason::Rule(figma),
                ],
            }
        );
    }

    #[test]
    fn everywheres_timer_and_hold_beat_the_schedule() {
        let timer = layer(OnOff::On, Some("2026-10-16T13:00:00+02:00"));
        let places = places(
            Vec::new(),
            kept_layers(vec![layered(
                Place::Everywhere,
                Some(layer(OnOff::Off, None)),
                Some(timer.clone()),
            )]),
        );
        let lent = Lent::at(DAY).app("com.apple.mail");

        let decision = places.look(&lent.world()).decision;

        assert_eq!(
            decision,
            Decision {
                state: Some(OnOff::On),
                place: Place::Everywhere,
                reasons: vec![
                    Reason::Timer(timer),
                    Reason::Hold(layer(OnOff::Off, None)),
                    schedule(OnOff::Off, EVENING),
                ],
            }
        );
    }

    #[test]
    fn the_tour_stands_in_for_the_schedule_under_everywheres_hold_and_timer() {
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

        let snooze = places.adopt(OnOff::On, &world);

        assert_eq!(
            snooze,
            Some(Snooze {
                rule: identity("app:com.figma.Desktop"),
                state: OnOff::On,
                until: Some(at(MORNING)),
            })
        );
        let look = places.look(&world);
        assert_eq!(look.decision.state, Some(OnOff::On));
        let figma = look.app.unwrap();
        assert_eq!(figma.hold.map(|hold| hold.until), Some(Some(at(MORNING))));
        assert_eq!(figma.after_reset, Some(OnOff::Off));
        assert_sound(&places, &world);
    }

    #[test]
    fn a_reset_puts_a_snoozed_rule_back_and_takes_off_the_hold_linked_to_its_snooze() {
        let lent = Lent::at(DAY).app("com.figma.Desktop");
        let world = lent.world();
        let mut places = with_config(vec![config("app:com.figma.Desktop", OnOff::Off)]);
        places.adopt(OnOff::On, &world);

        places.reset(&place("app:com.figma.Desktop"));

        let look = places.look(&world);
        assert_eq!(look.decision.state, Some(OnOff::Off));
        assert_eq!(look.layers, &[]);
    }

    #[test]
    fn linked_holds_come_and_go_in_pairs() {
        let lent = Lent::at(DAY).app("com.figma.Desktop");
        let world = lent.world();
        let figma = place("app:com.figma.Desktop");
        let snoozed = || {
            let mut places = with_config(vec![config("app:com.figma.Desktop", OnOff::Off)]);
            places.adopt(OnOff::On, &world);
            places
        };

        let linked = snoozed();
        let look = linked.look(&world);
        assert_eq!(
            look.everywhere.hold.map(|hold| hold.with),
            Some(Some(figma.clone()))
        );
        assert_eq!(
            look.app.and_then(|app| app.hold).map(|hold| hold.with),
            Some(Some(Place::Everywhere))
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
            .set(&figma, OnOff::Off, Length::UntilScheduleTurns, &world)
            .unwrap();

        for places in [&flipped_back, &switched, &ended, &edited] {
            assert_eq!(places.look(&world).layers, &[]);
        }
        assert_eq!(
            replaced.look(&world).everywhere.hold.map(|hold| hold.with),
            Some(None)
        );
        for places in [linked, flipped_back, switched, ended, edited, replaced] {
            assert_sound(&places, &world);
        }
    }

    #[test]
    fn a_place_has_one_set_of_layers_whatever_changes_it() {
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");
        let world = lent.world();
        let figma = place("app:com.figma.Desktop");
        let mut places = with_config(vec![config("app:com.figma.Desktop", OnOff::Off)]);

        places.set(&figma, OnOff::On, minutes(30), &world).unwrap();
        places
            .set(&figma, OnOff::On, Length::UntilScheduleTurns, &world)
            .unwrap();
        places.set(&figma, OnOff::Off, minutes(60), &world).unwrap();
        places.switch(OnOff::Off, &world);
        places.adopt(OnOff::On, &world);
        places
            .set(
                &Place::Everywhere,
                OnOff::Off,
                Length::UntilTurnedBack,
                &world,
            )
            .unwrap();

        assert_eq!(places.look(&world).layers.len(), 2);
        assert_sound(&places, &world);
        places.reset(&figma);
        places.reset(&Place::Everywhere);
        assert_eq!(places.look(&world).layers, &[]);
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
                layers: Vec::new(),
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
                config("app:com.apple.mail", OnOff::Off),
                config("app:io.github.choco.nightcap.tour-sample", OnOff::On),
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
            kept_layers(vec![layered(
                figma.clone(),
                None,
                Some(layer(OnOff::On, Some("2026-10-16T23:30:00+02:00"))),
            )]),
        );
        type Change<'a> = &'a dyn Fn(&mut Places) -> Result<(), Refusal>;
        let refused: [(Change<'_>, &str); 8] = [
            (
                &|places| places.set(&figma, OnOff::On, minutes(0), &world),
                "a length is longer than zero",
            ),
            (
                &|places| places.set(&figma, OnOff::On, Length::For(SignedDuration::MAX), &world),
                "is too long",
            ),
            (
                &|places| places.set(&figma, OnOff::On, Length::Until(at(DAY)), &world),
                "that time has passed",
            ),
            (
                &|places| {
                    places.set(
                        &figma,
                        OnOff::On,
                        Length::UntilScheduleTurns,
                        &unscheduled.world(),
                    )
                },
                "there's no schedule to wait for",
            ),
            (
                &|places| places.set(&figma, OnOff::On, Length::UntilTurnedBack, &world),
                "for good in an app or on a site is a rule",
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
        let late = Some(layer(OnOff::On, Some("2026-10-17T01:00:00+02:00")));
        let found = Kept {
            layers: vec![
                layered(
                    figma.clone(),
                    None,
                    Some(layer(OnOff::On, Some("2026-10-16T23:30:00+02:00"))),
                ),
                layered(
                    Place::Everywhere,
                    Some(layer(OnOff::Off, Some(MORNING))),
                    late.clone(),
                ),
            ],
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
            .set(&figma, OnOff::On, Length::UntilScheduleTurns, &world)
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

        assert_eq!(
            places.kept(),
            &Kept {
                layers: vec![layered(Place::Everywhere, None, late)],
                ad_hoc: Vec::new(),
            }
        );
        assert_sound(&places, &world);
        places.end_tour(at("2026-10-17T01:00:00+02:00"));
        assert_eq!(places.kept(), &Kept::default());
        assert_eq!(places.look(&world).rules.len(), 2);
    }

    #[test]
    fn the_tours_end_puts_back_what_it_found_short_of_what_ended() {
        let lent = Lent::at(NIGHT);
        let world = lent.world();
        let hold = layer(OnOff::Off, None);
        let mut places = places(
            Vec::new(),
            kept_layers(vec![layered(
                Place::Everywhere,
                Some(hold.clone()),
                Some(layer(OnOff::On, Some("2026-10-16T23:30:00+02:00"))),
            )]),
        );
        places.start_tour(true);
        assert_eq!(places.look(&world).layers, &[]);
        places.switch(OnOff::Off, &world);

        places.end_tour(at("2026-10-16T23:45:00+02:00"));

        let look = places.look(&world);
        assert_eq!(look.layers, &[layered(Place::Everywhere, Some(hold), None)]);
        assert_eq!(look.everywhere.base, Some(schedule(OnOff::On, MORNING)));
    }

    #[test]
    fn after_the_release_only_everywheres_hold_decides_and_everything_is_kept() {
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");
        let world = lent.world();
        let kept = kept_layers(vec![
            layered(
                place("app:com.figma.Desktop"),
                Some(layer(OnOff::On, Some(MORNING))),
                None,
            ),
            layered(
                Place::Everywhere,
                Some(layer(OnOff::Off, None)),
                Some(layer(OnOff::On, Some("2026-10-16T23:30:00+02:00"))),
            ),
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
                reasons: vec![
                    Reason::Hold(layer(OnOff::Off, None)),
                    schedule(OnOff::On, MORNING),
                ],
            }
        );
        assert_eq!(places.kept(), &kept);
    }

    #[test]
    fn the_hold_the_switch_would_leave_is_the_one_it_leaves() {
        let soon = Some("2026-10-16T23:30:00+02:00");
        let everywhere = |hold: Option<Layer>, timer: Option<Layer>| {
            kept_layers(vec![layered(Place::Everywhere, hold, timer)])
        };
        let cases = [
            (Kept::default(), false),
            (everywhere(Some(layer(OnOff::Off, None)), None), false),
            (everywhere(Some(layer(OnOff::On, None)), None), false),
            (everywhere(None, Some(layer(OnOff::Off, soon))), false),
            (
                everywhere(Some(layer(OnOff::Off, None)), Some(layer(OnOff::On, soon))),
                false,
            ),
            (
                everywhere(Some(layer(OnOff::On, None)), Some(layer(OnOff::Off, soon))),
                false,
            ),
            (Kept::default(), true),
            (everywhere(Some(layer(OnOff::Off, None)), None), true),
        ];

        for lent in [Lent::at(NIGHT), Lent::at(DAY), Lent::unscheduled(NIGHT)] {
            let world = lent.world();
            for (kept, tour) in &cases {
                let mut places = places(Vec::new(), kept.clone());
                if *tour {
                    places.start_tour(true);
                    places.switch(OnOff::Off, &world);
                }
                let look = places.look(&world);
                let (to, foreseen) = (look.everywhere.gives.other(), look.after_switch);

                places.switch(to, &world);

                let everywhere = places.look(&world).everywhere;
                assert_eq!(everywhere.hold, foreseen, "{kept:?}");
                assert_eq!(everywhere.timer, None, "{kept:?}");
                assert_eq!(everywhere.gives, to, "{kept:?}");
            }
        }
    }

    #[test]
    fn a_look_says_what_the_tour_holds() {
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
        let tour_rule = |identity: &RuleIdentity| {
            look.rules
                .iter()
                .find(|(rule, _)| rule.identity == *identity && rule.source == RuleSource::Tour)
        };
        let snooze = tour_rule(&sample).and_then(|(_, snooze)| *snooze);
        assert_eq!(snooze.map(|snooze| snooze.state), Some(OnOff::On));
        assert!(look.rules.iter().any(|(rule, _)| {
            rule.source == RuleSource::Tour && matches!(rule.identity, RuleIdentity::Domain(_))
        }));
        places.end_tour(lent.now);
        assert_eq!(places.look(&world).rules, Vec::new());
    }
}
