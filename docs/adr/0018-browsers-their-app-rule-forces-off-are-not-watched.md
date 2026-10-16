# A browser its own app rule forces off isn't watched

With website checking on, ADR-0016 watches the frontmost browser whenever one is in front, so the menu can offer a rule for the current site and `status` can show its host. When an app rule covers the browser itself, Night Shift is forced off whatever the tab shows: the host can't change the decision, and a site rule offered then would do nothing.

This replaces ADR-0016's "the frontmost browser is watched whenever one is in front"; the rest of ADR-0016 stands.

## Decision

While an app rule that isn't snoozed covers the front browser, Nightcap doesn't watch it: no observers, no reads. The menu offers no rule for the site, and `status` shows no host. Snoozing or removing that rule starts watching again, since domain rules can matter again.

## Consequences

- No Accessibility reads at all in a browser the user has forced off as a whole.
- After a snooze or a removal, domain rules apply once a fresh read reports the host, as they do whenever a browser comes to the front.

## Rejected alternatives

- **Watching anyway, so the site's rule stays in the menu**: it offers a rule that can't change anything, and reads every tab for nothing.
