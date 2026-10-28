# The core decides by reconciling against a schedule it computes itself

CoreBrightness reports Night Shift status through one callback, and that callback fires for three different things: the echo of Nightcap's own write, a schedule transition, and an outside change. Each needs different handling. An echo needs nothing, a schedule transition ends what waits on it, and an outside change is adopted (ADR-0018). The obvious way to separate them, a one-shot "my write" flag armed by every action, misreads them: the next callback is swallowed even when it was a real schedule transition or a Control Center toggle.

## Decision

The core is the only part that decides what Night Shift should be doing. It compares every status callback with that, the wanted state:

- if the system already matches, nothing happens;
- if the callback coincides with a computed schedule transition, it is a schedule transition;
- anything else is an outside change.

The core never has to recognise the echo of its own writes.

To compute schedule transitions, the core does the schedule math itself from two inputs the shell provides: the custom hours from the status struct, and sunrise and sunset from `BrightnessSystemClient`'s `BlueLightSunSchedule`. It keeps one timer for the next schedule transition, and recomputes on wake and on clock or time-zone changes. Until CoreBrightness has the sun times, as for a minute or two after login, the timer comes each minute instead, and as going without them becomes a problem, and the shell reads them again each time; meanwhile the core leaves Night Shift's schedule to macOS.

The shell's Night Shift adapter looks up `CBBlueLightClient` and `BrightnessSystemClient` at runtime (`NSClassFromString`). Those two classes are the whole private API surface. An OS update that removes one makes the adapter report "unavailable" instead of crashing at launch.

## Consequences

- Our sun times can differ slightly from CoreBrightness's own, so matching a callback to a schedule transition needs a tolerance window.

## Rejected alternatives

- **A one-shot "my write" flag**: it misreads callbacks, as described above.
- **Inferring schedule transitions from status callbacks**: the same misreading, from the other side.
- **The last sun times read, standing in until CoreBrightness has new ones**: while it has none, CoreBrightness schedules by placeholder times of its own, so a stand-in would set the core against macOS, and a callback following the placeholders could be adopted as the user's change.
- **Waiting for CoreBrightness to announce the sun times**: it has no notification for them (as its iOS counterpart shows), and the status callback fires only when their arrival switches Night Shift.
- **The status struct's `active` flag or `setEnabled:withOption:`**: undocumented, and no use showed in testing on macOS 27: `active` stayed true throughout, and options 0 to 2 behaved alike.
- **The `nightlight` crate**: it only polls, never registers the status callback, and depends on the deprecated `objc` 0.2.
