//! What the tour's steps follow and the menu's titles they quote (ADR-0016): the facts from
//! the live look at the places, and the titles from the rows the menu shows, on fixed scenes.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nightcap_protocol::{BundleId, Command, Domain, Length, OnOff, Place, Reason, RuleIdentity};

use crate::domain::Host;
use crate::places::{Context, Kept, Look, Places, Refusal, Stack, World};
use crate::rows::Rows;
use crate::schedule::{SunTimes, Timeline};
use crate::{MenuItem, PlaceRow, RowItem, config};

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

/// What a running tour's steps follow, and the titles they quote (ADR-0016).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TourSnapshot {
    pub facts: TourFacts,
    pub titles: TourTitles,
}

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
    /// Its place's top layer says the other way from its rule.
    pub snoozed: bool,
}

/// A menu item as the tour quotes it: the place row it's in, then the item.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MenuPath {
    pub row: String,
    pub item: String,
}

/// The menu's titles that the tour's steps quote, as `menu()` shows them in the situation each
/// step describes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TourTitles {
    /// Always, in the sample's row with the sample in front and no rule.
    pub add_sample_rule: MenuPath,
    /// The sample's row while its rule holds, whose lengths snooze the rule.
    pub snooze_sample: String,
    /// What ends the rule's snooze, in the sample's row.
    pub put_sample_rule_back: MenuPath,
    /// Always, in the site's row with its browser in front on it.
    pub add_site_rule: Option<MenuPath>,
    /// Always, in the example's row with it in front and Night Shift off: a rule that says on.
    pub add_example_rule: MenuPath,
}

/// The titles the tour's steps quote, for its sample, its site and the app it gives as an
/// example, which `Core::tour` reports too.
#[uniffi::export]
pub fn tour_titles(sample: TourApp, site: Option<TourSite>, example: TourApp) -> TourTitles {
    titles(&sample, site.as_ref(), &example)
}

/// What `TourStarted` named, which the tour's steps follow until it ends.
#[derive(Debug)]
pub(crate) struct Tour {
    sample: BundleId,
    site: Option<Domain>,
    titles: TourTitles,
}

impl Tour {
    pub(crate) fn new(sample: &TourApp, site: Option<&TourSite>, example: &TourApp) -> Self {
        Self {
            sample: sample.bundle_id.clone(),
            site: site.map(|site| site.domain.clone()),
            titles: titles(sample, site, example),
        }
    }

    /// What the steps follow in this look, and the titles they quote.
    pub(crate) fn report(&self, look: &Look<'_>) -> TourSnapshot {
        let place = |identity: RuleIdentity, front: Option<&Stack>| {
            let rule = look
                .rules
                .iter()
                .find(|(rule, _)| rule.identity == identity);
            let place = Place::from(identity);
            TourPlace {
                in_front: front.is_some_and(|stack| stack.place == place),
                rule: rule.map(|(rule, _)| rule.says),
                snoozed: rule.is_some_and(|(_, snooze)| snooze.is_some()),
            }
        };
        TourSnapshot {
            facts: TourFacts {
                stands_in: look.everywhere.base == Some(Reason::Tour),
                everywhere_on: look.everywhere.gives == OnOff::On,
                sample: place(RuleIdentity::App(self.sample.clone()), look.app.as_ref()),
                site: self
                    .site
                    .clone()
                    .map(|site| place(RuleIdentity::Domain(site), look.site.as_ref())),
            },
            titles: self.titles.clone(),
        }
    }
}

