# A write during one of the schedule's fades says how its tint goes on

ADR-0004 leaves the schedule's transitions to macOS, which fades them where a write fades in two seconds, and has the core write only where the schedule alone decides over a write of its own since the last transition: Nightcap giving Night Shift back to the schedule after a rule stopped deciding. But macOS's fade lasts longer than ADR-0004 says, and a write cuts it short. CoreBrightness moves the tint in a straight line, every two seconds, over the half hour after sunset for sunset to sunrise and the two minutes after custom hours start; it fades out over as long before the schedule switches Night Shift off (`-[NightModeControl setMode:]` sets the lengths; measured on macOS 27). A write marks an override, after which CoreBrightness drops its fade in and fades to full in two seconds; its fade out goes on over a write. So leaving an app whose rule turned Night Shift off, ten minutes after sunset, brought Night Shift back at full strength, where the schedule had it at a third.

## Decision

A write that gives Night Shift back to the schedule during one of the schedule's fades says how the tint goes on: `SetNightShift { tint }`, straight lines between points in time, from where the tint shows to the last point, along what's left of the fade, rising after the switch-on or falling before the switch-off. The core decides it as it decides the write. Any other write says nothing, and macOS fades it as a switch, over two seconds. The core already computes the schedule (ADR-0004), so the lengths of macOS's fades sit beside it.

The shell's adapter plays the curve from the report that first shows the write: every two seconds, a two-second fade of the tint (`BlueLightReductionFactor`, the peek's, ADR-0015) to where the curve will be, as macOS moves along its own fades, while the write's mark stands: until the curve ends, another write lands, or the schedule clears the mark of writes. A peek holds the steps while it lasts. Quitting leaves the rest to macOS as one fade to the curve's next point, since nothing steps after Nightcap exits.

A curve the user picks, such as a slower fade in, would be a curve from the core on writes that now say nothing, with nothing new in the shell.

## Consequences

- The lengths and the shape of macOS's fades are CoreBrightness's own, read from its code, so each macOS version Nightcap claims needs them checked as the private methods are (ADR-0013).
- Until the write's report arrives, a moment after it, the tint starts along the write's own fade to full; CoreBrightness starts each fade from the tint it shows, so the first step carries on from there.
- Quitting hands over a curve whose write no report shows yet as it is, and the hand-over can reach CoreBrightness before the write, over another connection, for the write's own fade to replace.
- The adapter moves the tint only while a curve lasts, half an hour at most. CoreBrightness sets the tint whole again as it re-evaluates, at wake or as the display comes on, and the next step puts it back. A curve that lasts hours would need Nightcap to own the tint across those, and across a crash, which leaves it partway.
- A crash, or a Force Quit, while a curve plays leaves the tint partway until CoreBrightness next re-evaluates it, and a relaunch doesn't put it back: Night Shift is as the core wants it, so nothing is written, and the curve was the shell's alone. Taking it up again would mean the core working out the rest of the fade after a launch, for the adapter to play against the standing mark.
- A rule that holds Night Shift on past the schedule's switch-off doesn't hold its tint: macOS's fade out goes on over the rule's write, and once it has switched Night Shift off, the core's write brings it back over two seconds, a dip at every sunrise, or at the end of custom hours. Nothing written before the switch-off survives it, and setting the tint during the fade would fight CoreBrightness, which works each step out from the fade's start rather than from the tint it shows. A rule that holds Night Shift off past the switch-on shows next to nothing, since macOS's fade in starts at the switch.

## Rejected alternatives

- **The write naming the schedule's last transition, the shell working out macOS's fade from it**: the shell had to know that the transition was a switch-on and read the schedule's mode for the fade's length, and each new curve would be another special case in the shell.
- **Taking the write back, so the schedule decides and fades again**: CoreBrightness clears the mark only at the schedule's next switch or as its mode changes. `setMode:` with the mode it has returns at once, and switching the mode away and back is a schedule edit, which ends the rules waiting on the schedule's turn (ADR-0018) and flips Night Shift twice.
- **One fade to the curve's end**: CoreBrightness eases every fade along its `_rampProfile`, slow, fast, then slow, so a long fade strays from the line by up to a fifth of the way. Quitting falls back to it only because nothing steps afterwards.
- **The core stepping the tint through effects**: a step every two seconds would be a turn of the core every two seconds for half an hour.
- **Every write saying how its tint fades, a switch's two seconds too**: the shell played each switch's fade over the one macOS makes by itself, and every write depended on the clock.
- **Stepping as the write is sent**: the first step can reach CoreBrightness before the write, over another connection, whose own fade then replaces it, so the write's report had to step again.
