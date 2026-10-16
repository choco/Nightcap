# Nightcap

A macOS app that switches Night Shift off for chosen apps and websites: a Swift shell (`app/`) that owns every Apple API, around a platform-free Rust core (ADR-0001).

## Development

Work inside `nix develop`. It provides the pinned Rust toolchain, points cargo at Xcode's compiler and linker, and puts `scripts/` on `PATH`. CI runs the same scripts by path, so changing a script changes CI.

- `build-core [--release]`: builds the core and the CLI, and regenerates `app/NightcapCore` (the xcframework and the Swift bindings). The Swift side sees a change to the core's UniFFI exports only after this runs.
- `build-app [--release]`: builds `build/xcode/<Debug|Release>/Nightcap.app`, unsigned. Debug is the dev build (`io.github.choco.nightcap.dev`).
- `run-dev`: builds, signs and launches the dev build.
- `run-tests`: runs the Rust tests.
- `render-icons`: renders `app/icons/AppIcon.svg` into the app's icon set. The menu-bar moons are SVGs in `app/Nightcap/Assets.xcassets`, used as they are.

To compare the UI against a mockup, a dev build renders the settings window's tabs, light and dark, from a fixed scenario into a folder and quits: `NIGHTCAP_SNAPSHOT="$PWD/tmp/snap" build/xcode/Debug/Nightcap.app/Contents/MacOS/Nightcap`. It needs no core and no screen-recording permission (`app/Nightcap/Snapshot.swift`).

Swift files under `app/Nightcap/` join the app target automatically (a folder-synced group); the project file changes only for settings, build phases and packages. Signing goes through `release/sign` alone, with Xcode's own signing off. `build/`, `target/`, the xcframework and `NightcapCore.swift` are build outputs: change their sources and rebuild.

## Agent skills

### Domain docs

Multi-context: `GLOSSARY-MAP.md` indexes one `GLOSSARY.md` per bounded context, with ADRs under `docs/adr/`. Read `docs/agents/domain.md` before exploring code, naming a domain concept, or recording a term or decision.
