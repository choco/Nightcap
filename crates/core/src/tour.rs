//! What the tour's steps follow (ADR-0016), from the live look at the places.

use jiff::Timestamp;
use nightcap_protocol::{BundleId, Domain, NightShift, OnOff, Place, Reason, RuleIdentity};

use crate::rules::places::{Look, PlaceLook, is_app_or_site_rule};
use crate::server::state::Inner;

/// An app the tour names: its bundle ID, which a rule names it by, and its name, which the menu
/// calls it by.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TourApp {
    pub bundle_id: BundleId,
    pub name: String,
}

/// The site the tour opens for its site step, and the browser it opens it in.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TourSite {
    pub domain: Domain,
    pub browser: BundleId,
}

/// What a running tour's steps follow (ADR-0016).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TourFacts {
    /// The tour stands in for the schedule with Night Shift on: it was off as the tour started.
    pub stands_in: bool,
    /// Night Shift on everywhere, as the Night Shift item's check shows it: the switch's side.
    pub everywhere_on: bool,
    pub sample: TourPlace,
    /// None where the tour has no site step.
    pub site: Option<TourPlace>,
}

/// The tour sample or the site, as its step follows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TourPlace {
    /// The sample is the front app, or the site is the one in the front tab (ADR-0017).
    pub in_front: bool,
    /// What its rule says, if it has one.
    pub rule: Option<OnOff>,
    /// Its place's shortest rule says the other way from its indefinite one.
    pub snoozed: bool,
}

/// What `UiCommand::StartTour` named, which the tour's steps follow until it ends.
#[derive(Debug)]
pub(crate) struct Tour {
    sample: BundleId,
    site: Option<Domain>,
}

impl Tour {
    pub(crate) fn new(sample: &TourApp, site: Option<&TourSite>) -> Self {
        Self {
            sample: sample.bundle_id.clone(),
            site: site.map(|site| site.domain.clone()),
        }
    }

    /// What the steps follow in this look.
    pub(crate) fn report(&self, look: &Look<'_>) -> TourFacts {
        let place = |identity: RuleIdentity, front: Option<&PlaceLook>| {
            let place = Place::from(identity);
            let rule = look
                .rules
                .iter()
                .find(|(rule, _)| rule.place == place && is_app_or_site_rule(rule));
            TourPlace {
                in_front: front.is_some_and(|at| at.place == place),
                rule: rule.map(|(rule, _)| rule.says),
                snoozed: rule.is_some_and(|(_, snooze)| snooze.is_some()),
            }
        };
        TourFacts {
            stands_in: look.everywhere.base == Some(Reason::Tour),
            everywhere_on: look.everywhere.gives == OnOff::On,
            sample: place(RuleIdentity::App(self.sample.clone()), look.app.as_ref()),
            site: self
                .site
                .clone()
                .map(|site| place(RuleIdentity::Domain(site), look.site.as_ref())),
        }
    }
}

impl Inner {
    /// Starts the tour, unless one runs, with what its steps follow (ADR-0016). It stands in for
    /// the schedule with Night Shift on if it finds Night Shift off.
    pub(crate) fn tour_started(&mut self, sample: &TourApp, site: Option<&TourSite>) {
        let stands_in = self.facts.night_shift == NightShift::Off;
        if !self.places.start_tour(stands_in) {
            tracing::warn!("a tour started while one runs; keeping the first");
            return;
        }
        self.tour = Some(Tour::new(sample, site));
        tracing::info!(
            "tour started{}",
            if stands_in {
                ", standing in for the schedule with Night Shift on"
            } else {
                ""
            }
        );
    }

    /// Ends the tour, which puts back what it found, short of what ran out by `now`.
    pub(crate) fn tour_ended(&mut self, now: Timestamp) {
        self.places.end_tour(now);
        self.tour = None;
    }
}
