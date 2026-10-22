# One background app: a Swift shell around a platform-free Rust core

Nightcap depends on several macOS-only facilities: NSWorkspace notifications, which only arrive while the main run loop runs; Accessibility observers; private CoreBrightness calls; a menu bar and notifications. Its permission grants (Accessibility, notifications) are tied by macOS to a certificate-signed bundle with a stable bundle ID. The rules, decisions and schedule math, by contrast, are pure logic that should be testable without a Mac.

## Decision

`Nightcap.app` is one menu-bar process (`LSUIElement`), started at login by its installer's LaunchAgent or by its own login item (ADR-0011). It shows in the Dock only while one of its windows, such as onboarding or Settings, is open. It is built in two layers:

- **The Swift shell** owns every Apple API: the app lifecycle, the menu bar and the windows, notifications, NSWorkspace, Accessibility and the browser watcher, and CoreBrightness.
- **The Rust core** is a static library linked into the same process. It owns the rules, the decisions, the schedule math, config and persistence, and the IPC server. It never calls a macOS API.

The `nightcap` CLI is a separate Rust binary that shares a protocol crate with the core. It needs no privacy permissions.

This follows Ghostty: the core never owns the app lifecycle or system services. AppKit is idiomatic from Swift, so the menu bar and the strength slider are cheap, and a platform-free core runs its tests and most of CI on Linux.

## Consequences

- Two languages and an FFI layer (ADR-0002).
- The main thread must never block, which is why browser reads run on their own thread (ADR-0012).
- The shell needs Apple's toolchain, so the app is built in CI rather than in Nix (ADR-0003).

## Rejected alternatives

- **All Rust with `objc2`**: one toolchain and no FFI, but AppKit code is verbose and full of `unsafe` (Glide's status bar alone is about 300 lines with 16 `unsafe` blocks), and every OS-facing piece would be fought from Rust.
- **Swift for the UI only, with Rust doing the OS integration**: both languages would own parts of the main run loop.
- **A bare LaunchAgent binary from the Nix store**: grants are tied to the store path or build hash, so every update loses them ([nixpkgs#517790](https://github.com/NixOS/nixpkgs/issues/517790)).
- **Separate processes for the UI, the browser watcher and the core**: more lifecycles and internal protocols, with nothing gained.
- **A regular app with an embedded menu-bar helper**, so the app gets focus at launch without forcing it: the helper needs its own bundle identity, so the grants, Sparkle and the login item would all move to it, and onboarding's permission steps would run through it over the socket. A menu-bar app can force its onboarding window to the front instead.
