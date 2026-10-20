# The tour turns Night Shift on and adds its sample's rule only while it runs

The tour, offered at the end of onboarding and from Settings, walks through the moon, a rule at work, the peek and the menu in a small panel by the menu-bar moon. A rule at work needs an app with a rule in front; when none of the user's is running, the tour opens a sample window of its own, and that window needs a rule. And the moon, the rule and the peek show nothing while Night Shift is off, as it is all day: the tour has to warm the screen and give it back as it found it. Both would otherwise reach the user's own state: an ad-hoc rule left in `rules.toml`, or a manual state, or an external change the core adopts.

## Decision

- The shell tells the core when the tour starts and ends. Neither is a command: the CLI has no tour.
- When the tour opens its sample window, the core holds a rule for it, with its own source, in memory only. The shell reports the sample window as an app of its own while it's key, so the menu, the rule toggle and a snooze treat it as any app with a rule. It never reaches `state.json`, `rules.toml`, Settings' rule list or `rules export`, and the tour's end removes it with any snooze of it.
- When Night Shift is off as the tour starts, the tour stands in for the schedule with on, for its length: it sets aside the manual state and the timed disable, and is the reason under the status line. Night Shift really switches on, so everything the tour shows is real. The user's own choices during the tour still come first, and a tap off then on returns to the tour's on.
- The tour's end, or quitting, puts back what it set aside, short of what the user chose during the tour, and the schedule decides again. Throughout the tour, `state.json` holds the state its end would leave, with Night Shift as the tour holds it: after a crash mid-tour, the next run finds Night Shift where the last run left it, which is no external change (ADR-0011), and switches it back.

## Consequences

- The tour can turn Night Shift on in daytime, which the core otherwise does only on the user's command.
- A rule source, and a reason for Night Shift being on, that exist only while a tour runs.

## Rejected alternatives

- **Warming with the tint alone**, `BlueLightReductionFactor` at 1 with Night Shift off (ADR-0021): no core change, but the moon and the menu would say off while the screen looks warm, and the rule would have nothing to switch off.
- **An ad-hoc rule for the sample**, removed at the end: a crash mid-tour leaves it in `rules.toml`, and Settings lists it meanwhile.
- **The tour only at night**: it would be offered at the end of onboarding, mostly run in daytime, and show nothing.
- **Turning Night Shift on with `on`**: a manual state until the next transition, which the tour's end would have to undo, and which a crash leaves behind.
