# Every change to Night Shift is a rule that says where and for how long, a place holds one temporary rule, and the switch is everywhere's

Night Shift can be changed from the menu, the shortcut, the CLI and Control Center, and each change has to say what it covers and when it ends. Turning Night Shift on in front of a rule, off past sunrise, on for an evening in one app: each needs an answer that survives Cmd-Tabbing away and back and never costs the user their rules. Nightcap can't block Control Center, so an outside change always takes effect; the only question is what it means. Two logic prototypes, kept on the `prototype/on-off` branch, settled the model below.

## Decision

**Rules**

- Every change to Night Shift is a rule: a place, what it says there, on or off, a length and who set it.
- A place is everywhere, the app in front, or the site in front. A site is a domain, covering its subdomains as a domain rule does (ADR-0006): the most specific domain with a rule that covers the tab, else the tab's own domain (ADR-0017).
- A rule lasts one of three lengths:
  - **Indefinitely**, until it's taken off.
  - **Until the schedule next turns it back:** the next sunrise for an on and the next sunset for an off, or the end or start of custom hours, and earlier if the schedule is edited. Without a schedule there's no turn to wait for, so only an outside change sets one then, and it lasts until it's taken off.
  - **Until a time:** one picked, or one of the lengths, 30 minutes, 1 hour and 2 hours unless the config's `lengths` sets others, from now.
- A rule until the schedule turns or until a time is temporary.
- Who sets what:
  - The config sets only indefinite rules, and never for everywhere (ADR-0006, ADR-0007).
  - The user sets indefinite rules, an app's or site's as ad-hoc rules and everywhere's with the switch, and temporary rules, anywhere.
  - During a trial, the indefinite rules the user adds are the trial's, which last while it runs (ADR-0016).
  - An outside change is adopted as a rule until the schedule turns.
- A place's row turns it the other way from what it gives now, whether that's its own say or one inherited from the place before it.

**Which rule decides**

- Each place holds its indefinite rule and at most one temporary rule. An app's or site's indefinite rule is the config's, the user's or a trial's; everywhere's is the switch's, over the stand-in or the schedule.
- The temporary rule decides over the indefinite one. When it ends, the indefinite rule decides again, and for everywhere without one, the stand-in or the schedule.
- A new temporary rule at a place replaces the one there, whatever its length.
- Across places, the most specific place in front that says something decides what Night Shift does: the site, then the app, then everywhere. Under everywhere's rules lies the schedule, which says something once it's known.

**The switch**

- The Night Shift item, the shortcut's tap (ADR-0015), and `nightcap on`, `off` and `toggle` are everywhere's switch. The switch takes off everywhere's temporary rule if it says otherwise, then its indefinite rule if that says otherwise. If the stand-in or the schedule still says otherwise, it sets an indefinite rule. Two flips in a row leave nothing behind.
- A place that says something keeps its say whatever the switch does, so in front of a rule the switch can change nothing on screen.
- Reset takes off a place's temporary rule, which puts back its indefinite one. For everywhere it takes off the indefinite rule too, back to the stand-in or the schedule.

**Snoozing**

- An indefinite rule is snoozed by its place's temporary rule where that says the other way. It waits underneath, and comes back when the snooze ends or the place is reset.
- An app's or site's snooze is temporary: changing its indefinite rule is changing the rule (ADR-0006).

**Outside changes**

- An outside change flips what the user sees. It takes off what the deciding place says: its temporary rule, then for everywhere its indefinite one, while they say otherwise, short of an app's or site's indefinite rule. If an indefinite rule or the schedule still says otherwise, it sets the new state as the place's temporary rule until the schedule next turns it back, as macOS's own toggle lasts.
- In front of an app's or site's rule, that snoozes the rule, and the core reports it, which the shell tells as "Snoozed the Figma rule until 07:00". If everywhere says otherwise too, everywhere gets the same temporary rule, linked so that taking off either takes off both. The link stays inside the core.

**The schedule**

- A schedule transition ends only what waits on it. An edited schedule ends every temporary rule waiting on its next turn; those until a time, and those with no end, stay.

**State and commands**

