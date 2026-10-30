# Changelog

What changed in each version of Nightcap.

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
