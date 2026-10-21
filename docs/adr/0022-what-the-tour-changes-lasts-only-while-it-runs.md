# What the tour turns on and adds lasts only while it runs

The tour, offered at the end of onboarding and from Settings, walks through the moon, a rule the user makes and sees act, and the peek, in a floating panel. A rule at work needs an app in front to make one for. The tour opens an app of its own, Tour Sample, even when one of the user's apps with a rule is running: every tour then runs the same way, on the Space the user is on, and the user makes the rule from the menu as they would for any app. Where website checking can, the tour does the same for a site, on the project's own, nightcap.choco.me. And the moon, the rule and the peek show nothing while Night Shift is off, as it is all day: the tour has to warm the screen and give it back as it found it. All of it would otherwise reach the user's own state: ad-hoc rules left in `rules.toml`, a manual state, or an external change the core adopts.

## Decision

- The shell tells the core when the tour starts and ends. Neither is a command: the CLI has no tour.
- While the tour runs, every rule the user adds, from the menu or the CLI, is the tour's rather than an ad-hoc one: it has its own source, lives in memory only, and its toggle removes it again. Tour rules never reach `state.json`, `rules.toml`, Settings' rule list or `rules export`, and the tour's end removes them with their snoozes. A rule that exists already stays as it is, and removing one of the user's own rules during the tour removes it for good.
- When Night Shift is off, the tour's first step says it will turn it on, and the shell tells the core the tour started only once the user continues. The tour then stands in for the schedule with on, for its length: it sets aside the manual state and the timed disable, and is the reason under the status line. Night Shift really switches on, so everything the tour shows is real. The user's own choices during the tour still come first, and a tap off then on returns to the tour's on.
- The tour's end, or quitting, puts back what it set aside, short of what the user chose during the tour, and the schedule decides again. Throughout the tour, `state.json` holds the state its end would leave, with Night Shift as the tour holds it: after a crash mid-tour, the next run finds Night Shift where the last run left it, which is no external change (ADR-0011), and switches it back.
- Tour Sample is an app in Nightcap's bundle, beside the CLI. A window of Nightcap's own would make Nightcap the front app, and on macOS 27, opening a status item's menu while its own app is in front hands the focus to another app, so the menu would offer a rule for that one.

## Consequences

- The tour can turn Night Shift on in daytime, which the core otherwise does only on the user's command.
- A rule source, and a reason for Night Shift being on, that exist only while a tour runs.
- A second app in the bundle, signed and checked with Nightcap's.

## Rejected alternatives

- **Warming with the tint alone**, `BlueLightReductionFactor` at 1 with Night Shift off (ADR-0021): no core change, but the moon and the menu would say off while the screen looks warm, and the rule would have nothing to switch off.
- **Ad-hoc rules for what the tour adds**, removed at the end: a crash mid-tour leaves them in `rules.toml`, and Settings lists them meanwhile.
- **The sample's rule there from the start**: the user would watch a rule act without making one, and the menu would need a step of its own.
- **The tour only at night**: it would be offered at the end of onboarding, mostly run in daytime, and show nothing.
- **Turning Night Shift on with `on`**: a manual state until the next transition, which the tour's end would have to undo, and which a crash leaves behind.
- **The panel as a popover from the moon**: on macOS 27 the moon's clicks never arrive while a popover hangs from it, and AppKit hides popovers on a change of Space.