- `state.json` keeps every place's temporary rule and everywhere's indefinite one, but not the apps' and sites' indefinite rules, which the config and `rules.toml` keep, each with its end, who set it, and Adoption's link (ADR-0008). Of the rules a file holds at a place, the first of each kind stays.
- Commands: `On`, `Off` and `Toggle` are the switch. `Set { place, state, length }` sets a temporary rule of the user's, for a time, until a time or until the schedule turns, in place of the place's; or, for everywhere alone, its indefinite rule, which takes off its temporary one too. An app's or site's indefinite rule is added with `AddRule` (ADR-0006). `Reset(place)` takes off what a reset does.
- The CLI: `nightcap on|off [--for 45m | --until 23:30 | --until-schedule] [--app ID | --domain D]`, `nightcap toggle` and `nightcap reset [--app ID | --domain D]`. A length is written as `30m`, `1h` or `1h30m`.
- The config's top-level `lengths` sets the lengths a temporary rule can be picked for, in the same form: the lengths are the model's, whichever client offers them.
- The core answers how each place in front stands: what it gives, its temporary rule, its indefinite rule, for everywhere the stand-in or the schedule under them, and what it inherits and from where. It forecasts nothing. `night_shift()` answers when the schedule next turns Night Shift each way, which is when a rule until the schedule turns would end, and the rules query lists each indefinite rule with its snooze. The app decides each item the menu offers from that state, relying on the commands' contracts: a reset leaves the place's indefinite rule, else what it inherits, and for everywhere the stand-in or the schedule; a new temporary rule takes the place of the one there; and the switch keeps everywhere's indefinite rule where it says its way, taking off the temporary rule over it. Which items, and in what words, is the App context's: its glossary's Place row.

## Consequences

- A tap in front of a rule can change nothing on screen, since the switch has one meaning everywhere. The menu bar's moon still shows the change.
- An indefinite rule for everywhere keeps Night Shift where the user put it through every sunrise and sunset. Nightcap sets it back after each of macOS's own transitions, as it does for an app's rule.
- A temporary rule for everywhere, such as on until sunrise over the switch's off, gives the indefinite rule back when it ends.
- A temporary rule set over another takes its place: on until sunrise, then off for an hour, leaves the place to its indefinite rule once the hour is up. Over a snooze, a length comes to the same as putting the rule back, so the menu offers only that.
- Turning Night Shift off everywhere leaves a snoozed rule's app warm, since the snooze is more specific.
- A browser whose own place says off is still watched while a site in it could say on (ADR-0012).
- Telling the user what an outside change was taken as needs notification permission. If it is denied, only `status` and the logs say so.

## Rejected alternatives

- **Layers: a place's rule under a hold and a timer, with the schedule under everywhere's**: three names, in the storage's terms, for what differ only in how long they last.
- **A stack at each place, one rule of each length, the shortest deciding and each giving back the next as it ends**: the menu reached a stack through one path, a length after Until …, and hid it everywhere else, while a row's line and the CLI told the whole chain.
- **Everywhere's indefinite rule and its temporary rule in one place, each replacing the other**: on until sunrise over the switch's off lost the off for good, where at an app or a site the indefinite rule comes back.
- **The switch until the schedule's next change, with the overrides it made in front of rules tied to it** (the first prototype): a rule in front took over the switch, and its overrides outlived what the user saw.
- **The switch flipping what the user sees, snoozing a rule in front**: changing everywhere would then act on the rule too, hiding everywhere exactly when a rule was in front.
- **The switch until the schedule next turns it back**: a forgotten flip would right itself at sunrise or sunset, but the switch would mean something different from an indefinite rule.
- **Deleting the rule in front when Night Shift is turned on over it**: a moment's change of mind permanently loses configuration.
- **A snooze until the front app changes, or until the rule stops matching**: Cmd-Tabbing away and back undoes it.
- **A config rule snoozed indefinitely**: Nightcap would override the config with no end (ADR-0007).
- **Resume, undoing everything at once**: each place's reset undoes its own.
- **Reverting an outside change, and notifying**: Night Shift visibly flips back, and if the core is wrong it fights the user, leaving the CLI as the only way out.
- **Adopting outside changes silently**: the user never learns what the core took the change to mean.
