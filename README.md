<p align="center"><img src="app/icons/AppIcon.svg" width="112" alt=""></p>

# Nightcap

Night Shift, made yours. Nightcap brings Night Shift to your menu bar. Set how warm it gets, switch it or peek past it with a shortcut, and keep it off while the apps and websites you choose are in front. The rest of the time, your schedule runs as usual.

[Download for Mac](https://nightcap.choco.me/) · macOS 15 or later, Apple silicon · Free and open source

## What it does

- **App rules.** Pick the apps that need true colour, such as Figma or Photoshop. While one is in front, Night Shift stays off.
- **Website rules.** The same for sites in Safari and Chrome, such as `figma.com` and its subdomains. Website checking stays off until you turn it on, and needs Accessibility: Nightcap reads the front tab's address and keeps only the site's name.
- **Your schedule stays yours.** Rules only ever turn Night Shift off. Switch away and it comes straight back, warm as before.
- **It goes along with you.** Turn Night Shift on from the menu, Control Center or a script while a rule holds it off, and Nightcap snoozes that rule until the schedule's next change instead of fighting you.
- **Off for a while.** Turn Night Shift off for 30 minutes, an hour or two from the menu, or for any length with `nightcap disable`, whatever is in front.
- **One shortcut, two moves.** Tap ⌃⌥⌘N in any app to turn Night Shift on or off. Hold it to see true colour for as long as you hold, with Night Shift still on. Record another shortcut in Settings.
- **How warm.** A slider in the menu sets Night Shift's strength, and `nightcap strength` does the same from a script.
- **Made to be scripted.** Everything in the menu is also a `nightcap` command, and rules can live in a config file next to the rest of your setup.

## Install

Download the DMG from [the site](https://nightcap.choco.me/), or the zip from [Releases](https://github.com/choco/Nightcap/releases), and drag Nightcap to Applications. Releases are signed and notarized. The first time it opens, Nightcap sets up Night Shift's strength and schedule with you, suggests apps and websites to keep it off for, and asks about notifications and opening at login; you can change each later in Settings.

A copy in Applications checks for updates once a day and installs them when it quits.

The command line tool is inside the app. Link it into a folder on your `PATH`, such as `~/.local/bin`:

```sh
ln -s /Applications/Nightcap.app/Contents/Helpers/nightcap ~/.local/bin/nightcap
```

Packaging Nightcap for Nix, Homebrew or a script? [`docs/installing.md`](docs/installing.md) is the contract: what a release contains, how to install it so permissions survive updates, and how to start it.

## The command line

```sh
nightcap status                          # what Nightcap is doing, and why
nightcap rules add app:com.figma.Desktop
nightcap rules add domain:figma.com
nightcap disable 1h30m                   # Night Shift off for a while, whatever is in front
nightcap strength 40                     # how warm Night Shift gets, 0 to 100
nightcap resume                          # back to the schedule
nightcap status --json | jq -r .decision.verdict
```

`nightcap --help` lists the rest.

## The config

Rules can also live in `~/.config/nightcap/config.toml`:

```toml
websites = true

[rules]
app = ["com.figma.Desktop", "com.adobe.Photoshop"]
domain = ["figma.com"]

[night_shift]
strength = 60
schedule = "sunset-to-sunrise"   # or "off", or custom hours such as "22:00-07:00"
```

Run `nightcap reload` after changing it. Night Shift's strength and schedule stay the system's: Nightcap applies a `[night_shift]` value when it changes, so a change you make later in System Settings stands. Rules you add from the menu or with `nightcap rules add` are kept by Nightcap itself, and `nightcap rules export --format toml` (or `nix`) prints them ready to paste into the config. [`docs/installing.md`](docs/installing.md) has the details.

## Privacy

- A copy that updates itself makes one network request a day, to check for a new release. A copy whose updates a package manager handles, such as one in the Nix store, makes none.
- With website checking on, Nightcap reads only the front tab of Safari or Chrome, and keeps only the site's name. macOS's log shows it as `<private>`.

## How it works

Night Shift has no public API, so Nightcap drives it through macOS's private CoreBrightness framework. It checks the framework's method signatures when it starts: if a macOS update changes them, Nightcap shows Night Shift as unavailable rather than guessing. It has been checked on macOS 15 and 27.

The app is a Swift shell that owns every Apple API, around a Rust core that makes every decision. The decisions behind it, and the alternatives they beat, are in [`docs/adr/`](docs/adr/).

## Building

Work inside `nix develop`, which provides the pinned Rust toolchain and puts the build scripts on `PATH`:

```sh
build-core        # the Rust core and CLI, and the app's generated bindings
build-app         # build/xcode/Debug/Nightcap Dev.app, unsigned
run-tests
```

The dev build has its own bundle ID, so it runs beside an installed copy without sharing its permissions or state. [`AGENTS.md`](AGENTS.md) covers the rest of the workflow.

## Licence

[MIT](LICENSE). The site's fonts are under the SIL Open Font License, with their licences in [`design/licenses/`](design/licenses/).
