# Persist runtime state; release every force on a clean exit

launchd restarts the app at every update, after a crash and at login. Each restart would drop the user's runtime intent: the manual state, snoozes, a timed disable. Separately, a stopping app can't tell a restart from being removed; if it leaves a force in place and never comes back, Night Shift stays off.

## Decision

`$XDG_STATE_HOME/nightcap/state.json` holds the manual state, the snoozes, the timed-disable deadline and the current forces, each with its expiry. On start, the core drops expired entries and re-evaluates. On SIGTERM, the shell asks the core to release every force and write the file.

## Consequences

- An update while a rule is active makes Night Shift flash on for a moment.
- After a crash or panic without a restart, Night Shift stays as it was, at worst until the next schedule transition.

## Rejected alternatives

- **Persist nothing, or only the timed disable**: updates and reboots lose the user's intent.
- **Skip the release on exit**: the daemon can't tell a restart from being removed.
