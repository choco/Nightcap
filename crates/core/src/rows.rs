//! The menu's place rows (ADR-0018): everywhere's under the Night Shift item, then the front
//! app's and site's, from one look at the places, and the words they put things in.

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use nightcap_protocol::{
    Command, Layer, Length, OnOff, Place, Reason, Rule, RuleIdentity, RuleSource,
};

use crate::places::{Context, Look, Stack, World, place_identity};
use crate::schedule::CheckedSchedule;

/// A place row: everywhere's under the Night Shift item, then the front app's and site's.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlaceRow {
    pub title: String,
    /// A site's domain, on a line of its own under the title, which says "This Site" so that a
    /// long domain doesn't widen the menu.
    pub site: Option<String>,
    /// What the place says, or for everywhere's row the place that keeps its own say.
    pub subtitle: Option<String>,
    pub image: RowImage,
    /// Drawn one level in, under the row it belongs to: a site under its browser.
    pub indented: bool,
    /// The submenu; empty while there's nothing to offer, which greys the row.
    pub items: Vec<RowItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RowImage {
    /// Everywhere: the timer, as the row under the switch.
    Everywhere,
    /// The app's own icon.
    App {
        bundle_id: String,
    },
    Site,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RowItem {
    Choice {
        item: MenuItem,
    },
    /// Custom…: the shell asks for an end time in a window titled `title`, starting at
    /// `start`, and sends `Set` for `place` and `state` until the time picked.
    PickTime {
        title: String,
        place: Place,
        state: OnOff,
        start: Timestamp,
    },
    Separator,
}

/// Choosing it sends `command` back with `Core::send`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MenuItem {
    pub title: String,
    /// A line under the title, e.g. when a length ends.
    pub subtitle: Option<String>,
    pub enabled: bool,
    pub command: Command,
}

/// What the rows read: a look at the places, the world it was taken in, and the menu's lengths.
pub(crate) struct Rows<'a> {
    pub look: &'a Look<'a>,
    pub world: &'a World<'a>,
    /// The menu's preset lengths.
    pub lengths: &'a [SignedDuration],
}

