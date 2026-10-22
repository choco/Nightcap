# Nightcap runs on macOS 15 and later, on Apple silicon

Nightcap was built for, and checked on, macOS 27 alone. A lower deployment target lets more Macs run it, but every version it claims costs fallbacks for newer APIs and a check, on that version, of the private CoreBrightness API and of the browsers' Accessibility trees. Xcode 27 builds for macOS 12 at the oldest, and `SMAppService`, which opening at login relies on (ADR-0011), needs 13. Apple in practice still ships security fixes for the current macOS and the two before it: 27, 26 and 15. macOS 27 runs only on Apple silicon; 15 and 26 also run on some Intel Macs.

## Decision

- The deployment target is macOS 15.0, for arm64 alone.
- Code that needs a newer macOS checks with `#available` and falls back. So far that's the badge on a Settings tab with a problem in it, which needs macOS 26, so on 15 the problem shows only in its pane; and keeping every menu item's image showing, which macOS 27 would otherwise hide for some.
- CI keeps building with Xcode 27 and the macOS 27 SDK. The deployment target sets the floor, not the SDK.
- Each version Nightcap claims gets the checks macOS 27 got: Night Shift through CoreBrightness, website checking in Safari and Chrome, and how the windows look.

## Rejected alternatives

- **macOS 13 or 14**: more Macs, but each step down adds fallbacks (bringing a window to the front, the menu's section headers and subtitles, the lock behind core wake-ups) and another version to check, for versions Apple no longer patches.
- **macOS 12**: without `SMAppService`, opening at login would need the deprecated helper-app login item.
- **Intel as well, as a universal build**: Intel Macs on 15 and 26 could run Nightcap, but there's no Intel Mac to check Night Shift on, and every release would build and check twice.
