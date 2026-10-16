# The core decides by reconciling against a schedule it computes itself

CoreBrightness reports Night Shift status through one callback, and that callback fires for three different things: the echo of Nightcap's own write, a schedule transition, and an external change. Each needs different handling. An echo needs nothing, a schedule transition clears snoozes and the manual state, and an external change is adopted (ADR-0006). The obvious way to separate them, a one-shot "my write" flag armed by every action, misreads them: the next callback is swallowed even when it was a real schedule transition or a Control Center toggle.

## Decision

The core is the only part that decides what Night Shift should be doing. It compares every status callback with that desired state:

- if the system already matches, nothing happens;
- if the callback coincides with a computed schedule transition, it is a schedule transition;
- anything else is an external change.

The core never has to recognise the echo of its own writes.

To compute schedule transitions, the core does the schedule math itself from two inputs the shell provides: the custom hours from the status struct, and sunrise and sunset from `BrightnessSystemClient`'s `BlueLightSunSchedule`. It keeps one timer for the next schedule transition, and recomputes on wake and on clock or time-zone changes.

The shell's Night Shift adapter looks up `CBBlueLightClient` and `BrightnessSystemClient` at runtime (`NSClassFromString`). Those two classes are the whole private API surface. An OS update that removes one makes the adapter report "unavailable" instead of crashing at launch.

## Consequences

- Our sun times can differ slightly from CoreBrightness's own, so matching a callback to a schedule transition needs a tolerance window.

## Rejected alternatives

- **A one-shot "my write" flag**: it misreads callbacks, as described above.
- **Inferring schedule transitions from status callbacks**: the same misreading, from the other side.
- **The status struct's `active` flag or `setEnabled:withOption:`**: undocumented, and no use showed in testing on macOS 27: `active` stayed true throughout, and options 0 to 2 behaved alike.
- **The `nightlight` crate**: it only polls, never registers the status callback, and depends on the deprecated `objc` 0.2.
