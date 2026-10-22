# Nightcap

A macOS app that switches Night Shift off for chosen apps and websites: a Swift shell (`app/`) that owns every Apple API, around a platform-free Rust core (ADR-0001).

## Development

Work inside `nix develop`. It provides the pinned Rust toolchain, points cargo at Xcode's compiler and linker, and puts `scripts/` on `PATH`. CI runs the same scripts by path, so changing a script changes CI.

- `build-core [--release]`: builds the core and the CLI, and regenerates `app/NightcapCore` (the xcframework and the Swift bindings). The Swift side sees a change to the core's UniFFI exports only after this runs.
- `build-app [--release]`: builds `build/xcode/Release/Nightcap.app` or the dev build, `build/xcode/Debug/Nightcap Dev.app` (`io.github.choco.nightcap.dev`), unsigned. The dev build's own name keeps it apart from the installed app in System Settings.
- `run-dev`: builds, signs and launches the dev build.
- `run-tests`: runs the Rust tests.
- `build-site`: installs, type-checks and builds the project site into `build/site`, prerendered to static HTML. Preview it with `python3 -m http.server -d build/site`; the download buttons need a `download.json` beside it (`{"version", "url"}`), as the Pages deploy writes one.
- `render-dmg-background`: renders `release/dmg-assets/background.html` into the DMG's background PNGs, with Google Chrome. `release/dmg` builds the DMG from a signed app.
- `render-icons`: renders `app/icons/AppIcon.svg` into the app's icon set, and `TourSample.svg` into the tour sample's. The menu-bar moons are SVGs in `app/Nightcap/Assets.xcassets`, used as they are.

To compare the UI against a mockup, a dev build renders the settings window's tabs, light and dark, from a fixed scenario into a folder and quits: `NIGHTCAP_SNAPSHOT="$PWD/tmp/snap" "build/xcode/Debug/Nightcap Dev.app/Contents/MacOS/Nightcap Dev"`. It needs no core and no screen-recording permission (`app/Nightcap/Snapshot.swift`).

The site (`site/`) is React and TypeScript, built with Vite and pnpm, and prerendered at build time (`prerender.ts`), so the page arrives whole and React hydrates it. `pnpm --dir site dev` serves it, rendered the same way, with hot reload. Tailwind styles its layout and type, themed from the design system's tokens; the drawn mocks (the macOS scene, menus, desk) keep their measures in CSS modules beside their components. `site/plugins/design.ts` writes `src/styles/tokens.css` from `design/tokens.json` and copies the app icon to `public/icon.svg`; both are build outputs.

Swift files under `app/Nightcap/` join the app target automatically (a folder-synced group); the project file changes only for settings, build phases and packages. Signing goes through `release/sign` alone, with Xcode's own signing off. `build/`, `target/`, the xcframework and `NightcapCore.swift` are build outputs: change their sources and rebuild.

## Design

Brand and UI work start from `design/README.md`, the design system's brand book, with its tokens, fonts and component previews beside it. The system is also a Design System artifact on claude.ai (https://claude.ai/artifact/ACj6WANfTwWJ9LJRZux9uR), which keeps the same files under `project/` and holds the app icon and menu-bar moons as uploads. `design/` is the source: change it there, then publish the changed files to the artifact from a copy staged as `tmp/design/project/`.

## Agent skills

### Domain docs

Multi-context: `GLOSSARY-MAP.md` indexes one `GLOSSARY.md` per bounded context, with ADRs under `docs/adr/`. Read `docs/agents/domain.md` before exploring code, naming a domain concept, or recording a term or decision.
