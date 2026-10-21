# Every change to Night Shift says where and for how long, and the switch is everywhere's

The menu had four ways to change Night Shift, each lasting its own way. The Night Shift item and the shortcut's tap set the manual state until the next schedule transition, and in front of a rule they snoozed it until then too (ADR-0006, ADR-0021). Turn Off For forced Night Shift off for a fixed time. The rule toggle added, removed or snoozed a rule (ADR-0009). Resume undid the lot. What a snooze was, how long it lasted, and why turning Night Shift off didn't end one were all unclear, and some changes had no way in: on for a while, off past sunrise, one app for an evening. Two logic prototypes, kept on the `prototype/on-off` branch, settled the model below.

## Decision

**Places and their layers**

- Every change applies to a place: everywhere, the app in front, or the site in front. A site is a domain, covering its subdomains as a domain rule does: the domain of the most specific rule covering the tab, else the tab domain.
- A place is a stack. At the bottom is the schedule for everywhere, and a rule, if there is one, for an app or site (ADR-0025). Over that sits a hold, and over the hold a timer. A place says what its top layer says, and an app or site with no layer says nothing.
- The most specific place in front that says something decides what Night Shift does: the site, then the app, then everywhere, which always says something.

**How long**

- A change goes the other way from what its place gives now, whether that's its own say or one inherited from the place before it. It lasts one of three ways:
  - **For a time:** a timer for one of the menu's lengths, 30 minutes, 1 hour and 2 hours unless the config sets others, or until a time picked with Custom….
  - **Until the schedule next turns it back:** a hold that ends at the next sunrise if it's on, the next sunset if it's off, or the ends of custom hours. The menu shows it as the time, as in "Until 07:00", and doesn't offer it without a schedule.
  - **Until the user turns it back:** for everywhere, a hold with no end. For an app or site, a rule of the user's, made with Always (ADR-0025).
- A timer gives back what was under it when it ends. A new timer replaces the place's timer; a new hold replaces its hold and ends its timer.
- Custom… opens a small window with the end time, an hour from now rounded up to the quarter hour, an earlier time meaning tomorrow. A time typed into a menu fights the menu's own keys.

**The switch**

- The Night Shift item, the shortcut's tap, and `nightcap on`, `off` and `toggle` are everywhere's switch. The switch takes off everywhere's layers that say otherwise, top first. If the schedule still says otherwise, it holds the new state until the user turns it back. Two flips in a row leave nothing behind.
- A place that says something keeps its say whatever the switch does, so in front of a rule the switch can change nothing on screen. The item is checked while Night Shift is on everywhere, and the line under it says what decides the screen.
- The row under it, Turn On or Turn Off, is everywhere whatever is in front, with everywhere's other lengths.

**Snoozing**

- A rule is snoozed by a timer or hold on its place that says the other way. The rule waits underneath, and comes back when the snooze ends or when the user puts it back.
- A snooze lasts for a time or until the schedule turns it back, never with no end: changing a rule for good is changing the rule (ADR-0025).
- The row for the app or site in front holds that place alone: its lengths, Custom…, until sunrise or sunset, and, as they apply, Always, Remove Your Rule and Put the Rule Back.

**External changes**

- Nightcap can't stop Control Center, so an external change flips what the user sees. It takes off what the deciding place says. If a rule or the schedule still says otherwise, it holds the new state over it until the schedule next turns it back, as macOS's own toggle lasts. With no schedule, the hold lasts until the user turns it back.
- In front of a rule, that snoozes the rule, and a notification says so. If everywhere says otherwise too, everywhere gets the same hold, linked so that flipping back takes off both.

**The schedule**

- A schedule transition ends only what waits on it. An edited schedule ends every hold waiting on the schedule; timers, and holds with no end, stay.

**State and commands**

- `state.json` keeps each place's hold and timer with its end (ADR-0011). The manual state, the snoozes and the timed disable go. A file from an earlier build loads without them, so an update drops what they held.
- Commands:
  - `On`, `Off` and `Toggle` are the switch.
  - `Set { place, state, length }` puts a layer on a place.
  - `Reset { place }` takes off a place's hold and timer.
  - `Disable`, `Resume`, `Snooze` and `Unsnooze` go.
- The CLI: `nightcap on|off [--for 45m | --until 23:30 | --until-schedule] [--app ID | --domain D]`, `nightcap toggle` and `nightcap reset [--app ID | --domain D]`. `disable`, `resume`, `rules snooze` and `rules unsnooze` go. A length is written as `30m`, `1h` or `1h30m`.
- The config's `[menu] lengths` sets the menu's preset lengths, in the same form.

This supersedes ADR-0006. It supersedes in part:

- ADR-0005: a schedule transition no longer ends the manual state and the snoozes.
- ADR-0009: the rule toggle, and snoozing a config rule.
- ADR-0011: what `state.json` holds.
- ADR-0020: an edited schedule ends the holds waiting on the schedule.
- ADR-0021: a tap is everywhere's switch, not an override.
- ADR-0022: the tour sets aside everywhere's hold and timer.

## Consequences

- A tap in front of a rule can change nothing on screen. That reopens an alternative ADR-0006 rejected, `on` appearing to do nothing, because the switch now has one meaning everywhere, and snoozing has a row of its own. The menu bar's moon still shows the change.
- A hold with no end keeps Night Shift where the user put it through every sunrise and sunset. Nightcap sets it back after each of macOS's own transitions, as it does for rules.
- Turning Night Shift off everywhere leaves a snoozed rule's app warm, since the snooze is more specific.
- A browser whose own place says off is still watched while a site in it could say on.
- Custom… needs a window of its own, floating like the tour's panel.

## Rejected alternatives

- **The switch until the schedule's next change, with the overrides it made in front of rules tied to it** (the first prototype): a rule in front took over the switch, and its overrides outlived what the user saw.
- **The switch flipping what the user sees, snoozing a rule in front**: the row under it then acted on the rule too, hiding everywhere exactly when a rule was in front.
- **The switch until the schedule next turns it back**: a forgotten flip would right itself at sunrise or sunset, but the switch would mean something different from the row's "until you turn it back".
- **One item that both switches and opens a submenu**: an AppKit menu item does one or the other.
- **A config rule snoozed until the user turns it back**: Nightcap would override the config with no end, which ADR-0009 rejected.
- **Resume, undoing everything at once**: each place's row now undoes its own.
