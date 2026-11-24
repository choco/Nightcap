# Changelog

What changed in each version of Nightcap.

## 1.2.4 · 24 November 2026

After onboarding, Nightcap waits for macOS to show its moon before saying the menu bar has no room for it, and points at the moon if room frees up later.

### Fixed

- After onboarding, “No room for Nightcap’s moon” no longer shows when macOS is only slow to put the moon in the menu bar.
- If the menu bar has no room for the moon after onboarding and you make some, by quitting an app with an icon up there, “Nightcap is up here” takes the note’s place and points at the moon.

## 1.2.3 · 17 November 2026

The note that points out Nightcap’s moon after onboarding stays on the moon when you move it, and onboarding, rules and messages get a few smaller fixes. This release also simplifies Nightcap’s internals.

### Fixed

- After onboarding, “Nightcap is up here” follows the moon when you ⌘-drag it or other menu bar icons come and go.
- Nightcap waits until onboarding is done to tell you about problems, so macOS asks about notifications at onboarding’s own step rather than ahead of it.
- While ad-hoc rules are off, Always and Remove Your Rule in the menu, Add App and Add Website in Settings, and Add Rules in onboarding are greyed out, instead of letting you try and then refusing.
- A website with only a temporary rule shows its icon, as one with a lasting rule does.
- File paths in Settings’ checks, notifications and the command line’s replies show your home folder as `~`.
- During the tour, the command line says a change lasts until the tour ends.

## 1.2.2 · 11 November 2026

When you leave an app or site that turns Night Shift off, Night Shift comes back partway through the schedule’s fade in, as macOS has it, instead of at full warmth.

### Fixed

- Leaving an app or site whose rule turns Night Shift off, in the half hour Night Shift fades in after sunset (two minutes with custom hours), brings it back where the schedule’s fade is, and the fade carries on from there.

## 1.2.1 · 30 October 2026

Night Shift follows macOS’s schedule more reliably, and the shortcut’s peek restores the screen as it found it. This release also simplifies Nightcap’s internals.

### Fixed

- Nightcap leaves the schedule’s gradual transitions to macOS and distinguishes them from changes you make in Control Center or System Settings.
- Holding the shortcut shows the other side of Night Shift even while it’s off. Letting go restores the tint as it was, including partway through a scheduled transition.
- Temporary rules end on time after the clock is set back. A new temporary change replaces the previous one at its place, and everywhere’s indefinite rule comes back when a temporary change ends.
- A website’s rule can take effect even when its browser has a rule of its own.
- Onboarding handles notification permission changes safely and still asks about apps when the config leaves them to you. The tour handles quick Continue clicks and early exits safely.
- Configuration errors point to the file and line that need attention, and command-line replies explain refusals and snoozed rules more clearly.

If your config customizes the menu’s lengths, move `lengths` out of `[menu]` to the top of the file, before any table.

The command-line JSON format has changed. Scripts that read it may need updating; the current format includes a schema version.

## 1.2.0 · 26 October 2026

After an update, Nightcap tells you what’s new.

### New

- The first time Nightcap opens after an update, a notification says so; click it to see what changed.
- The update window shows what a new version changes, and every version’s notes are at nightcap.choco.me/changelog.

## 1.1.0 · 25 October 2026

Nightcap picks up sunset to sunrise right after login, and tells you when Location Services keeps it from running.

### New

- With Location Services off, macOS turns sunset to sunrise off until it’s back on. Nightcap now says so in the menu and in Settings, and offers to open Location Services or to keep the schedule off.
- If macOS still hasn’t worked out sunrise and sunset ten minutes on, Nightcap tells you. Custom hours work without them.

### Fixed

- Right after login, the menu could offer to turn Night Shift on in the app in front while it was already on. Until macOS works out sunrise and sunset, the menu now shows Night Shift as it is, and Nightcap follows the schedule within a minute of macOS having it.

## 1.0.0 · 22 October 2026

The first stable release.
