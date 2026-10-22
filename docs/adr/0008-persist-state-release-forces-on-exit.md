# Persist runtime state; release every force on a clean exit

Nightcap restarts at every update and at login, and, under a LaunchAgent, after a crash. Each restart would drop the user's runtime intent: the holds and timers they set on each place. Separately, a stopping app can't tell a restart from being removed; if it leaves a force in place and never comes back, Night Shift stays where Nightcap held it.

## Decision

`$XDG_STATE_HOME/nightcap/state.json` holds each place's hold and timer with its end (ADR-0018), and the Night Shift state Nightcap was holding. On start, the core drops what has ended and decides again. Night Shift found where the last run left it, held before a crash or released on a clean exit, is Nightcap's own doing, not an external change. On quit or SIGTERM, the shell asks the core to release every force, a rule's off or on as much as a hold or a timer, and to write the file.

## Consequences

- An update while a rule is active makes Night Shift flash its other state for a moment.
- After a crash or panic without a restart, Night Shift stays as it was, at worst until the next schedule transition.

## Rejected alternatives

- **Persist nothing, or only the timers**: updates and reboots lose the user's intent.
- **Skip the release on exit**: the daemon can't tell a restart from being removed.
