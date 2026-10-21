# Rules say off or on, one per app or site, and the most specific decides

ADR-0008 had rules only force Night Shift off. The rule engine's answer was then force off or hands off, so undoing a force only ever meant undoing an off. It rejected rules forcing Night Shift on because they turn it on by day and clash with timed disables. Saying where and for how long (ADR-0024) asks for an app's or site's "always" both ways: Books warm all day, Figma never warm. Its layers also settle the clash, since a place's timer sits over its rule.

## Decision

- A rule says off or on, while its app is in front or on its domain.
- A rule's identity is still its kind and target. So an app or domain has one rule at most, whatever it says and wherever it comes from.
- Always, in an app's or site's row, makes the user's own rule say that way, replacing one of theirs that said the other. A config rule can't be replaced or removed from the menu, Settings or the CLI. Its Always is greyed, saying the config decides, and changing it means changing the config.
- The config lists rules by what they say, in `[rules.off]` and `[rules.on]`, each with `app` and `domain`. An identity in both makes the config invalid. The flat `[rules]` lists go.
- An ad-hoc rule graduates when its identity appears in the config, whatever either says.
- Where several domain rules cover the host, the longest domain decides. A site's rule decides over its browser's app rule, as a site decides over its app (ADR-0024).
- A rule that says on turns Night Shift on by day while its app is in front, and Nightcap puts it back off when the app leaves.
- `nightcap rules add <identity> [--on]` makes a rule that says on. Export writes both lists.

This supersedes in part ADR-0008, for rules only forcing off and for rejecting a most-specific match, and ADR-0009, for the config's rule lists.

## Consequences

- The rule engine decides on or off, from the place that decides, rather than force off or hands off. On a clean exit, Nightcap releases a rule's on as it releases an off (ADR-0011).
- A config with the flat `[rules]` lists stops loading, with an error that names the new tables. The README, the docs and the nix module's output change with it.
- Settings' rule list and `nightcap rules list` show what each rule says.
- A config rule can only be changed for good in the config, and snoozed meanwhile (ADR-0024).

## Rejected alternatives

- **Rules that only force off**, leaving "always on in one app" out: the user's for-good choices would go one way only.
- **On-exceptions as a separate kind over off rules** (rejected in ADR-0008 too): a rule that says on is the same concept with the other state.
- **Keeping the flat lists for off and adding `[rules.on]`**: two spellings for one thing, one of them lopsided.
- **An ad-hoc rule overriding a config rule with the same identity**: Nightcap would quietly override the config, which ADR-0009 rejected.
