//! The menu, which the shell pulls after each event and tick (ADR-0002): the moon, the Night
//! Shift item with what decides the screen, the problem line, the strength and the place rows.

use jiff::Timestamp;
use nightcap_protocol::{Command, Decision, NightShift, OnOff, Place, Reason};

use crate::places::Stack;
use crate::rows::{PlaceRow, Rows, capitalized, clock_time, here, until_text, whose};
use crate::schedule::CheckedSchedule;
use crate::{ProblemLine, State};

/// What the menu shows. The core decides it; the shell only draws it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Menu {
    pub icon: MenuIcon,
    /// The menu's first item, after the problem line.
    pub night_shift: NightShiftItem,
    /// The problem line, while any problem exists.
    pub problem_line: Option<ProblemLine>,
    pub entries: Vec<MenuEntry>,
}

/// The menu's Night Shift item: everywhere's switch, with what decides the screen (ADR-0018).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NightShiftItem {
    /// Checked while Night Shift is on everywhere.
    pub on: bool,
    /// What decides the screen, in a few words, e.g. "Off in Figma: your config’s rule".
    pub reason: Option<String>,
    /// None while Night Shift is unavailable, which disables the item.
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MenuIcon {
    On,
    Off,
    ForcedOff,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MenuEntry {
    /// The strength's slider, 0 to 100, under the Night Shift item. Moving it sends
    /// `PreviewStrength`, and letting go sends `Strength`.
    Strength {
        value: u8,
    },
    /// A place row: a submenu that turns one place the other way (ADR-0018).
    Row {
        row: PlaceRow,
    },
    Separator,
}

impl Menu {
    /// The menu as `state` has it at `now`.
    pub(crate) fn new(state: &State, now: Timestamp) -> Self {
        if state.facts.night_shift == NightShift::Unavailable {
            return Menu {
                icon: MenuIcon::Unavailable,
                night_shift: NightShiftItem {
                    on: false,
                    reason: Some("Unavailable".to_owned()),
                    command: None,
                },
                problem_line: None,
                entries: Vec::new(),
            };
        }
        let world = state.facts.world(now);
        let look = state.places.look(&world);
        let rows = Rows {
            look: &look,
            world: &world,
            lengths: &state.lengths,
        };
        let everywhere = look.everywhere.gives;
        let mut entries = Vec::new();
        if let Some(value) = state.strength {
            entries.push(MenuEntry::Strength { value });
        }
        if let Some(row) = rows.everywhere() {
            entries.push(MenuEntry::Row { row });
        }
        let front: Vec<&Stack> = look.app.iter().chain(&look.site).collect();
        if !front.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        for stack in front {
            entries.push(MenuEntry::Row {
                row: rows.place(stack),
            });
        }
        Menu {
            icon: match look.screen {
                OnOff::Off
                    if look.decision.place != Place::Everywhere && everywhere == OnOff::On =>
                {
                    MenuIcon::ForcedOff
                }
                OnOff::On => MenuIcon::On,
                OnOff::Off => MenuIcon::Off,
            },
            night_shift: NightShiftItem {
                on: everywhere == OnOff::On,
                reason: state.menu_reason(&look.decision),
                command: Some(match everywhere {
                    OnOff::On => Command::Off,
                    OnOff::Off => Command::On,
                }),
            },
            problem_line: state.problem_line(now),
            entries,
        }
    }
}

impl State {
    /// The reason under the menu's Night Shift item: what decides the screen.
    fn menu_reason(&self, decision: &Decision) -> Option<String> {
        let Some(top) = decision.reasons.first() else {
            // Without the sun times, only macOS knows where the schedule stands, and applies it.
            return (self.facts.timeline.schedule == Some(CheckedSchedule::SunsetToSunrise))
                .then(|| "Following the schedule".to_owned());
        };
        let at = match &decision.place {
            Place::Everywhere => String::new(),
            place => format!(" {}", here(place, &self.facts.context)),
        };
        let text = match top {
            Reason::Timer(layer) | Reason::Hold(layer) => format!(
                "{}{at} {}",
                layer.state,
                until_text(layer.until, layer.state, &self.facts.timeline.zone)
            ),
            Reason::Rule(rule) => format!("{}{at}: {}", rule.says, whose(rule)),
            Reason::Tour => "on for the tour".to_owned(),
            Reason::Schedule { until, .. } => match (until, self.facts.timeline.schedule) {
                (Some(until), _) => {
                    format!(
                        "following the schedule until {}",
                        clock_time(*until, &self.facts.timeline.zone)
                    )
                }
                (None, Some(CheckedSchedule::Off)) => "no schedule".to_owned(),
                (None, _) => "following the schedule".to_owned(),
            },
        };
        Some(capitalized(&text))
    }
}
