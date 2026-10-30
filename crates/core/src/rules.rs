//! The rules at every place, an indefinite one and a temporary one at most: what they decide,
//! and the commands that change them (ADR-0018, ADR-0006), with what a trial found, which its end
//! puts back (ADR-0016), and the release on the way out (ADR-0008).

mod resolution;
mod set;

use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    BundleId, Decision, Front, Lasting, Length, OnOff, Place, Reason, Refusal, Rule, RuleAdded,
    RuleIdentity, SetBy, TrialState,
};

use resolution::Resolution;
use set::RuleSet;

use crate::Effect;
use crate::domain::Host;
use crate::schedule::Timeline;

/// What rules match against.
#[derive(Default)]
pub(crate) struct Context {
    /// The frontmost app's bundle ID, when it has one a rule could name.
    pub app: Option<BundleId>,
    /// The frontmost browser's host, while it's watched and known.
    pub host: Option<Host>,
}

/// What the rules read but don't own, lent for one call.
pub(crate) struct World<'a> {
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

/// The rules at every place (ADR-0018).
///
/// Nothing ends by itself: a caller expires what ended by its `now` before it changes anything,
/// so no change lands on a rule that already ended. A change that's refused leaves the places as
/// they were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rules {
    /// What decides now, and outside a trial what's kept.
    live: RuleSet,
    trial: Option<Trial>,
    /// On the way out, only everywhere's rules short of one until a time still decide: macOS
    /// keeps Night Shift where the user put it until its next transition, but nothing else of
    /// Nightcap's may outlive it (ADR-0008).
    released: bool,
}

/// A trial that runs (ADR-0016).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Trial {
    /// The rules as it found them, which `state.json` and `rules.toml` keep while it runs, and
    /// its end puts back, short of what ended meanwhile.
    found: RuleSet,
    /// It has Night Shift on, standing in for the schedule.
    stands_in: bool,
}

impl Rules {
    /// The places as `state.json` and `rules.toml` kept them, with no config rules until
    /// `set_config`. Of the rules `state.json` holds at a place, the first of each kind stays.
    pub(crate) fn new(
        kept: Vec<Rule>,
        linked: Option<Place>,
        ad_hoc: Vec<(RuleIdentity, OnOff)>,
    ) -> Self {
        let live = RuleSet::new(kept, linked, ad_hoc);
        Self {
            live,
            trial: None,
            released: false,
        }
    }

    /// The rules `state.json` keeps, every place's temporary rule then everywhere's indefinite
    /// one, with Adoption's link: mid-trial, those the trial found.
    pub(crate) fn kept(&self) -> (Vec<Rule>, Option<Place>) {
        self.found().kept()
    }

    /// The rules outside a trial: mid-trial, those it found, which its end puts back.
    fn found(&self) -> &RuleSet {
        self.trial.as_ref().map_or(&self.live, |trial| &trial.found)
    }

    /// The ad-hoc rules `rules.toml` keeps: mid-trial, those the trial found.
    pub(crate) fn ad_hoc(&self) -> Vec<(RuleIdentity, OnOff)> {
        self.found().ad_hoc()
    }

    /// The apps' and sites' indefinite rules, as they decide now.
    pub(crate) fn indefinite(&self) -> &[Rule] {
        self.live.indefinite()
    }

    /// Takes the config's rules, dropping the user's and a trial's for the places it now has,
    /// which have graduated (ADR-0007), from what the trial's end puts back too.
    pub(crate) fn set_config(&mut self, config: Vec<(RuleIdentity, OnOff)>) {
        for rules in self.kept_mut() {
            rules.set_config(&config);
        }
    }

