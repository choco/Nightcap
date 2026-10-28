# What changes during the tour lasts only while it runs

The tour, offered at the end of onboarding and from Settings, walks through the moon, a rule the user makes and sees act, snoozing it, and the peek, in a floating panel. A rule at work needs an app in front to make one for. The tour opens an app of its own, Tour Sample, even when one of the user's apps with a rule is running: every tour then runs the same way, on the Space the user is on, and the user makes the rule from the menu as they would for any app. Where website checking can, the tour does the same for a site, on the project's own, nightcap.choco.me. And the moon, the rule and the peek show nothing while Night Shift is off, as it is all day: the tour has to warm the screen and give it back as it found it. Everything the user tries along the way would otherwise reach their own state: rules left in `rules.toml`, a rule of theirs changed or removed, temporary rules, or an external change the core adopts.

## Decision

- The shell tells the core when the tour starts and ends, with commands only the menu-bar UI sends: the CLI has no tour. The start names the tour sample and the site where the tour has a site step.
- While the tour runs, the core reports what its steps follow, whichever way the user made a change: whether the sample and the site are in front, what their rules say and whether they're snoozed, the switch's side, and whether the tour stands in for the schedule.
- The steps quote the menu's items from the menu-bar UI's own model of the menu, the one the menu draws, in the situation each step describes, so a step quotes what the menu shows.
- The tour keeps the user's ad-hoc rules and every other rule of the user's or an external change's as it found them, and its end puts them back, short of what would have ended meanwhile. So what the user changes during the tour, from the menu or the CLI, lasts only while it runs:
  - A rule added during the tour is the tour's: the tour set it, lives in memory only, and never reaches `rules.toml`, Settings' rule list or `rules export`.
  - Changing or removing one of the user's own rules stays in memory; `rules.toml` and `rules export` keep the rules the end puts back.
  - Snoozes, other temporary rules and the switch go with the tour.
  - The strength, website checking and Settings' other choices aren't the tour's, and stay.
- When Night Shift is off, the tour's first step says it will turn it on, and the shell tells the core the tour started only once the user continues. The tour then stands in for the schedule with on, for its length: everywhere's rules wait aside, and the Night Shift item says "On for the tour". Night Shift really switches on, so everything the tour shows is real. The user's own choices during the tour still come first, and a tap off then on returns to the tour's on.
- The tour's end, or quitting, puts back what the tour found, and the schedule decides again. Throughout the tour, `state.json` holds what its end would leave, with Night Shift as the tour holds it: after a crash mid-tour, the next run finds Night Shift where the last run left it, which is no external change (ADR-0008), and switches it back.
- Tour Sample is an app in Nightcap's bundle, beside the CLI. A window of Nightcap's own would make Nightcap the front app, and on macOS 27, opening a status item's menu while its own app is in front hands the focus to another app, so the menu would offer a rule for that one.

## Consequences

- The tour can turn Night Shift on in daytime, which the core otherwise does only on the user's command.
- A setter of rules, and a reason for Night Shift being on, that exist only while a tour runs.
- A config reload during the tour still graduates rules, from `rules.toml` and from what the end puts back.
- A second app in the bundle, signed and checked with Nightcap's.

## Rejected alternatives

- **Keeping what the user changed during the tour**: a rule of theirs turned the other way or removed while trying the menu, or a snooze set for the sample, would outlast the tour unnoticed.
- **Warming with the tint alone**, `BlueLightReductionFactor` at 1 with Night Shift off (ADR-0015): no core change, but the moon and the menu would say off while the screen looks warm, and the rule would have nothing to switch off.
- **Ad-hoc rules for what the tour adds**, removed at the end: a crash mid-tour leaves them in `rules.toml`, and Settings lists them meanwhile.
- **The sample's rule there from the start**: the user would watch a rule act without making one, and the menu would need a step of its own.
- **The tour only at night**: it would be offered at the end of onboarding, mostly run in daytime, and show nothing.
- **Turning Night Shift on with `on`**: an indefinite rule for everywhere, which the tour's end would have to undo, and which a crash leaves behind.
- **The panel as a popover from the moon**: on macOS 27 the moon's clicks never arrive while a popover hangs from it, and AppKit hides popovers on a change of Space.
