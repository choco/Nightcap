//! The rules at every place, and the commands that change them (ADR-0018, ADR-0006).

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    BundleId, Lasting, Length, OnOff, Place, Reason, Refusal, Rule, RuleAdded, RuleIdentity, SetBy,
};

use crate::rules::domain::Host;
use crate::rules::schedule::Timeline;
use crate::rules::trial::Trial;

pub(crate) mod look;

/// What rules match against.
#[derive(Default)]
pub(crate) struct Context {
    /// The frontmost app's bundle ID, when it has one a rule could name.
    pub app: Option<BundleId>,
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
    /// shortest first, at most one of each length.
    pub rules: Vec<Rule>,
    /// The place an outside change set a rule until the schedule turns at together with
    /// everywhere's: taking off either takes off both (ADR-0018).
    pub linked: Option<Place>,
    pub ad_hoc: Vec<(RuleIdentity, OnOff)>,
}

/// The rules at every place (ADR-0018), with what a trial found, which its end puts back
/// (ADR-0016), and the release on the way out (ADR-0008).
///
/// Nothing ends by itself: a caller expires what ended by its `now` before it changes
/// anything, so no change lands on a rule that already ended, and `look` shows the places as of
/// the last `expire`. A change that's refused leaves the places as they were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Places {
    /// What decides now, and outside a trial what's kept.
    live: Kept,
    config: Vec<(RuleIdentity, OnOff)>,
    /// The indefinite rules at apps and sites: the config's, then the ad-hoc ones, then the
    /// trial's, one per place, the config's where it has one (ADR-0006).
    indefinite: Vec<Rule>,
    trial: Option<Trial>,
    /// On the way out, the only rules that still decide: everywhere's, short of one until a
    /// time.
    released: Option<Vec<Rule>>,
}

/// What an outside change was adopted as (ADR-0018): the deciding place set to `state` until
/// the schedule turns, and the rule that snoozes, if one decided.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Adoption {
    pub place: Place,
    pub snoozed: Option<RuleIdentity>,
    pub state: OnOff,
    pub until: Option<Timestamp>,
}

impl Places {
    /// The places as `state.json` and `rules.toml` kept them, with no config rules until
    /// `set_config`.
    pub(crate) fn new(kept: Kept) -> Self {
        let mut places = Self {
            live: kept,
            config: Vec::new(),
            indefinite: Vec::new(),
            trial: None,
            released: None,
        };
        places.tidy();
        places.lay_out();
        places
    }

    /// What `state.json` and `rules.toml` keep: mid-trial, what the trial found.
    pub(crate) fn kept(&self) -> &Kept {
        self.trial.as_ref().map_or(&self.live, Trial::found)
    }

    /// Takes the config's rules, dropping the ad-hoc and trial rules for the places it now has,
    /// which have graduated (ADR-0007), from what the trial's end puts back too.
    pub(crate) fn set_config(&mut self, rules: Vec<(RuleIdentity, OnOff)>) {
        for kept in self.kept_mut() {
            kept.ad_hoc
                .retain(|(identity, _)| !rules.iter().any(|(config, _)| config == identity));
        }
        if let Some(trial) = &mut self.trial {
            trial.graduate(&rules);
        }
        self.config = rules;
        self.lay_out();
    }