impl Rows<'_> {
    /// The row under the Night Shift item (ADR-0018): everywhere, whatever is in front, with
    /// everywhere's lengths but the switch's own, and none that would come to the same as
    /// clicking the switch. None when it would offer nothing.
    pub(crate) fn everywhere(&self) -> Option<PlaceRow> {
        let look = self.look;
        let everywhere = &look.everywhere;
        let place = &everywhere.place;
        let state = everywhere.gives.other();
        let mut items = Vec::new();
        // While the tour stands in, what's under everywhere's layers is the tour's on, which the
        // switch gives back; the schedule isn't there to go back to.
        let schedule_says_so = matches!(
            everywhere.base,
            Some(Reason::Schedule { state: scheduled, until: Some(_) }) if scheduled == state
        );
        if schedule_says_so
            && (everywhere.timer.is_some() || everywhere.hold.is_some())
            && look.after_switch.is_some()
        {
            items.push(choice(
                "Back to the Schedule",
                Command::Reset(place.clone()),
            ));
        }
        let mut lengths = Vec::new();
        if everywhere.after_timer != Some(state) {
            lengths.extend(self.timers(place, state, "Turn {} Everywhere"));
        }
        if everywhere.after_reset != Some(state)
            && let Some(until) = self.next_turn(state)
        {
            lengths.push(self.until_turn(place, state, until));
        }
        if !items.is_empty() && !lengths.is_empty() {
            items.push(RowItem::Separator);
        }
        items.extend(lengths);
        if items.is_empty() {
            return None;
        }
        let decision = &look.decision;
        Some(PlaceRow {
            title: format!("Turn {}", capitalized(&state.to_string())),
            site: None,
            // The app or site in front keeps the other way, which the row can't change there.
            subtitle: (decision.place != Place::Everywhere && decision.state != Some(state)).then(
                || {
                    format!(
                        "Everywhere but {}",
                        here(&decision.place, self.world.context)
                    )
                },
            ),
            image: RowImage::Everywhere,
            indented: false,
            items,
        })
    }

    /// The row for an app or site in front (ADR-0018): that place alone, with undoing first:
    /// putting its rule back where that goes the right way, and removing a rule of the user's;
    /// then its lengths, then for good, which is a rule.
    pub(crate) fn place(&self, stack: &Stack) -> PlaceRow {
        let context = self.world.context;
        let place = &stack.place;
        let state = stack.gives.other();
        let rule = rule_of(stack);
        let mut items = Vec::new();
        let below = stack.after_reset;
        if (stack.timer.is_some() || stack.hold.is_some()) && below == Some(state) {
            // A site follows its browser where the browser has its own say.
            let title = match (rule, &stack.inherits) {
                (Some(_), _) => "Put the Rule Back".to_owned(),
                (None, Some((Place::App(bundle_id), _))) => format!(
                    "Like the Rest of {}",
                    name_in_front(&RuleIdentity::App(bundle_id.clone()), context)
                ),
                (None, _) => "Like Everywhere Else".to_owned(),
            };
            items.push(choice(&title, Command::Reset(place.clone())));
        }
        // Removing a rule is the user's to do whenever they like, even where nothing on screen
        // would change.
        if let Some(rule) = rule
            && rule.source != RuleSource::Config
        {
            items.push(choice(
                "Remove Your Rule",
                Command::RemoveRule(rule.identity.clone()),
            ));
        }
        let mut lengths = Vec::new();
        if stack.after_timer != Some(state) {
            let title = format!("Turn {{}} {}", here(place, context));
            lengths.extend(self.timers(place, state, &title));
        }
        if below != Some(state)
            && let Some(until) = self.next_turn(state)
        {
            lengths.push(self.until_turn(place, state, until));
        }
        let always = match rule {
            Some(rule) if rule.says == state => None,
            Some(rule) if rule.source == RuleSource::Config => Some(MenuItem {
                title: "Always".to_owned(),
                subtitle: Some("Your config decides".to_owned()),
                enabled: false,
                command: Command::AddRule {
                    identity: rule.identity.clone(),
                    says: state,
                },
            }),
            _ => place_identity(place).map(|identity| MenuItem {
                title: "Always".to_owned(),
                subtitle: None,
                enabled: true,
                command: Command::AddRule {
                    identity,
                    says: state,
                },
            }),
        };
        if !items.is_empty() && !lengths.is_empty() {
            items.push(RowItem::Separator);
        }
        items.extend(lengths);
        if let Some(item) = always {
            if !items.is_empty() {
                items.push(RowItem::Separator);
            }
            items.push(RowItem::Choice { item });
        }
        let offers = items.iter().any(|item| match item {
            RowItem::Choice { item } => item.enabled,
            RowItem::PickTime { .. } => true,
            RowItem::Separator => false,
        });
        let here = match place {
            Place::Domain(_) => "at This Site".to_owned(),
            _ => here(place, context),
        };
        PlaceRow {
            title: if offers {
                format!("Turn {} {here}", capitalized(&state.to_string()))
            } else {
                capitalized(&here)
            },
            site: match place {
                Place::Domain(domain) => Some(domain.to_string()),
                _ => None,
            },
            subtitle: self.stack_text(stack),
            image: match place {
                Place::App(bundle_id) => RowImage::App {
                    bundle_id: bundle_id.to_string(),
                },
                Place::Domain(_) | Place::Everywhere => RowImage::Site,
            },
            indented: matches!(place, Place::Domain(_)),
            items: if offers { items } else { Vec::new() },
        }
    }

    fn next_turn(&self, state: OnOff) -> Option<Timestamp> {
        self.world
            .timeline
            .next_turn(self.world.applied_until, state)
    }

    /// The menu's lengths for a place, then Custom…, whose window `title` names, with `{}` for
    /// on or off.
    fn timers(&self, place: &Place, state: OnOff, title: &str) -> Vec<RowItem> {
        let mut items: Vec<_> = self
            .lengths
            .iter()
            .map(|duration| {
                choice(
                    &format!("For {}", length_title(*duration)),
                    Command::Set {
                        place: place.clone(),
                        state,
                        length: Length::For(*duration),
                    },
                )
            })
            .collect();
        #[expect(
            clippy::expect_used,
            reason = "rounding fails only beside jiff's limits, millennia from any clock"
        )]
        let start = (self.world.now + SignedDuration::from_hours(1))
            .round(
                jiff::TimestampRound::new()
                    .smallest(jiff::Unit::Minute)
                    .increment(15)
                    .mode(jiff::RoundMode::Ceil),
            )
            .expect("a quarter hour rounds any time");
        items.push(RowItem::PickTime {
            title: title.replace("{}", &capitalized(&state.to_string())),
            place: place.clone(),
            state,
            start,
        });
        items
    }

    fn until_turn(&self, place: &Place, state: OnOff, until: Timestamp) -> RowItem {
        // The time alone could be any clock's; the line under it says it's the schedule's.
        let moment = match (&self.world.timeline.schedule, state) {
            (Some(CheckedSchedule::SunsetToSunrise), OnOff::On) => "At sunrise",
            (Some(CheckedSchedule::SunsetToSunrise), OnOff::Off) => "At sunset",
            (_, OnOff::On) => "When the schedule ends",
            (_, OnOff::Off) => "When the schedule starts",
        };
        RowItem::Choice {
            item: MenuItem {
                title: format!("Until {}", clock_time(until, &self.world.timeline.zone)),
                subtitle: Some(moment.to_owned()),
                enabled: true,
                command: Command::Set {
                    place: place.clone(),
                    state,
                    length: Length::UntilScheduleTurns,
                },
            },
        }
    }

    /// What a place holds, top down, for its row: "On until 07:00, then your config’s rule".
    fn stack_text(&self, stack: &Stack) -> Option<String> {
        let mut parts: Vec<String> = (stack.timer.is_some() || stack.hold.is_some())
            .then(|| {
                layer_text(
                    stack.timer.as_ref(),
                    stack.hold.as_ref(),
                    &self.world.timeline.zone,
                )
            })
            .into_iter()
            .collect();
        match rule_of(stack) {
            Some(rule) if parts.is_empty() => {
                parts.push(format!("always {}: {}", rule.says, whose(rule)));
            }
            Some(rule) => parts.push(whose(rule).to_owned()),
            None => {}
        }
        (!parts.is_empty()).then(|| capitalized(&parts.join(", then ")))
    }
}