/// The titles the steps quote, from the rows the menu shows in the situation each step
/// describes, reached with the rows' own commands: the sample in front, given the rule its row
/// offers, snoozed for its row's first length; the site in front in its browser; and the
/// example in front with Night Shift off. Each item is found by its command, so a renamed one
/// carries through.
pub(crate) fn titles(sample: &TourApp, site: Option<&TourSite>, example: &TourApp) -> TourTitles {
    let identity = RuleIdentity::App(sample.bundle_id.clone());
    let in_front = Scene::new(Some(sample), None, OnOff::On);
    let mut places = Places::new(Kept::default());
    places.start_tour(true);

    let row = in_front.row(&places);
    let always = item(&row, |command| is_rule_for(command, &identity));
    let add_sample_rule = path(&row, always);
    in_front.run(&mut places, &always.command);

    let row = in_front.row(&places);
    let snooze_sample = row.title.clone();
    let snooze = item(&row, |command| {
        matches!(
            command,
            Command::Set {
                length: Length::For(_),
                ..
            }
        )
    });
    in_front.run(&mut places, &snooze.command);

    let row = in_front.row(&places);
    let reset = Command::Reset(Place::from(identity));
    let put_sample_rule_back = path(&row, item(&row, |command| *command == reset));

    let add_site_rule = site.map(|site| {
        let on_site = Scene::new(None, Some(site), OnOff::On);
        let mut places = Places::new(Kept::default());
        places.start_tour(true);
        let row = on_site.row(&places);
        let identity = RuleIdentity::Domain(site.domain.clone());
        path(&row, item(&row, |command| is_rule_for(command, &identity)))
    });

    let off = Scene::new(Some(example), None, OnOff::Off);
    let row = off.row(&Places::new(Kept::default()));
    let identity = RuleIdentity::App(example.bundle_id.clone());
    let add_example_rule = path(&row, item(&row, |command| is_rule_for(command, &identity)));

    TourTitles {
        add_sample_rule,
        snooze_sample,
        put_sample_rule_back,
        add_site_rule,
        add_example_rule,
    }
}

/// One situation a step describes, as the menu would see it: an app in front, or a browser on
/// a site, with Night Shift as `observed` reports it and no schedule. The titles quoted depend
/// on none of the time zone, the time or the lengths, so the scene takes the defaults.
struct Scene {
    context: Context,
    timeline: Timeline,
    observed: OnOff,
}

impl Scene {
    fn new(app: Option<&TourApp>, site: Option<&TourSite>, observed: OnOff) -> Self {
        Self {
            context: Context {
                app: app
                    .map(|app| app.bundle_id.clone())
                    .or_else(|| site.map(|site| site.browser.clone())),
                app_name: app.map(|app| app.name.clone()),
                host: site.and_then(|site| Host::new(site.domain.as_str())),
            },
            timeline: Timeline {
                schedule: None,
                sun_times: SunTimes::default(),
                zone: TimeZone::UTC,
            },
            observed,
        }
    }

    fn world(&self) -> World<'_> {
        World {
            context: &self.context,
            timeline: &self.timeline,
            applied_until: Timestamp::UNIX_EPOCH,
            now: Timestamp::UNIX_EPOCH,
            observed: self.observed,
            website_checking: true,
        }
    }

    /// The row for the site in front, else for the front app.
    #[expect(
        clippy::expect_used,
        reason = "a scene always puts an app in front, and a host only with a browser"
    )]
    fn row(&self, places: &Places) -> PlaceRow {
        let world = self.world();
        let look = places.look(&world);
        let lengths = config::default_lengths();
        let rows = Rows {
            look: &look,
            world: &world,
            lengths: &lengths,
        };
        rows.place(
            look.site
                .as_ref()
                .or(look.app.as_ref())
                .expect("the scene has a place in front"),
        )
    }

    /// Runs a command a row offers, as the menu would.
    #[expect(
        clippy::expect_used,
        reason = "the tour's own rules and their snoozes are never refused"
    )]
    fn run(&self, places: &mut Places, command: &Command) {
        let world = self.world();
        let outcome: Result<(), Refusal> = match command {
            Command::AddRule { identity, says } => places.add_rule(identity, *says),
            Command::Set {
                place,
                state,
                length,
            } => places.set(place, *state, *length, &world),
            _ => Ok(()),
        };
        outcome.expect("a scene's command is never refused");
    }
}

/// The row's item that sends the command `wanted` picks out.
#[expect(
    clippy::expect_used,
    reason = "each scene's row offers what its step asks for, which the tests check"
)]
fn item(row: &PlaceRow, wanted: impl Fn(&Command) -> bool) -> &MenuItem {
    row.items
        .iter()
        .find_map(|item| match item {
            RowItem::Choice { item } if wanted(&item.command) => Some(item),
            _ => None,
        })
        .expect("the row offers the step's item")
}

fn is_rule_for(command: &Command, identity: &RuleIdentity) -> bool {
    matches!(command, Command::AddRule { identity: ruled, .. } if ruled == identity)
}

fn path(row: &PlaceRow, item: &MenuItem) -> MenuPath {
    MenuPath {
        row: row.title.clone(),
        item: item.title.clone(),
    }
}
