# Installing Nightcap

This is the contract for anyone packaging Nightcap: a Nix module, a Homebrew cask, or a script. Nightcap needs macOS 15 or later on Apple silicon.

## What a release publishes

Every release on GitHub carries four files:

- `Nightcap-<version>.zip`: the signed and notarized app, `Nightcap.app`, with its notarization ticket stapled. Installers and Sparkle's updates use it.
- `Nightcap-<version>.dmg`: the same app in a disk image to drag into Applications, for people downloading it from the site. The disk image itself is unsigned, so macOS mounts it without asking; the app inside carries the signature and the ticket.
- `SHA256SUMS`: the zip's and the DMG's SHA-256. Check downloads against it.
- `appcast.xml`: Sparkle's feed for this release. Installed copies read it from `https://nightcap.choco.me/appcast.xml`, which always serves the newest one.

## Installing the app

1. **Install the bundle byte for byte.** Unzip it with `ditto -x -k` or anything that keeps symlinks, and change nothing inside it: no re-signing, no patching, no fixups (in Nix, `dontFixup = true`). Every permission the user grants is tied to the bundle's signature, so a modified bundle loses Accessibility and notifications.
2. **Decide who owns updates.**
   - If your installer updates Nightcap, keep the bundle's folder read-only to the user, as the Nix store is. Nightcap then sees a *managed install*: it never updates itself and never adds a login item, and `nightcap update` refuses, pointing at you.
   - If the folder is writable, as `/Applications` is, Nightcap is *self-managed*. It checks the appcast once a day, downloads updates in the background, and installs them when it quits. It opens at login if the user chooses it, in onboarding or in Settings.
3. **Start it.** A managed install starts `Contents/MacOS/Nightcap` from a LaunchAgent, pointing straight at the installed path, with `KeepAlive` so launchd restarts it. A self-managed install opens itself at login if the user chose that.
4. **Put the CLI on `PATH`.** It's `Contents/Helpers/nightcap`; link it, don't copy it. `Contents/Helpers` also holds `Tour Sample.app`, which the in-app tour opens; leave it where it is.

A downloaded zip carries the quarantine flag; leave it. The stapled ticket lets Gatekeeper accept the app offline, and the first launch asks only whether to open an app downloaded from the internet.

## Where Nightcap keeps its files

Nightcap follows the XDG base directories. launchd passes no XDG variables, so a LaunchAgent that wants anything other than the defaults sets them in its `EnvironmentVariables`:

| File | Default | Variable |
|---|---|---|
| `nightcap/config.toml`: the rules you declare | `~/.config` | `XDG_CONFIG_HOME` |
| `nightcap/rules.toml`: rules added at runtime | `~/.local/share` | `XDG_DATA_HOME` |
| `nightcap/state.json`: what the user turned on or off, and until when | `~/.local/state` | `XDG_STATE_HOME` |

Each must be an absolute path; anything else falls back to the default. The CLI talks to the app over `~/Library/Application Support/Nightcap/nightcap.sock`, which depends only on `$HOME`.

## The config

`config.toml` holds the rules and, optionally, `websites`, which settles website checking: set, it wins over the user's choice in onboarding, Settings and `nightcap websites`. An unknown key is an error, so a typo can't hide a rule:

```toml
websites = true

[rules.off]
app = ["com.figma.Desktop", "com.adobe.Photoshop"]
domain = ["figma.com"]

[rules.on]
app = ["com.apple.iBooksX"]

[night_shift]
strength = 60
schedule = "sunset-to-sunrise"

[menu]
lengths = ["30m", "1h", "2h"]
```

`[night_shift]` can set Night Shift's `strength`, 0 to 100, and its `schedule`: `"off"`, `"sunset-to-sunrise"`, or custom hours such as `"22:00-07:00"`. Each is optional. Both stay the system's: Nightcap applies a value when Nightcap starts or reloads and finds it changed since it last applied it, so a change the user makes in System Settings afterwards stands until the config changes again. A value the config sets is no longer asked about in onboarding.

A rule under `[rules.off]` keeps Night Shift off while its app is in front, or its site in a browser Nightcap watches; one under `[rules.on]` keeps it on, even by day. An app or domain can be in one list only. A domain rule covers the domain and any subdomain: `figma.com` covers `www.figma.com`, but not `notfigma.com`. Where two cover a site, the longer one decides, and a site's rule decides over its browser's. Write domains bare, with no scheme, path, port or wildcard. Domain rules apply while website checking is on, which the user chooses in onboarding or Settings unless the config sets `websites`. Website checking needs the Accessibility permission, which Nightcap asks for when it's turned on.

`[rules]` with `app` and `domain` directly under it, as earlier versions read, is an error: list those rules under `[rules.off]`. `[menu] lengths` sets the lengths the menu offers, written as `30m`, `1h` or `1h30m`, from a minute to a day; without it they're 30 minutes, 1 hour and 2 hours.

Rules added while Nightcap runs, with `nightcap rules add`, from the menu or in Settings, live in `rules.toml`. `nightcap rules export --format nix` (or `toml`), or Settings' "Copy", gives them ready to paste into the config; once the config has a rule, Nightcap drops its own copy.

Nightcap doesn't watch the file. After writing it, run:

```sh
nightcap reload --wait 5s
```

- Exit 0: loaded.
- Exit 2: the config is invalid. Nightcap keeps the rules it had and prints the parse error. Fail the installation.
- Exit 1: Nightcap didn't come up within the wait. Warn, but don't fail: it reads the config when it starts.

## Uninstalling

Quit Nightcap (`launchctl bootout` the LaunchAgent, or quit it from Activity Monitor), then remove the app and the CLI link. Remove the three XDG folders above, `~/Library/Application Support/Nightcap` and `~/Library/Caches/io.github.choco.nightcap`, where the sites' icons are kept, to forget everything.
