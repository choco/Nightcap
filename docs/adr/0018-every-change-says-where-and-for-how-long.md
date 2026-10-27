# Every change to Night Shift is a rule that says where and for how long, and the switch is everywhere's

Night Shift can be changed from the menu, the shortcut, the CLI and Control Center, and each change has to say what it covers and when it ends. Turning Night Shift on in front of a rule, off past sunrise, on for an evening in one app: each needs an answer that survives Cmd-Tabbing away and back and never costs the user their rules. Nightcap can't block Control Center, so an external change always takes effect; the only question is what it means. Two logic prototypes, kept on the `prototype/on-off` branch, settled the model below.

## Decision

**Rules**

- Every change to Night Shift is a rule: a place, what it says there, on or off, a length and who set it.
- A place is everywhere, the app in front, or the site in front. A site is a domain, covering its subdomains as a domain rule does (ADR-0006): the most specific domain with a rule that covers the tab, else the tab's own domain (ADR-0017).
- A rule lasts one of three lengths:
  - **Indefinitely**, until it's taken off.
  - **Until the schedule next turns it back:** the next sunrise for an on and the next sunset for an off, or the end or start of custom hours, and earlier if the schedule is edited. Without a schedule there's no turn to wait for, so only an external change sets one then, and it lasts until it's taken off.
  - **Until a time:** one picked, or one of the lengths, 30 minutes, 1 hour and 2 hours unless the config's `[menu] lengths` sets others, from now.
- Who sets what:
  - The config sets only indefinite rules, and never for everywhere (ADR-0006, ADR-0007).
  - The user sets indefinite rules, an app's or site's as ad-hoc rules and everywhere's with the switch, and rules until the schedule turns or until a time, anywhere.
  - The tour sets indefinite rules, which last while it runs (ADR-0016).
  - An external change is adopted as a rule until the schedule turns.
- A change goes the other way from what its place gives now, whether that's its own say or one inherited from the place before it.

**Which rule decides**

- At one place, there's one rule of each length at most, and the shortest decides: until a time, then until the schedule turns, then indefinitely. When one ends, the next takes over. Everywhere's indefinite rule and its rule until the schedule turns take each other's place.
- A new rule takes off its place's rules of its length and shorter.
- Across places, the most specific place in front that says something decides what Night Shift does: the site, then the app, then everywhere. Under everywhere's rules lies the schedule, which says something once it's known.

**The switch**

- The Night Shift item, the shortcut's tap (ADR-0015), and `nightcap on`, `off` and `toggle` are everywhere's switch. The switch takes off everywhere's rules that say otherwise, shortest first. If the schedule still says otherwise, it sets an indefinite rule. Two flips in a row leave nothing behind.
- A place that says something keeps its say whatever the switch does, so in front of a rule the switch can change nothing on screen.
- Reset takes off a place's temporary rules, which puts back its indefinite one. For everywhere it takes off the indefinite rule too, back to the schedule.

**Snoozing**

- A rule is snoozed by a shorter one at its place that says the other way. It waits underneath, and comes back when the snooze ends or the place is reset.
- An app's or site's snooze is temporary: changing its indefinite rule is changing the rule (ADR-0006).

**External changes**

- An external change flips what the user sees. It takes off what the deciding place says: its rules that say otherwise, shortest first, short of an app's or site's indefinite one. If an indefinite rule or the schedule still says otherwise, it sets the new state until the schedule next turns it back, as macOS's own toggle lasts.
- In front of an app's or site's rule, that snoozes the rule, and the core reports it, which the shell tells as "Snoozed the Figma rule until 07:00". If everywhere says otherwise too, everywhere gets the same rule, linked so that taking off either takes off both. The link stays inside the core.

**The schedule**

- A schedule transition ends only what waits on it. An edited schedule ends every rule waiting on its next turn; rules until a time, and those with no end, stay.

**State and commands**

- `state.json` keeps every rule but the apps' and sites' indefinite ones, which the config and `rules.toml` keep, with its end, who set it and Adoption's link (ADR-0008).
- Commands: `On`, `Off` and `Toggle` are the switch. `Set { place, state, length }` sets a rule of the user's for a time, until a time, until the schedule turns, or, for everywhere alone, indefinitely; an app's or site's indefinite rule is added with `AddRule` (ADR-0006). `Reset(place)` takes off what a reset does.
- The CLI: `nightcap on|off [--for 45m | --until 23:30 | --until-schedule] [--app ID | --domain D]`, `nightcap toggle` and `nightcap reset [--app ID | --domain D]`. A length is written as `30m`, `1h` or `1h30m`.
- The config's `[menu] lengths` sets the lengths a change can be picked for, in the same form.
- What the menu offers for each place, and in what words, is the App context's: its glossary's Place row.

## Consequences

- A tap in front of a rule can change nothing on screen, since the switch has one meaning everywhere. The menu bar's moon still shows the change.
- An indefinite rule for everywhere keeps Night Shift where the user put it through every sunrise and sunset. Nightcap sets it back after each of macOS's own transitions, as it does for an app's rule.
- Turning Night Shift off everywhere leaves a snoozed rule's app warm, since the snooze is more specific.
- A browser whose own place says off is still watched while a site in it could say on (ADR-0012).
- Telling the user what an external change was taken as needs notification permission. If it is denied, only `status` and the logs say so.

## Rejected alternatives

- **Layers: a place's rule under a hold and a timer, with the schedule under everywhere's**: three names, in the storage's terms, for what differ only in how long they last.
- **The switch until the schedule's next change, with the overrides it made in front of rules tied to it** (the first prototype): a rule in front took over the switch, and its overrides outlived what the user saw.
- **The switch flipping what the user sees, snoozing a rule in front**: changing everywhere would then act on the rule too, hiding everywhere exactly when a rule was in front.
- **The switch until the schedule next turns it back**: a forgotten flip would right itself at sunrise or sunset, but the switch would mean something different from an indefinite rule.
- **Deleting the rule in front when Night Shift is turned on over it**: a moment's change of mind permanently loses configuration.
- **A snooze until the front app changes, or until the rule stops matching**: Cmd-Tabbing away and back undoes it.
- **A config rule snoozed indefinitely**: Nightcap would override the config with no end (ADR-0007).
- **Resume, undoing everything at once**: each place's reset undoes its own.
- **Reverting an external change, and notifying**: Night Shift visibly flips back, and if the core is wrong it fights the user, leaving the CLI as the only way out.
- **Adopting external changes silently**: the user never learns what the core took the change to mean.
