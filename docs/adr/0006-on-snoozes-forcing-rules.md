---
status: superseded by ADR-0024
---

# `on` during a force snoozes the forcing rules; external changes are adopted the same way

When a rule forces Night Shift off and the user turns it on anyway, Nightcap has to decide what that means and for how long. The answer has to survive Cmd-Tabbing away and back, and must not cost the user their rules. Nightcap can't block Control Center, so an external change always takes effect; the only question is how to interpret it.

## Decision

When `on` arrives while Night Shift is forced off:

- the rules forcing it off at that moment are snoozed;
- a running timed disable is cancelled;
- the manual state becomes on.

Snoozes and the manual state expire at the next schedule transition. Other rules still apply. `resume` clears the timed disable, every snooze and the manual state. This matches macOS's own "until sunrise" behaviour for manual toggles.

An external change is handled exactly like the equivalent command. If it collides with a force, a notification says what the core took it to mean, e.g. "turned on outside Nightcap, Photoshop rule snoozed until sunrise".

## Consequences

- Nightcap needs notification permission. If it is denied, conflict notices appear only in `status` and the logs.

## Rejected alternatives

- **Deleting the forcing rules**: a moment's change of mind permanently loses configuration.
- **The snooze lasts until the context changes, or until the forcing rule stops matching**: Cmd-Tabbing away and back undoes it.
- **`on` beats every rule until the next schedule transition**: one keypress turns off all automation for the night.
- **`on` only sets the manual state while rules still win**: `on` would appear to do nothing.
- **Reverting an external change, and notifying**: Night Shift visibly flips back, and if the core is wrong it fights the user, leaving the CLI as the only way out.
- **Adopting external changes silently**: the user never learns what the core took the toggle to mean.
