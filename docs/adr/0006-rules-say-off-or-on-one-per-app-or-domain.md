# Rules say off or on, one per app or domain; domains match on label boundaries, and the most specific decides

Nightcap's rules say what Night Shift does while a given app is in front, or while the front browser tab is on a given site: never warm in Figma, warm all day in Books. That leaves which kinds of rule exist, what a rule can say, how a host matches a domain, and which rule decides when several apply.

## Decision

- There are two kinds of rule. **App rules** apply while the app is frontmost. **Domain rules** apply on an entry and its subdomains, matched on label boundaries: `figma.com` matches `www.figma.com`, but not `notfigma.com`.
- A rule says off or on. One that says on turns Night Shift on by day while its app is in front, and Nightcap puts it back off when the app leaves.
- A rule's identity is its kind and target, so an app or domain has one rule at most, whatever it says and wherever it comes from.
- Where several domain rules cover the host, the longest domain decides. A site's rule decides over its browser's app rule, as a site decides over its app (ADR-0018). To cover only part of a site, write a narrower rule.
- Always, in an app's or site's row, makes the user's own rule say that way, replacing one of theirs that said the other. A config rule can't be replaced or removed from the menu, Settings or the CLI: its Always is greyed, saying the config decides, and changing it means changing the config.
- The config lists rules by what they say, in `[rules.off]` and `[rules.on]`, each with `app` and `domain`. An identity in both makes the config invalid, and lists straight under `[rules]` fail with an error that names the two tables.
- `nightcap rules add <identity> [--on]` makes the user's rule, saying off unless told on. Export writes both lists.

## Consequences

- The rule engine decides on or off, from the place that decides. On a clean exit, Nightcap releases a rule's on as it releases an off (ADR-0008).
- Settings' rule list and `nightcap rules list` show what each rule says.
- A config rule can only be changed for good in the config, and snoozed meanwhile (ADR-0018).

## Rejected alternatives

- **Rules that only force off**: undoing a force would only ever mean undoing an off, but the user's for-good choices would go one way only. A place's timer sits over its rule (ADR-0018), so a rule that says on no longer clashes with a change for a while.
- **Exceptions as a separate kind**, an on over an off rule, or a subdomain cancelling its domain's rule: a second concept, where a rule that says the other way on a narrower domain is the same one.
- **Running-app rules, applying while an app is open at all**: few real uses, and they add launch and quit tracking to what the rule engine matches against.
- **Matching by registrable domain**: entries like `github.io` or `maps.google.com` would match nothing.
- **A separate exact-host rule kind**: redundant once subdomains match.
- **Flat lists for off, with `[rules.on]` beside them**: two spellings for one thing, one of them lopsided.
- **An ad-hoc rule overriding a config rule with the same identity**: Nightcap would quietly override the config (ADR-0007).