/// A place's timer and hold, top down: "on until 23:30, then off until 07:00".
pub(crate) fn layer_text(timer: Option<&Layer>, hold: Option<&Layer>, zone: &TimeZone) -> String {
    [timer, hold]
        .into_iter()
        .flatten()
        .map(|layer| {
            format!(
                "{} {}",
                layer.state,
                until_text(layer.until, layer.state, zone)
            )
        })
        .collect::<Vec<_>>()
        .join(", then ")
}

/// "until 07:00", or "until you turn it on" for an off with no end.
pub(crate) fn until_text(until: Option<Timestamp>, state: OnOff, zone: &TimeZone) -> String {
    match until {
        Some(until) => format!("until {}", clock_time(until, zone)),
        None => format!("until you turn it {}", state.other()),
    }
}

/// A time of day in the user's time zone, as the menu shows it.
pub(crate) fn clock_time(at: Timestamp, zone: &TimeZone) -> String {
    at.to_zoned(zone.clone()).strftime("%H:%M").to_string()
}

/// "in Figma" for the front app, by its name, "at figma.com" for a site, or "everywhere".
pub(crate) fn here(place: &Place, context: &Context) -> String {
    match place {
        Place::Everywhere => "everywhere".to_owned(),
        Place::App(bundle_id) => format!(
            "in {}",
            name_in_front(&RuleIdentity::App(bundle_id.clone()), context)
        ),
        Place::Domain(domain) => format!("at {domain}"),
    }
}

/// An app rule's app by its name when it's the frontmost one, and a domain rule's domain.
pub(crate) fn name_in_front(identity: &RuleIdentity, context: &Context) -> String {
    match identity {
        RuleIdentity::App(bundle_id) => context
            .app_name
            .clone()
            .filter(|_| context.app.as_ref() == Some(bundle_id))
            .unwrap_or_else(|| bundle_id.to_string()),
        RuleIdentity::Domain(domain) => domain.to_string(),
    }
}

pub(crate) fn whose(rule: &Rule) -> &'static str {
    match rule.source {
        RuleSource::Config => "your config’s rule",
        RuleSource::AdHoc => "your rule",
        RuleSource::Tour => "the tour’s rule",
    }
}

/// `text` with its first letter capitalized.
pub(crate) fn capitalized(text: &str) -> String {
    let mut letters = text.chars();
    letters
        .next()
        .map(char::to_uppercase)
        .into_iter()
        .flatten()
        .chain(letters)
        .collect()
}

fn choice(title: &str, command: Command) -> RowItem {
    RowItem::Choice {
        item: MenuItem {
            title: title.to_owned(),
            subtitle: None,
            enabled: true,
            command,
        },
    }
}

/// "30 Minutes", "1 Hour", "1 Hour 30 Minutes".
fn length_title(duration: SignedDuration) -> String {
    let minutes = duration.as_mins();
    let unit =
        |count: i64, name: &str| format!("{count} {name}{}", if count == 1 { "" } else { "s" });
    match (minutes / 60, minutes % 60) {
        (0, minutes) => unit(minutes, "Minute"),
        (hours, 0) => unit(hours, "Hour"),
        (hours, minutes) => format!("{} {}", unit(hours, "Hour"), unit(minutes, "Minute")),
    }
}

/// The rule at a place's base, which for everywhere is none.
fn rule_of(stack: &Stack) -> Option<&Rule> {
    match &stack.base {
        Some(Reason::Rule(rule)) => Some(rule),
        _ => None,
    }
}