    /// The switch (ADR-0018): takes off everywhere's temporary rule, then its indefinite one,
    /// while they say otherwise, and if the stand-in or the schedule still says otherwise, sets
    /// an indefinite rule of the user's saying `to`.
    pub(crate) fn switch(&mut self, to: OnOff, world: &World<'_>) {
        while self.live.peel(&Place::Everywhere, Some(to)) {}
        if self.says(&Place::Everywhere, world) != Some(to) {
            tracing::info!("switched {to} everywhere, indefinitely");
            self.live.set_everywhere(to);
        }
    }

    /// Sets a rule of the user's at a place, saying `state` for `length` (ADR-0018): a temporary
    /// one takes the place of the one there, whatever its length, and everywhere's indefinite one
    /// takes off its temporary one too.
    pub(crate) fn set(
        &mut self,
        place: &Place,
        state: OnOff,
        length: Lasting,
        world: &World<'_>,
    ) -> Result<(), Refusal> {
        let length = match length {
            Lasting::For(duration) if duration <= SignedDuration::ZERO => {
                return Err(Refusal::TooShort);
            }
            Lasting::For(duration) => Length::Until {
                until: world
                    .now
                    .checked_add(duration)
                    .map_err(|_| Refusal::TooLong { length: duration })?,
            },
            Lasting::Until(until) if until <= world.now => return Err(Refusal::Passed),
            Lasting::Until(until) => Length::Until { until },
            Lasting::UntilScheduleTurns => Length::UntilScheduleTurns {
                until: Some(
                    world
                        .timeline
                        .next_turn(world.applied_until, state)
                        .ok_or(Refusal::NoSchedule)?,
                ),
            },
            Lasting::Indefinite => Length::Indefinite,
        };
        self.live.set(place, state, length)
    }

    /// Takes off a place's temporary rule, which puts back its indefinite rule, and for
    /// everywhere its indefinite one too, which puts back the stand-in or the schedule.
    pub(crate) fn reset(&mut self, place: &Place) {
        tracing::info!("reset {}", loggable(place));
        self.live.reset(place);
    }

    /// Takes an outside change as flipping what the user sees (ADR-0018): takes off what the
    /// deciding place says, and sets `state` until the schedule next turns it back over an
    /// indefinite rule or the schedule, everywhere too, linked, where an app or a site decides
    /// and everywhere says otherwise. Gives what it set, unless taking off what said otherwise
    /// was enough, as the effect that tells it.
    pub(crate) fn adopt(&mut self, state: OnOff, world: &World<'_>) -> Option<Effect> {
        let mut decision = self.decide(world);
        while decision.state != Some(state) && self.live.peel(&decision.place, Some(state)) {
            decision = self.decide(world);
        }
        if decision.state == Some(state) {
            return None;
        }
        let until = world.timeline.next_turn(world.applied_until, state);
        let place = decision.place;
        self.live.adopt(&place, state, until);
        if place != Place::Everywhere {
            while self.live.peel(&Place::Everywhere, Some(state)) {}
            if self.says(&Place::Everywhere, world) != Some(state) {
                self.live.link_adoption(&place, state, until);
            }
        }
        let snoozed = decision.reasons.iter().find_map(|reason| match reason {
            Reason::Rule(rule) if rule.length == Length::Indefinite => rule.place.identity(),
            _ => None,
        });
        if snoozed.is_some() {
            tracing::info!("snoozed {} after an outside change", loggable(&place));
        }
        Some(Effect::Adopted {
            place,
            snoozed,
            state,
            until,
        })
    }

    /// Makes the indefinite rule for an app or domain say `says`, and takes off the place's
    /// temporary rule, which ends its snooze (ADR-0018): the user's rule, replacing one of
    /// theirs that said the other, or the one that already says so, the config's too. A config
    /// rule changes only in the config (ADR-0006). A rule added during a trial is the trial's,
    /// which nothing keeps past it, and a change to one of the user's own lasts only while it
    /// runs (ADR-0016).
    pub(crate) fn add_rule(
        &mut self,
        identity: &RuleIdentity,
        says: OnOff,
    ) -> Result<RuleAdded, Refusal> {
        let set_by = self.trial.as_ref().map_or(SetBy::You, |_| SetBy::Trial);
        self.live.add_rule(identity, says, set_by)
    }