    /// The switch (ADR-0018): takes off everywhere's rules that say otherwise, shortest first,
    /// and if the stand-in or the schedule still says otherwise, sets an indefinite rule of the
    /// user's saying `to`.
    pub(crate) fn switch(&mut self, to: OnOff, world: &World<'_>) {
        self.peel_everywhere(to, world);
        if self.says(&Place::Everywhere, world) == Some(to) {
            return;
        }
        self.put(Rule {
            place: Place::Everywhere,
            says: to,
            length: Length::Indefinite,
            set_by: SetBy::You,
        });
        tracing::info!("switched {to} everywhere, indefinitely");
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
                    .map_err(|_| Refusal::TooLong { length: duration })?;
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

    /// Takes an outside change as flipping what the user sees (ADR-0018): takes off what the
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
            tracing::info!("snoozed {} after an outside change", loggable(identity));
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
    /// rule changes only in the config (ADR-0006). A rule added during a trial is the trial's,
    /// which nothing keeps past it, and a change to one of the user's own lasts only while it
    /// runs (ADR-0016).
    pub(crate) fn add_rule(
        &mut self,
        identity: &RuleIdentity,
        says: OnOff,
    ) -> Result<RuleAdded, Refusal> {
        let place = Place::from(identity.clone());
        let there = self
            .indefinite_at(&place)
            .map(|rule| (rule.says, rule.set_by));
        let added = match there {
            Some((said, set_by)) if said == says => RuleAdded::AlreadySays { set_by },
            Some((said, SetBy::Config)) => {
                return Err(Refusal::ConfigSays {
                    identity: identity.clone(),
                    says: said,
                });
            }
            Some((_, set_by)) => {
                self.make_rule(identity, says);
                RuleAdded::Changed { set_by }
            }
            None => {
                self.make_rule(identity, says);
                RuleAdded::New
            }
        };
        self.clear(&place);
        self.lay_out();
        Ok(added)
    }

    /// Makes the user's rule say `says`: the ad-hoc one, else during a trial the trial's.
    fn make_rule(&mut self, identity: &RuleIdentity, says: OnOff) {
        if let Some((_, kept)) = self
            .live
            .ad_hoc
            .iter_mut()
            .find(|(kept, _)| kept == identity)
        {
            *kept = says;
            tracing::info!("ad-hoc rule {}: {says}", loggable(identity));
        } else if let Some(trial) = &mut self.trial {
            tracing::info!("trial rule {}: {says}", loggable(identity));
            trial.set_rule(identity, says);
        } else {
            self.live.ad_hoc.push((identity.clone(), says));
            tracing::info!("ad-hoc rule {}: {says}", loggable(identity));
        }
    }

    /// Removes the user's rule or a trial's, and the place's temporary rules; a config rule
    /// changes only in the config (ADR-0006).
    pub(crate) fn remove_rule(&mut self, identity: &RuleIdentity) -> Result<(), Refusal> {
        if self
            .trial
            .as_mut()
            .is_some_and(|trial| trial.remove_rule(identity))
        {
            tracing::info!("removed a trial rule: {}", loggable(identity));
        } else if let Some(index) = self
            .live
            .ad_hoc
            .iter()
            .position(|(kept, _)| kept == identity)
        {
            self.live.ad_hoc.remove(index);
            tracing::info!("removed an ad-hoc rule: {}", loggable(identity));
        } else if self.config.iter().any(|(config, _)| config == identity) {
            return Err(Refusal::ConfigRule {
                identity: identity.clone(),
            });
        } else {
            return Err(Refusal::NoSuchRule {
                identity: identity.clone(),
            });
        }
        self.clear(&Place::from(identity.clone()));
        self.lay_out();
        Ok(())
    }

    /// Takes off every rule that ended by `at`, and those a trial's end would put back
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

    /// An edited schedule ends every rule waiting on its next turn (ADR-0018), and those a
    /// trial's end would put back likewise.
    pub(crate) fn schedule_edited(&mut self) {
        let waiting =
            |rule: &Rule| matches!(rule.length, Length::UntilScheduleTurns { until: Some(_) });
        for kept in self.kept_mut() {
            kept.rules.retain(|rule| !waiting(rule));
        }
        self.tidy();
    }

    /// Begins a trial unless one runs (ADR-0016), standing in for the schedule with Night Shift
    /// on if `stands_in`. What it finds waits for its end; the stand-in sets everywhere's rules
    /// aside.
    pub(crate) fn begin_trial(&mut self, stands_in: bool) {
        if self.trial.is_some() {
            tracing::warn!("a trial began while one runs; keeping the first");
            return;
        }
        tracing::info!(
            "trial began{}",
            if stands_in {
                ", standing in for the schedule with Night Shift on"
            } else {
                ""
            }
        );
        self.trial = Some(Trial::new(self.live.clone(), stands_in));
        if stands_in {
            self.clear(&Place::Everywhere);
        }
        self.lay_out();
    }

    /// Ends a trial: puts back the ad-hoc rules and every other rule as it found them, short of
    /// what ran out by `now`, and drops the rules added during it.
    pub(crate) fn end_trial(&mut self, now: Timestamp) {
        let Some(trial) = self.trial.take() else {
            return;
        };
        tracing::info!("trial ended");
        self.live = trial.end();
        self.expire(now);
        self.lay_out();
    }

    /// On the way out, ends a trial, then leaves only everywhere's rules to decide, short of one
    /// until a time: macOS keeps Night Shift where the user put it until its next transition,
    /// but nothing else of Nightcap's may outlive it (ADR-0008). What's kept stays, for the
    /// next run.
    pub(crate) fn release(&mut self, now: Timestamp) {
        self.end_trial(now);
        self.released = Some(
            self.rules_at(&Place::Everywhere)
                .filter(|rule| !matches!(rule.length, Length::Until { .. }))
                .cloned()
                .collect(),
        );
    }

    /// The trial that runs, if one does (ADR-0016).
    pub(crate) fn trial(&self) -> Option<&Trial> {
        self.trial.as_ref()
    }

    /// Whether a trial runs that stands in for the schedule.
    fn stands_in(&self) -> bool {
        self.trial.as_ref().is_some_and(Trial::stands_in)
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

    /// What's live and what a trial's end would put back.
    fn kept_mut(&mut self) -> impl Iterator<Item = &mut Kept> {
        std::iter::once(&mut self.live).chain(self.trial.iter_mut().map(Trial::found_mut))
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

    /// The config rules, then the ad-hoc rules, then the trial's.
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
        let trial = self
            .trial
            .iter()
            .flat_map(Trial::rules)
            .map(rule(SetBy::Trial));
        self.indefinite = config.chain(ad_hoc).chain(trial).collect();
    }
}

/// Where a rule's length puts it among its place's, shortest first.
fn rank(rule: &Rule) -> u8 {
    match rule.length {
        Length::Until { .. } => 0,
        Length::UntilScheduleTurns { .. } => 1,
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

/// Whether a rule is an app's or a site's indefinite one: the config's, the user's or a
/// trial's, as the app's rule list and `nightcap rules` show them.
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

#[cfg(test)]
mod tests {
    use jiff::civil;
    use jiff::tz::TimeZone;
    use nightcap_protocol::Decision;

    use super::look::says;
    use super::*;
    use crate::rules::schedule::{CheckedSchedule, SunTimes};

    pub(super) const DAY: &str = "2026-10-16T12:00:00+02:00";
    pub(super) const NIGHT: &str = "2026-10-16T23:00:00+02:00";
    pub(super) const EVENING: &str = "2026-10-16T22:00:00+02:00";
    pub(super) const MORNING: &str = "2026-10-17T07:00:00+02:00";

    fn at(instant: &str) -> Timestamp {
        instant.parse().unwrap()
    }

    /// What the places are lent: Zurich, custom hours from 22:00 to 07:00 applied up to `now`,
    /// Night Shift last reported on, website checking on, and nothing in front.
    pub(super) struct Lent {
        context: Context,
        timeline: Timeline,
        pub(super) now: Timestamp,
    }

    impl Lent {
        pub(super) fn at(now: &str) -> Self {
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
        pub(super) fn unscheduled(now: &str) -> Self {
            let mut lent = Self::at(now);
            lent.timeline.schedule = None;
            lent
        }

        pub(super) fn app(mut self, bundle_id: &str) -> Self {
            self.context.app = Some(bundle_id.parse().unwrap());
            self
        }

        pub(super) fn host(mut self, host: &str) -> Self {
            self.context.host = Host::new(host);
            self
        }

        pub(super) fn world(&self) -> World<'_> {
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

    pub(super) fn identity(text: &str) -> RuleIdentity {
        text.parse().unwrap()
    }

    pub(super) fn place(text: &str) -> Place {
        Place::from(identity(text))
    }

    /// A config rule, as the config gives it.
    pub(super) fn config(text: &str, says: OnOff) -> (RuleIdentity, OnOff) {
        (identity(text), says)
    }

    /// A config rule, as it decides.
    pub(super) fn config_rule(text: &str, says: OnOff) -> Rule {
        Rule {
            place: place(text),
            says,
            length: Length::Indefinite,
            set_by: SetBy::Config,
        }
    }

    pub(super) fn places(config: Vec<(RuleIdentity, OnOff)>, kept: Kept) -> Places {
        let mut places = Places::new(kept);
        places.set_config(config);
        places
    }

    pub(super) fn with_config(rules: Vec<(RuleIdentity, OnOff)>) -> Places {
        places(rules, Kept::default())
    }

    /// A rule of the user's.
    pub(super) fn yours(place: Place, says: OnOff, length: Length) -> Rule {
        Rule {
            place,
            says,
            length,
            set_by: SetBy::You,
        }
    }

    /// A rule an outside change set, until the schedule turns at `until`.
    fn adopted(place: Place, says: OnOff, until: &str) -> Rule {
        Rule {
            place,
            says,
            length: until_turn(until),
            set_by: SetBy::OutsideChange,
        }
    }

    pub(super) fn until(instant: &str) -> Length {
        Length::Until { until: at(instant) }
    }

    pub(super) fn until_turn(instant: &str) -> Length {
        Length::UntilScheduleTurns {
            until: Some(at(instant)),
        }
    }

    pub(super) fn kept_rules(rules: Vec<Rule>) -> Kept {
        Kept {
            rules,
            linked: None,
            ad_hoc: Vec::new(),
        }
    }

    pub(super) fn schedule(state: OnOff, until: &str) -> Reason {
        Reason::Schedule {
            state,
            until: Some(at(until)),
        }
    }

    pub(super) fn minutes(count: i64) -> Lasting {
        Lasting::For(SignedDuration::from_mins(count))
    }

    /// The places' invariants: one indefinite rule per app or site; in what decides and in
    /// what's kept, each place's other rules together, shortest first, one of each length, and
    /// none indefinite but everywhere's; and a link only between two rules an outside change
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
        assert_eq!(
            figma.base,
            Some(Reason::Rule(config_rule(
                "app:com.figma.Desktop",
                OnOff::Off
            )))
        );
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
        let notes = identity("app:com.apple.Notes");
        let mut places = places(
            Vec::new(),
            Kept {
                rules: Vec::new(),
                linked: None,
                ad_hoc: vec![(mail.clone(), OnOff::Off)],
            },
        );
        places.begin_trial(false);
        places.add_rule(&notes, OnOff::Off).unwrap();
        places.add_rule(&mail, OnOff::On).unwrap();

        places.set_config(vec![
            config("app:com.apple.mail", OnOff::Off),
            config("app:com.apple.Notes", OnOff::On),
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
                config_rule("app:com.apple.Notes", OnOff::On),
            ]
        );
        assert_eq!(places.kept().ad_hoc, Vec::new());
        assert_sound(&places, &world);
        places.end_trial(lent.now);
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
        let refused: [(Change<'_>, Refusal); 8] = [
            (
                &|places| places.set(&figma, OnOff::On, minutes(0), &world),
                Refusal::TooShort,
            ),
            (
                &|places| places.set(&figma, OnOff::On, Lasting::For(SignedDuration::MAX), &world),
                Refusal::TooLong {
                    length: SignedDuration::MAX,
                },
            ),
            (
                &|places| places.set(&figma, OnOff::On, Lasting::Until(at(DAY)), &world),
                Refusal::Passed,
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
                Refusal::NoSchedule,
            ),
            (
                &|places| places.set(&figma, OnOff::On, Lasting::Indefinite, &world),
                Refusal::IndefiniteIsAdded,
            ),
            (
                &|places| {
                    places
                        .add_rule(&identity("app:com.figma.Desktop"), OnOff::On)
                        .map(drop)
                },
                Refusal::ConfigSays {
                    identity: identity("app:com.figma.Desktop"),
                    says: OnOff::Off,
                },
            ),
            (
                &|places| places.remove_rule(&identity("app:com.figma.Desktop")),
                Refusal::ConfigRule {
                    identity: identity("app:com.figma.Desktop"),
                },
            ),
            (
                &|places| places.remove_rule(&identity("app:com.apple.mail")),
                Refusal::NoSuchRule {
                    identity: identity("app:com.apple.mail"),
                },
            ),
        ];

        for (change, why) in refused {
            let mut places = before.clone();
            assert_eq!(change(&mut places), Err(why));
            assert_eq!(places, before);
        }
    }

    #[test]
    fn mid_trial_only_expiry_the_edited_schedule_and_the_config_reach_what_is_kept() {
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
        places.begin_trial(false);

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
        places.end_trial(at("2026-10-17T01:00:00+02:00"));
        assert_eq!(places.kept(), &Kept::default());
        assert_eq!(places.look(&world).rules.len(), 2);
    }

    #[test]
    fn a_trials_end_puts_back_what_it_found_short_of_what_ended() {
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
        places.begin_trial(true);
        assert_eq!(places.live.rules, Vec::new());
        places.switch(OnOff::Off, &world);

        places.end_trial(at("2026-10-16T23:45:00+02:00"));

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
        places.begin_trial(true);
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
    fn the_switch_keeps_everywheres_rules_that_say_its_way() {
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
            (
                vec![
                    everywhere(OnOff::On, until_turn(MORNING)),
                    everywhere(OnOff::Off, Length::Indefinite),
                ],
                false,
            ),
            (
                vec![
                    everywhere(OnOff::Off, until(soon)),
                    everywhere(OnOff::On, until_turn(MORNING)),
                    everywhere(OnOff::Off, Length::Indefinite),
                ],
                false,
            ),
            (Vec::new(), true),
            (vec![everywhere(OnOff::Off, Length::Indefinite)], true),
        ];

        for lent in [Lent::at(NIGHT), Lent::at(DAY), Lent::unscheduled(NIGHT)] {
            let world = lent.world();
            for (rules, stand_in) in &cases {
                let mut places = places(Vec::new(), kept_rules(rules.clone()));
                if *stand_in {
                    places.begin_trial(true);
                    places.switch(OnOff::Off, &world);
                }
                let before = places.look(&world).everywhere;
                let to = before.gives.other();

                places.switch(to, &world);

                // It takes off those that say otherwise, shortest first, up to the first that
                // says its way, else sets its own where the stand-in or the schedule still
                // says otherwise.
                let left: Vec<Rule> = match before.rules.iter().position(|rule| rule.says == to) {
                    Some(first) => before.rules[first..].to_vec(),
                    None if before.base.as_ref().map(says) == Some(to) => Vec::new(),
                    None => vec![yours(Place::Everywhere, to, Length::Indefinite)],
                };
                let after = places.look(&world).everywhere;
                assert_eq!(after.rules, left, "{rules:?}");
                assert_eq!(after.gives, to, "{rules:?}");
            }
        }
    }
}
