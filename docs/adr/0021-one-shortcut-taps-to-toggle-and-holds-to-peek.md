# One global shortcut turns Night Shift on or off with a tap, and shows true colour while held

Every action is a CLI command that skhd, Raycast or Karabiner can bind, so built-in global shortcuts were left out. But the two things people reach for most are turning Night Shift on or off and seeing, for a moment, what a warm screen really looks like, and onboarding's tour wants a shortcut to end on. CoreBrightness can drop the tint while Night Shift stays on: `BrightnessSystemClient`'s `BlueLightReductionFactor` set to 0 shows true colour, and 1 brings the warmth back (checked on macOS 27).

## Decision

- Nightcap has one global shortcut, ⌃⌥⌘N unless the user records another or clears it in Settings. It's registered through KeyboardShortcuts, on Carbon's hotkey API, which needs no permission.
- A tap, let go within 0.3 seconds, sends `Toggle`, which turns Night Shift the other way as the menu's Night Shift item does, so a tap during a force is an override (ADR-0006).
- Holding it is the peek: from the press, the screen shows true colour until the key goes up. The peek sets the factor and nothing else, so Night Shift stays on, the core decides nothing, and no status callback arrives. It belongs to the shell alone.
- The factor outlives Nightcap, and tints the screen even while Night Shift is off. So Nightcap puts it back to 1 only while Night Shift is on: when the key goes up, when Nightcap quits, and at launch, where a factor of 0 with Night Shift on is a peek that a crash cut short. A tap that turns Night Shift off leaves the factor at 0, and switching Night Shift on sets it back by itself.
- The menu shows the shortcut beside its Night Shift item, which turns Night Shift on or off.

## Consequences

- Another private CoreBrightness property, which every macOS version Nightcap claims needs checking for (ADR-0018).
- The shortcut lives in Nightcap's user defaults, not the config, so a config can't set it.
- Until Nightcap starts again, a crash mid-peek leaves the screen in true colour with Night Shift on.

## Rejected alternatives

- **Peeking by switching Night Shift off and on**: the core would take both as the user's own changes, and macOS fades each switch, so the peek would be slow.
- **A global event monitor**: it needs Input Monitoring or Accessibility, for what Carbon's hotkeys do without.
- **No default**: nothing to point to in onboarding or the tour, and nothing works until the user visits Settings.
- **A separate shortcut for the peek**: two shortcuts to learn and keep free, where the press itself tells a tap from a hold.
