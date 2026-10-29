# The core decides by reconciling against a schedule it computes itself

CoreBrightness reports Night Shift status through one callback, and that callback fires for three different things: the echo of Nightcap's own write, a schedule transition, and an outside change. Each needs different handling. An echo needs nothing, a schedule transition ends what waits on it, and an outside change is adopted (ADR-0018). The obvious way to separate them, a one-shot "my write" flag armed by every action, misreads them: the next callback is swallowed even when it was a real schedule transition or a Control Center toggle. Matching callbacks against the times of schedule transitions misreads them too: a toggle near a transition reads as that transition, and a transition macOS performs late, as after waking, reads as an outside change.

## Decision

The core is the only part that decides what Night Shift should be doing: the wanted state. It tells the callbacks apart by the mark CoreBrightness keeps of writes, `BlueLightReductionAlgoOverride` and its time in the `CBBlueReductionStatus` dictionary. Every `setEnabled:`, Nightcap's or Control Center's, marks a new time, even one that changes nothing. macOS's schedule leaves the mark alone, and clears it as it next switches Night Shift against it (checked on macOS 27). So:

- a callback with no new write is macOS's schedule, never an outside change;
- a new write that matches the wanted state needs nothing, so the core never has to recognise the echo of its own writes;
- any other new write is an outside change.

The core still computes the schedule itself, since macOS exposes neither what it says now, which is what Night Shift goes back to whenever a rule stops deciding, nor when it next turns, which is when a rule until the schedule turns ends. It does the math from two inputs the shell provides: the custom hours from the status struct, and sunrise and sunset from `BrightnessSystemClient`'s `BlueLightSunSchedule`. It keeps one timer for the next schedule transition, and recomputes on wake and on clock or time-zone changes. Until CoreBrightness has the sun times, as for a minute or two after login, the timer comes each minute instead, and as going without them becomes a problem, and the shell reads them again each time; meanwhile the core leaves Night Shift's schedule to macOS.

The shell's Night Shift adapter looks up `CBBlueLightClient` and `BrightnessSystemClient` at runtime (`NSClassFromString`). Those two classes are the whole private API surface. An OS update that removes one makes the adapter report "unavailable" instead of crashing at launch, and so does a status without the mark of writes, without which the core can't tell the schedule's switches from the user's.

## Consequences

- The mark of writes is one more private property each macOS version Nightcap claims needs checking for (ADR-0013); so far it's checked on macOS 27 only.

## Rejected alternatives

- **A one-shot "my write" flag**: it misreads callbacks, as described above.
- **Matching callbacks against computed transition times**: it misreads them too, as described above, and needs guessed windows around every transition.
- **Inferring schedule transitions from status callbacks**: a transition that changes nothing fires no callback, and only the schedule says what Night Shift goes back to when a rule stops deciding.
- **The last sun times read, standing in until CoreBrightness has new ones**: while it has none, CoreBrightness schedules by placeholder times of its own, so a stand-in would set the core against macOS.
- **Waiting for CoreBrightness to announce the sun times**: it has no notification for them (as its iOS counterpart shows), and the status callback fires only when their arrival switches Night Shift.
- **The status struct's `active` flag or `setEnabled:withOption:`**: undocumented, and no use showed in testing on macOS 27: `active` stayed true throughout, and options 0 to 2 behaved alike.
- **The `nightlight` crate**: it only polls, never registers the status callback, and depends on the deprecated `objc` 0.2.
