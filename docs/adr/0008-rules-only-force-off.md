---
status: superseded in part by ADR-0023
---

# Two rule kinds that only force off; domains match on label boundaries

Nightcap's rules say when Night Shift must be off: while a given app is in front, or while the front browser tab is on a given site. That leaves three questions: which kinds of rule exist, what a rule can do, and how a host matches a domain entry.

## Decision

There are two kinds of rule:

- **App rules** force Night Shift off while the app is frontmost.
- **Domain rules** force it off on an entry and its subdomains, matched on label boundaries: `figma.com` matches `www.figma.com`, but not `notfigma.com`.

Rules only ever force Night Shift off. The rule engine's output has just two values, force off or hands off, so restoring after a force only ever has to undo "off". To cover only part of a site, write narrower domain rules (`docs.google.com`, `photos.google.com`) rather than one broad rule.

The Public Suffix List (the `psl` crate, with the list compiled in) is used only to turn the current host into a registrable domain when creating a rule from the current tab.

## Rejected alternatives

- **Rules or exceptions that force Night Shift on**: they turn it on during the day, and conflict with timed disables.
- **Domain exceptions that cancel domain rules on a subdomain, with the most specific match winning**: a second concept and a precedence rule, for cases narrower domain rules already express.
- **Running-app rules, forcing Night Shift off while an app is open at all**: few real uses, and they add launch and quit tracking to what the rule engine matches against.
- **Matching by registrable domain**: entries like `github.io` or `maps.google.com` would match nothing.
- **A separate exact-host rule kind**: redundant once subdomains match.