    /// Removes the user's rule or a trial's, and the place's temporary rule; a config rule
    /// changes only in the config (ADR-0006).
    pub(crate) fn remove_rule(&mut self, identity: &RuleIdentity) -> Result<(), Refusal> {
        self.live.remove_rule(identity)
    }

    /// Takes off every rule that ended by `at`, and those a trial's end would put back
    /// likewise.
    pub(crate) fn expire(&mut self, at: Timestamp) {
        let ended = |rule: &Rule| rule.length.end().is_some_and(|end| end <= at);
        for rule in self.live.temporary().iter().filter(|rule| ended(rule)) {
            tracing::info!("a rule ended: {}", loggable(&rule.place));
        }
        for rules in self.kept_mut() {
            rules.expire(at);
        }
    }

    /// An edited schedule ends every rule waiting on its next turn (ADR-0018), and those a
    /// trial's end would put back likewise.
    pub(crate) fn schedule_edited(&mut self) {
        for rules in self.kept_mut() {
            rules.schedule_edited();
        }
    }

    /// Begins a trial unless one runs (ADR-0016), standing in for the schedule with Night Shift
    /// on if `stands_in`. What it finds waits for its end; the stand-in sets everywhere's rules
    /// aside.
    pub(crate) fn begin_trial(&mut self, stands_in: bool) {
        if self.trial.is_some() {
            tracing::warn!("a trial began while one runs; keeping the first");
            return;
        }
        tracing::info!("trial began, standing in: {stands_in}");
        self.trial = Some(Trial {
            found: self.live.clone(),
            stands_in,
        });
        if stands_in {
            self.reset(&Place::Everywhere);
        }
    }

    /// Ends a trial: puts back the rules as it found them, short of what ran out by `now`.
    pub(crate) fn end_trial(&mut self, now: Timestamp) {
        if let Some(trial) = self.trial.take() {
            tracing::info!("trial ended");
            self.live = trial.found;
            self.expire(now);
        }
    }

    /// On the way out, ends a trial, then leaves only everywhere's rules to decide, short of one
    /// until a time (ADR-0008). What's kept stays, for the next run.
    pub(crate) fn release(&mut self, now: Timestamp) {
        self.end_trial(now);
        self.released = true;
    }

    /// The trial that runs, if one does (ADR-0016).
    pub(crate) fn trial(&self) -> Option<TrialState> {
        self.trial.as_ref().map(|trial| TrialState {
            stands_in: trial.stands_in,
        })
    }

    /// What Night Shift should be: what the most specific place in front that says something
    /// says, and what it rests on.
    pub(crate) fn decide(&self, world: &World<'_>) -> Decision {
        self.resolve(world).decision()
    }

    /// What a place itself says, or None when nothing there says anything.
    pub(crate) fn says(&self, place: &Place, world: &World<'_>) -> Option<OnOff> {
        self.resolve(world).says(place)
    }

    /// What's in front, as `front()` answers it, with whether a browser is `watching`: each
    /// place in front with its rules and what they sit on, its own say, else the one it inherits
    /// from the first place after it that says something, else Night Shift as last reported.
    /// The site is the most specific domain with a rule covering the host, else the tab domain
    /// (ADR-0017), while website checking knows the host.
    pub(crate) fn front(&self, world: &World<'_>, watching: bool) -> Front {
        self.resolve(world).front(watching)
    }

