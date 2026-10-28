# Persist runtime state; release every force on a clean exit

Nightcap restarts at every update and at login, and, under a LaunchAgent, after a crash. Each restart would drop the user's runtime intent: the temporary rules they set at each place, and the switch. Separately, a stopping app can't tell a restart from being removed; if it leaves a force in place and never comes back, Night Shift stays where Nightcap held it.

## Decision

`$XDG_STATE_HOME/nightcap/state.json` holds every rule but the apps' and sites' indefinite ones, which the config and `rules.toml` keep, each with its end and who set it (ADR-0018), and the Night Shift state Nightcap was holding. On start, the core drops what has ended and decides again. Night Shift found where the last run left it, held before a crash or released on a clean exit, is Nightcap's own doing, not an outside change. On quit or SIGTERM, the shell asks the core to release every force, an app's rule's off or on as much as a temporary rule's, and to write the file.

## Consequences

- An update while a rule is active makes Night Shift flash its other state for a moment.
- After a crash or panic without a restart, Night Shift stays as it was, at worst until the next schedule transition.

## Rejected alternatives

- **Persist nothing, or only the rules until a time**: updates and reboots lose the user's intent.
- **Skip the release on exit**: the daemon can't tell a restart from being removed.
