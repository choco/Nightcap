# Every change to Night Shift says where and for how long, and the switch is everywhere's

Night Shift can be changed from the menu, the shortcut, the CLI and Control Center, and each change has to say what it covers and when it ends. Turning Night Shift on in front of a rule, off past sunrise, on for an evening in one app: each needs an answer that survives Cmd-Tabbing away and back and never costs the user their rules. Nightcap can't block Control Center, so an external change always takes effect; the only question is what it means. Two logic prototypes, kept on the `prototype/on-off` branch, settled the model below.

## Decision

**Places and their layers**

- Every change applies to a place: everywhere, the app in front, or the site in front. A site is a domain, covering its subdomains as a domain rule does: the most specific domain with a rule, a hold or a timer that covers the tab, else the tab's own domain.
- A place is a stack. At the bottom is the schedule for everywhere, and a rule, if there is one, for an app or site (ADR-0006). Over that sits a hold, and over the hold a timer. A place says what its top layer says, and an app or site with no layer says nothing.
- The most specific place in front that says something decides what Night Shift does: the site, then the app, then everywhere, which always says something.

**How long**

- A change goes the other way from what its place gives now, whether that's its own say or one inherited from the place before it. It lasts one of three ways:
  - **For a time:** a timer for one of the menu's lengths, 30 minutes, 1 hour and 2 hours unless the config sets others, or until a time picked with Custom….
  - **Until the schedule next turns it back:** a hold that ends at the next sunrise if it's on, the next sunset if it's off, or the end or start of custom hours. The menu shows it as the time, "Until 07:00", with the moment under it: "At sunrise", "At sunset", "When the schedule ends" or "When the schedule starts". Without a schedule it isn't offered.
  - **Until the user turns it back:** for everywhere, a hold with no end. For an app or site, a rule of the user's, made with Always (ADR-0006).
- A timer gives back what was under it when it ends. A new timer replaces the place's timer; a new hold replaces its hold and ends its timer.
- Custom… opens a small window with the end time, an hour from now rounded up to the quarter hour, an earlier time meaning tomorrow. A time typed into a menu fights the menu's own keys.

**The switch**

- The Night Shift item, the shortcut's tap (ADR-0015), and `nightcap on`, `off` and `toggle` are everywhere's switch. The switch takes off everywhere's layers that say otherwise, top first. If the schedule still says otherwise, it holds the new state until the user turns it back. Two flips in a row leave nothing behind.
- A place that says something keeps its say whatever the switch does, so in front of a rule the switch can change nothing on screen. The item is checked while Night Shift is on everywhere, and the line under it says what decides the screen: "Off in Figma: your config’s rule", "Following the schedule until 07:00", "No schedule".
- The row under it, Turn On or Turn Off, is everywhere whatever is in front, with everywhere's other lengths, and Back to the Schedule while a hold or timer keeps everywhere from what the schedule says.

**Snoozing**

- A rule is snoozed by a timer or hold on its place that says the other way. The rule waits underneath, and comes back when the snooze ends or when the user puts it back.
- A snooze from the menu lasts for a time or until the schedule turns it back, never with no end: changing a rule for good is changing the rule (ADR-0006).
- The row for the app or site in front holds that place alone: its lengths, Custom… and the schedule's time, and, as they apply, Always, Remove Your Rule, Put the Rule Back, and Like Everywhere Else for a place with no rule, named Like the Rest of Safari, say, for a site whose browser has a say of its own. A site's row is titled for "This Site", with the domain on the line under it.

**External changes**

- An external change flips what the user sees. It takes off what the deciding place says. If a rule or the schedule still says otherwise, it holds the new state over it until the schedule next turns it back, as macOS's own toggle lasts. With no schedule, the hold lasts until the user turns it back.
- In front of a rule, that snoozes the rule, and a notification says so, e.g. "Snoozed the Figma rule until 07:00". If everywhere says otherwise too, everywhere gets the same hold, linked so that flipping back takes off both.

**The schedule**

- A schedule transition ends only what waits on it. An edited schedule ends every hold waiting on the schedule; timers, and holds with no end, stay.

**State and commands**

- `state.json` keeps each place's hold and timer with its end (ADR-0008).
- Commands: `On`, `Off` and `Toggle` are the switch; `Set { place, state, length }` puts a layer on a place; `Reset { place }` takes off a place's hold and timer.
- The CLI: `nightcap on|off [--for 45m | --until 23:30 | --until-schedule] [--app ID | --domain D]`, `nightcap toggle` and `nightcap reset [--app ID | --domain D]`. A length is written as `30m`, `1h` or `1h30m`.
- The config's `[menu] lengths` sets the menu's preset lengths, in the same form.

## Consequences

- A tap in front of a rule can change nothing on screen, since the switch has one meaning everywhere and snoozing has a row of its own. The menu bar's moon still shows the change.
- A hold with no end keeps Night Shift where the user put it through every sunrise and sunset. Nightcap sets it back after each of macOS's own transitions, as it does for rules.
- Turning Night Shift off everywhere leaves a snoozed rule's app warm, since the snooze is more specific.
- A browser whose own place says off is still watched while a site in it could say on (ADR-0012).
- Custom… needs a window of its own, floating like the tour's panel.
- Notices about adopted changes need notification permission. If it is denied, they appear only in `status` and the logs.

## Rejected alternatives

- **The switch until the schedule's next change, with the overrides it made in front of rules tied to it** (the first prototype): a rule in front took over the switch, and its overrides outlived what the user saw.
- **The switch flipping what the user sees, snoozing a rule in front**: the row under it then acted on the rule too, hiding everywhere exactly when a rule was in front.
- **The switch until the schedule next turns it back**: a forgotten flip would right itself at sunrise or sunset, but the switch would mean something different from the row's "until you turn it back".
- **One item that both switches and opens a submenu**: an AppKit menu item does one or the other.
- **Deleting the rule in front when Night Shift is turned on over it**: a moment's change of mind permanently loses configuration.
- **A snooze until the front app changes, or until the rule stops matching**: Cmd-Tabbing away and back undoes it.
- **A config rule snoozed until the user turns it back**: Nightcap would override the config with no end (ADR-0007).
- **Resume, undoing everything at once**: each place's row undoes its own.
- **Reverting an external change, and notifying**: Night Shift visibly flips back, and if the core is wrong it fights the user, leaving the CLI as the only way out.
- **Adopting external changes silently**: the user never learns what the core took the change to mean.