    fn resolve(&self, world: &World<'_>) -> Resolution<'_> {
        Resolution::new(
            &self.live,
            world,
            self.trial.as_ref().is_some_and(|trial| trial.stands_in),
            self.released,
        )
    }

    /// What the rules query answers, as the rules decide now: the apps' and sites' indefinite
    /// rules, the config's, then the ad-hoc ones, then a trial's, each with its snooze, its
    /// place's temporary rule where that says the other way; and every place's temporary rule.
    pub(crate) fn rules(&self) -> nightcap_protocol::Rules {
        self.live.listed()
    }

    /// When each rule with an end ends.
    pub(crate) fn ends(&self) -> impl Iterator<Item = Timestamp> {
        self.live
            .temporary()
            .iter()
            .filter_map(|rule| rule.length.end())
    }

    /// Whether the forces are released, on the way out, where only everywhere's rules still
    /// decide (ADR-0008).
    pub(crate) fn released(&self) -> bool {
        self.released
    }

    /// What's live and what a trial's end would put back.
    fn kept_mut(&mut self) -> impl Iterator<Item = &mut RuleSet> {
        std::iter::once(&mut self.live).chain(self.trial.iter_mut().map(|trial| &mut trial.found))
    }
}

fn rule(place: Place, says: OnOff, length: Length, set_by: SetBy) -> Rule {
    Rule {
        place,
        says,
        length,
        set_by,
    }
}

/// An app's or a site's indefinite rule.
fn indefinite(identity: RuleIdentity, says: OnOff, set_by: SetBy) -> Rule {
    rule(Place::from(identity), says, Length::Indefinite, set_by)
}

fn is_turn(rule: &Rule) -> bool {
    matches!(rule.length, Length::UntilScheduleTurns { .. })
}

/// A place as the log may show it: `tracing-oslog` logs everything as public, so a domain is
/// named by its kind only (ADR-0009).
fn loggable(place: &Place) -> String {
    match place {
        Place::Domain(_) => "a domain".to_owned(),
        Place::Everywhere | Place::App(_) => place.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil;
    use jiff::tz::TimeZone;
    use nightcap_protocol::PlaceInFront;

    use super::*;
    use crate::schedule::{CheckedSchedule, SunTimes};

    const DAY: &str = "2026-10-16T12:00:00+02:00";
    const NIGHT: &str = "2026-10-16T23:00:00+02:00";
    const HALF_PAST: &str = "2026-10-16T23:30:00+02:00";
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
    fn config_rule(text: &str, says: OnOff) -> Reason {
        Reason::Rule(indefinite(identity(text), says, SetBy::Config))
    }

    fn places(config: Vec<(RuleIdentity, OnOff)>, temporary: Vec<Rule>) -> Rules {
        let mut places = Rules::new(temporary, None, Vec::new());
        places.set_config(config);
        places
    }

    fn yours(place: Place, says: OnOff, length: Length) -> Rule {
        rule(place, says, length, SetBy::You)
    }

    /// A rule an outside change set, until the schedule turns at `until`.
    fn adopted(place: Place, says: OnOff, until: &str) -> Rule {
        rule(place, says, until_turn(until), SetBy::OutsideChange)
    }

    fn until(instant: &str) -> Length {
        Length::Until { until: at(instant) }
    }

    fn until_turn(instant: &str) -> Length {
        Length::UntilScheduleTurns {
            until: Some(at(instant)),
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

    /// What `state.json` and `rules.toml` keep.
    fn kept(places: &Rules) -> (Vec<Rule>, Option<Place>, Vec<(RuleIdentity, OnOff)>) {
        let (rules, linked) = places.kept();
        (rules, linked, places.ad_hoc())
    }

    /// The places' invariants, in what decides and in what a trial's end puts back: an
    /// indefinite rule and a temporary one at most at each place, everywhere's the switch's; and
    /// a link only between two rules an outside change set until the schedule turns,
    /// everywhere's and another place's.
    fn assert_sound(places: &Rules) {
        places.live.assert_sound();
        places.found().assert_sound();
    }

    #[test]
    fn the_most_specific_place_in_front_that_says_something_decides_by_its_temporary_rule() {
        let safari = || {
            let youtube = config("domain:youtube.com", OnOff::On);
            places(
                vec![config("app:com.apple.Safari", OnOff::Off), youtube],
                Vec::new(),
            )
        };
        let google = config("domain:google.com", OnOff::Off);
        let docs = || place("domain:docs.google.com");
        let figma = place("app:com.figma.Desktop");
        let morning = yours(figma.clone(), OnOff::On, until_turn(MORNING));
        let half_past = yours(figma.clone(), OnOff::Off, until(HALF_PAST));
        let in_figma =
            |temporary| places(vec![config("app:com.figma.Desktop", OnOff::Off)], temporary);
        let until_one = yours(
            Place::Everywhere,
            OnOff::On,
            until("2026-10-16T13:00:00+02:00"),
        );
        let indefinitely = yours(Place::Everywhere, OnOff::Off, Length::Indefinite);
        let mut standing_in = places(Vec::new(), Vec::new());
        standing_in.begin_trial(true);
        let in_safari = |now, host| Lent::at(now).app("com.apple.Safari").host(host);
        let cases = [
            (
                safari(),
                in_safari(DAY, "www.youtube.com"),
                Some("youtube.com"),
                place("domain:youtube.com"),
                vec![config_rule("domain:youtube.com", OnOff::On)],
            ),
            (
                safari(),
                in_safari(DAY, "apple.com"),
                Some("apple.com"),
                place("app:com.apple.Safari"),
                vec![config_rule("app:com.apple.Safari", OnOff::Off)],
            ),
            (
                places(vec![google.clone()], Vec::new()),
                in_safari(NIGHT, "mail.google.com"),
                Some("google.com"),
                place("domain:google.com"),
                vec![config_rule("domain:google.com", OnOff::Off)],
            ),
            (
                places(
                    vec![google.clone(), config("domain:docs.google.com", OnOff::On)],
                    Vec::new(),
                ),
                in_safari(NIGHT, "docs.google.com"),
                Some("docs.google.com"),
                docs(),
                vec![config_rule("domain:docs.google.com", OnOff::On)],
            ),
            (
                places(vec![google], vec![yours(docs(), OnOff::On, until(EVENING))]),
                in_safari(DAY, "a.docs.google.com"),
                Some("docs.google.com"),
                docs(),
                vec![Reason::Rule(yours(docs(), OnOff::On, until(EVENING)))],
            ),
            (
                in_figma(vec![morning.clone()]),
                Lent::at(NIGHT).app("com.figma.Desktop"),
                None,
                figma.clone(),
                vec![
                    Reason::Rule(morning.clone()),
                    config_rule("app:com.figma.Desktop", OnOff::Off),
                ],
            ),
            (
                // Of two temporary rules `state.json` holds at a place, the first stays.
                in_figma(vec![half_past.clone(), morning]),
                Lent::at(NIGHT).app("com.figma.Desktop"),
                None,
                figma,
                vec![
                    Reason::Rule(half_past),
                    config_rule("app:com.figma.Desktop", OnOff::Off),
                ],
            ),
            (
                places(Vec::new(), vec![until_one.clone(), indefinitely.clone()]),
                Lent::at(DAY).app("com.apple.mail"),
                None,
                Place::Everywhere,
                vec![
                    Reason::Rule(until_one),
                    Reason::Rule(indefinitely),
                    schedule(OnOff::Off, EVENING),
                ],
            ),
            (
                standing_in.clone(),
                Lent::at(DAY),
                None,
                Place::Everywhere,
                vec![Reason::StandIn, schedule(OnOff::Off, EVENING)],
            ),
            (
                places(Vec::new(), Vec::new()),
                Lent::unscheduled(NIGHT),
                None,
                Place::Everywhere,
                Vec::new(),
            ),
        ];

        for (places, lent, site, deciding, reasons) in cases {
            let front = places.front(&lent.world(), false);
            assert_eq!(front.site, site.map(|site| site.parse().unwrap()));
            assert_eq!(places.decide(&lent.world()), front.decision);
            assert_eq!(
                front.decision,
                Decision {
                    state: reasons.first().map(Reason::says),
                    place: deciding,
                    reasons,
                }
            );
        }
        let unscheduled = Lent::unscheduled(NIGHT);
        let nothing = places(Vec::new(), Vec::new());
        assert_eq!(
            nothing.front(&unscheduled.world(), false).everywhere.gives,
            OnOff::On
        );
        let day = Lent::at(DAY);
        let everywhere = standing_in.front(&day.world(), false).everywhere;
        assert_eq!(everywhere.base, Some(Reason::StandIn));
    }

    #[test]
    fn linked_rules_come_and_go_in_pairs() {
        let lent = Lent::at(DAY).app("com.figma.Desktop");
        let world = lent.world();
        let figma = place("app:com.figma.Desktop");
        let snoozed = || {
            let mut places = places(
                vec![config("app:com.figma.Desktop", OnOff::Off)],
                Vec::new(),
            );
            places.adopt(OnOff::On, &world);
            places
        };

        let linked = snoozed();
        assert_eq!(
            kept(&linked),
            (
                vec![
                    adopted(figma.clone(), OnOff::On, MORNING),
                    adopted(Place::Everywhere, OnOff::On, MORNING),
                ],
                Some(figma.clone()),
                Vec::new(),
            )
        );
        let mut flipped_back = snoozed();
        flipped_back.adopt(OnOff::Off, &world);
        let mut switched = snoozed();
        switched.switch(OnOff::Off, &world);
        let mut ended = snoozed();
        ended.expire(at(MORNING));
        let mut edited = snoozed();
        edited.schedule_edited();
        let mut reset = snoozed();
        reset.reset(&figma);
        let mut replaced = snoozed();
        replaced
            .set(&figma, OnOff::Off, Lasting::UntilScheduleTurns, &world)
            .unwrap();

        for places in [&flipped_back, &switched, &ended, &edited, &reset] {
            assert_eq!(kept(places), (Vec::new(), None, Vec::new()));
        }
        assert_eq!(reset.decide(&world).state, Some(OnOff::Off));
        assert_eq!(
            kept(&replaced),
            (
                vec![
                    yours(figma.clone(), OnOff::Off, until_turn(EVENING)),
                    adopted(Place::Everywhere, OnOff::On, MORNING),
                ],
                None,
                Vec::new(),
            )
        );
        for places in [
            linked,
            flipped_back,
            switched,
            ended,
            edited,
            reset,
            replaced,
        ] {
            assert_sound(&places);
        }
    }

    #[test]
    fn a_place_has_one_temporary_rule_whatever_changes_it() {
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");
        let world = lent.world();
        let figma = place("app:com.figma.Desktop");
        let mut places = places(
            vec![config("app:com.figma.Desktop", OnOff::Off)],
            Vec::new(),
        );

        places.set(&figma, OnOff::On, minutes(30), &world).unwrap();
        places
            .set(&figma, OnOff::On, Lasting::UntilScheduleTurns, &world)
            .unwrap();
        places.set(&figma, OnOff::Off, minutes(60), &world).unwrap();
        places
            .set(&Place::Everywhere, OnOff::Off, minutes(30), &world)
            .unwrap();
        places.switch(OnOff::Off, &world);
        places.adopt(OnOff::On, &world);
        places
            .set(&Place::Everywhere, OnOff::Off, Lasting::Indefinite, &world)
            .unwrap();

        assert_eq!(
            kept(&places).0,
            vec![
                adopted(figma.clone(), OnOff::On, MORNING),
                yours(Place::Everywhere, OnOff::Off, Length::Indefinite),
            ]
        );
        assert_sound(&places);
        places.reset(&figma);
        places.reset(&Place::Everywhere);
        assert_eq!(kept(&places).0, Vec::new());
    }

    #[test]
    fn a_refused_change_leaves_the_places_as_they_were() {
        let lent = Lent::at(NIGHT).app("com.figma.Desktop");
        let world = lent.world();
        let unscheduled = Lent::unscheduled(NIGHT);
        let figma = place("app:com.figma.Desktop");
        let before = places(
            vec![config("app:com.figma.Desktop", OnOff::Off)],
            vec![yours(figma.clone(), OnOff::On, until(HALF_PAST))],
        );
        type Change<'a> = &'a dyn Fn(&mut Rules) -> Result<(), Refusal>;
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
        let found = vec![
            yours(figma.clone(), OnOff::On, until(HALF_PAST)),
            late.clone(),
            yours(
                place("app:com.apple.Notes"),
                OnOff::Off,
                until_turn(MORNING),
            ),
        ];
        let mut places = Rules::new(found.clone(), None, vec![(mail.clone(), OnOff::Off)]);
        places.set_config(vec![config("app:com.figma.Desktop", OnOff::Off)]);
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
        assert_eq!(kept(&places), (found, None, vec![(mail, OnOff::Off)]));
        places.expire(at(HALF_PAST));
        places.schedule_edited();
        places.set_config(vec![
            config("app:com.figma.Desktop", OnOff::Off),
            config("app:com.apple.mail", OnOff::On),
        ]);

        assert_eq!(kept(&places), (vec![late], None, Vec::new()));
        assert_sound(&places);
        places.end_trial(at("2026-10-17T01:00:00+02:00"));
        assert_eq!(kept(&places), (Vec::new(), None, Vec::new()));
        assert_eq!(places.rules().rules.len(), 2);
    }

    #[test]
    fn the_switch_keeps_everywheres_rules_that_say_its_way() {
        let everywhere = |says: OnOff, length: Length| yours(Place::Everywhere, says, length);
        let cases = [
            (Vec::new(), false),
            (vec![everywhere(OnOff::Off, Length::Indefinite)], false),
            (vec![everywhere(OnOff::On, Length::Indefinite)], false),
            (vec![everywhere(OnOff::Off, until(HALF_PAST))], false),
            (vec![everywhere(OnOff::On, until_turn(MORNING))], false),
            (
                vec![
                    everywhere(OnOff::On, until(HALF_PAST)),
                    everywhere(OnOff::Off, Length::Indefinite),
                ],
                false,
            ),
            (
                vec![
                    everywhere(OnOff::Off, until(HALF_PAST)),
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
            (Vec::new(), true),
            (vec![everywhere(OnOff::Off, Length::Indefinite)], true),
        ];

        for lent in [Lent::at(NIGHT), Lent::at(DAY), Lent::unscheduled(NIGHT)] {
            let world = lent.world();
            for (rules, stand_in) in &cases {
                let mut places = places(Vec::new(), rules.clone());
                if *stand_in {
                    places.begin_trial(true);
                    places.switch(OnOff::Off, &world);
                }
                let before = places.front(&world, false).everywhere;
                let to = before.gives.other();

                places.switch(to, &world);

                // It takes off its temporary rule, then its indefinite one, up to the first that
                // says its way, else sets its own where the stand-in or the schedule still says
                // otherwise.
                let rules_of = |at: &PlaceInFront| -> Vec<Rule> {
                    at.temporary.iter().chain(&at.indefinite).cloned().collect()
                };
                let rules_before = rules_of(&before);
                let left: Vec<Rule> = match rules_before.iter().position(|rule| rule.says == to) {
                    Some(first) => rules_before[first..].to_vec(),
                    None if before.base.as_ref().map(Reason::says) == Some(to) => Vec::new(),
                    None => vec![yours(Place::Everywhere, to, Length::Indefinite)],
                };
                let after = places.front(&world, false).everywhere;
                assert_eq!(rules_of(&after), left, "{rules:?}");
                assert_eq!(after.gives, to, "{rules:?}");
            }
        }
    }
}
