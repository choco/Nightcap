# Nightcap

A macOS app that switches Night Shift off, or on, for chosen apps and websites: a Swift shell (`app/`) that owns every Apple API, around a platform-free Rust core (ADR-0001).

## Development

Work inside `nix develop`. It provides the pinned Rust toolchain, points cargo at Xcode's compiler and linker, and puts `scripts/` on `PATH`. CI runs the same scripts by path, so changing a script changes CI.

- `build-core [--release]`: builds the core and the CLI, and regenerates `app/NightcapCore` (the xcframework and the Swift bindings). The Swift side sees a change to the core's UniFFI exports only after this runs.
- `build-app [--release]`: builds `build/xcode/Release/Nightcap.app` or the dev build, `build/xcode/Debug/Nightcap Dev.app` (`io.github.choco.nightcap.dev`), unsigned. The dev build's own name keeps it apart from the installed app in System Settings.
- `run-dev`: builds, signs and launches the dev build.
- `run-tests`: runs the Rust tests.
- `check [--rust] [--site]`: runs what CI checks, every area unless flags name some: rustfmt, clippy, rustdoc and the tests (`--rust`), and `build-site` (`--site`). CI's jobs call it by area.
- `release-notes [<version>]`: prints a version's notes from `CHANGELOG.md`, written for users: under a `## 1.1.0 · 25 October 2026` heading, a one-paragraph headline, then New and Fixed. The commit that bumps the version adds them: the release workflow fails without them, and the site build fails on a heading in another shape. They become the GitHub release, Sparkle's update window and the site's changelog, which the notification the first launch after an update posts opens.
- `build-site`: installs, lints, type-checks and builds the project site into `build/site`, prerendered to static HTML. Preview it with `python3 -m http.server -d build/site`. The download buttons lead to the releases page unless `VITE_RELEASE_VERSION` and `VITE_RELEASE_URL` name a release's version and DMG, as the Pages deploy sets them.
- `render-dmg-background`: renders `release/dmg-assets/background.html` into the DMG's background PNGs, with Google Chrome. `release/dmg` builds the DMG from a signed app.
- `render-icons`: renders `app/icons/AppIcon.svg` into the app's icon set and the site's touch icon, and `TourSample.svg` into the tour sample's. The menu-bar moons are SVGs in `app/Nightcap/Assets.xcassets`, used as they are.
- `render-og-image`: renders the site's social card, `site/public/og.png`, from the built page, with Google Chrome. Render it again when the hero changes.

To compare the UI against a mockup, a dev build renders the settings window's tabs, onboarding's steps, the tour's panels and the pointer to the moon, light and dark, from a fixed scenario into a folder and quits: `NIGHTCAP_SNAPSHOT="$PWD/tmp/snap" "build/xcode/Debug/Nightcap Dev.app/Contents/MacOS/Nightcap Dev"`. It needs no core and no screen-recording permission (`app/Nightcap/Snapshot.swift`).

The site (`site/`) is React and TypeScript, built with Vite and pnpm, and prerendered at build time (`prerender.ts`), so each page arrives whole and React hydrates it. `pnpm --dir site dev` serves it, rendered the same way, with hot reload. Its pages are the home page, the privacy policy, the terms, the changelog (rendered from `CHANGELOG.md` by `plugins/changelog.ts`) and the 404 page that Pages serves for any missing path: each is an HTML file with its own entry in `src/entries/`, and `src/render.tsx` maps each HTML file to the page it prerenders. Neither `vite preview` nor `python3 -m http.server` serves `404.html` for a missing path the way Pages does: check the 404 page at `/404.html`. Tailwind styles its layout and type, themed from the design system's tokens; the drawn mocks (the macOS scene, menus, desk) keep their measures in CSS modules beside their components. `site/plugins/design.ts` writes `src/styles/tokens.css` from `design/tokens.json` and copies the app icon to `public/icon.svg`; both are build outputs.

The site's dependencies are pinned to exact versions at least two weeks old; `site/pnpm-workspace.yaml` enforces the age for transitive ones too. Biome lints and formats the site with its recommended defaults (`pnpm --dir site check`, and `pnpm --dir site format` to apply fixes); it stands in for ESLint because typescript-eslint doesn't support TypeScript 7 yet.

The site's ambient motion is free in Safari only while WebKit's compositor runs all of it. A single animation WebKit runs on its main thread (a `reverse` direction, an animated `box-shadow` or mask, a script loop), any listener for animation events (`src/hydrate.tsx` turns React's away), or a moving layer the page's size with pixels to draw has it restyle every animated element on the page, every frame: the doodles then cost a sixth of a core. A transition, even of opacity, keeps that main thread busy for as long as it runs, the more so the more animations the page has, and one of `box-shadow` or a colour about three times as much: a demo that loops while in view, as the Desk and the Shortcut do, costs a few percent of a core. Keep motion to transform and opacity keyframes, move things after the first paint only by transform (a box that changes its size or place is a layout shift, which Lighthouse scores in Performance and in Agentic Browsing), and check a change's idle CPU in Safari and its CLS in Lighthouse.

The privacy policy (`site/src/Privacy.tsx`) says what Nightcap keeps, where, and what it sends over the network. A change to any of these, in the app or the site, changes the policy and its date in the same commit.

Swift files under `app/Nightcap/` join the app target automatically (a folder-synced group); the project file changes only for settings, build phases and packages. Signing goes through `release/sign` alone, with Xcode's own signing off. `build/`, `target/`, the xcframework and `NightcapCore.swift` are build outputs: change their sources and rebuild.

## Design

Brand and UI work start from `design/README.md`, the design system's brand book, with its tokens, fonts and component previews beside it.

## Agent skills

### Domain docs

Multi-context: `GLOSSARY-MAP.md` indexes one `GLOSSARY.md` per bounded context, with ADRs under `docs/adr/`. Read `docs/agents/domain.md` before exploring code, naming a domain concept, or recording a term or decision.
