# One global shortcut turns Night Shift on or off with a tap, and shows true colour while held

Every action is a CLI command that skhd, Raycast or Karabiner can bind, so built-in global shortcuts were left out. But the two things people reach for most are turning Night Shift on or off and seeing, for a moment, what a warm screen really looks like, and onboarding's tour wants a shortcut to end on. CoreBrightness can drop the tint while Night Shift stays on: `BrightnessSystemClient`'s `BlueLightReductionFactor` set to 0 shows true colour, and 1 brings the warmth back (checked on macOS 27).

## Decision

- Nightcap has one global shortcut, ⌃⌥⌘N unless the user records another or clears it in Settings. It's registered through KeyboardShortcuts, on Carbon's hotkey API, which needs no permission.
- A tap, let go within 0.3 seconds, sends `Toggle`: everywhere's switch, as the menu's Night Shift item is (ADR-0018). The press peeks at once, so a tap that turns Night Shift off shows no flash of warmth; the tap ends the peek on the toggle's reply, once its own outcome is carried out, never on another input's that was already on its way (ADR-0002).
- Holding it is the peek: from the press, the screen shows the other side of Night Shift until the key goes up, true colour while it's on and its warmth at the set strength while it's off. The peek sets the factor and nothing else, so Night Shift stays as it is, the core decides nothing, and no status callback arrives. It belongs to the shell alone.
- The factor is the tint macOS fades at each switch, so it can stand partway, as during one of the schedule's fades, and it tints the screen even while Night Shift is off. When the key goes up, and when Nightcap quits, Nightcap puts it back as the peek found it, which a fade of the schedule's then carries on from; where Night Shift switched meanwhile, to what that calls for. It outlives Nightcap, so at launch a factor of 0 with Night Shift on, or above 0 with it off, is a peek that a crash cut short, and Nightcap puts it right. A tap that turns Night Shift off leaves the factor at 0, and one that turns it on leaves it at 1, as switching Night Shift does by itself.
- The menu shows the shortcut beside its Night Shift item, which turns Night Shift on or off.

## Consequences

- Another private CoreBrightness property, which every macOS version Nightcap claims needs checking for (ADR-0013).
- The shortcut lives in Nightcap's user defaults, not the config, so a config can't set it.
- Until Nightcap starts again, a crash mid-peek leaves the screen in true colour with Night Shift on, or warm with it off.

## Rejected alternatives

- **Peeking by switching Night Shift off and on**: the core would take both as the user's own changes, and macOS fades each switch, so the peek would be slow.
- **A global event monitor**: it needs Input Monitoring or Accessibility, for what Carbon's hotkeys do without.
- **No default**: nothing to point to in onboarding or the tour, and nothing works until the user visits Settings.
- **A separate shortcut for the peek**: two shortcuts to learn and keep free, where the press itself tells a tap from a hold.
