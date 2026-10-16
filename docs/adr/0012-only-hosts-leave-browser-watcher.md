# Only hosts leave the browser watcher, and they're logged as private

The browser watcher sees every URL the user visits; that stream amounts to a browsing history. Domain rules need nothing more than the host. Both the shell (`Logger`) and the core (`tracing` through `tracing-oslog`) write to the system log, under the subsystem `io.github.choco.nightcap`.

## Decision

The browser watcher passes only the host to the main thread and the core. A host is logged only when it changed the force, and is formatted as private, so the system log redacts it unless you're debugging. A host that matches nothing is logged at debug level at most. Nightcap's only network access is the daily appcast check of a self-managed install (ADR-0015).

`tracing-oslog` formats every message as public, so the core never logs a host or a domain-rule target: it names domain reasons by kind and source only, e.g. "domain rule (config)". The shell logs the host and the matched domain with `privacy: .private`, and only when an effect changed Night Shift.

## Rejected alternatives

- **Logging every URL change at info level**: it writes the user's browsing history to disk.
- **Logging hosts or matched domains from the core**: `tracing-oslog` can't mark them private, so they would land in the log unredacted.
