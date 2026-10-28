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
lengths = ["30m", "1h", "2h"]

[rules.off]
app = ["com.figma.Desktop", "com.adobe.Photoshop"]
domain = ["figma.com"]

[rules.on]
app = ["com.apple.iBooksX"]

[night_shift]
strength = 60
schedule = "sunset-to-sunrise"
```

`[night_shift]` can set Night Shift's `strength`, 0 to 100, and its `schedule`: `"off"`, `"sunset-to-sunrise"`, or custom hours such as `"22:00-07:00"`. Each is optional. Both stay the system's: Nightcap applies a value when Nightcap starts or reloads and finds it changed since it last applied it, so a change the user makes in System Settings afterwards stands until the config changes again. A value the config sets is no longer asked about in onboarding.

A rule under `[rules.off]` keeps Night Shift off while its app is in front, or its site in a browser Nightcap watches; one under `[rules.on]` keeps it on, even by day. An app or domain can be in one list only. A domain rule covers the domain and any subdomain: `figma.com` covers `www.figma.com`, but not `notfigma.com`. Where two cover a site, the longer one decides, and a site's rule decides over its browser's. Write domains bare, with no scheme, path, port or wildcard. Domain rules apply while website checking is on, which the user chooses in onboarding or Settings unless the config sets `websites`. Website checking needs the Accessibility permission, which Nightcap asks for when it's turned on.

`[rules]` with `app` and `domain` directly under it, as earlier versions read, is an error: list those rules under `[rules.off]`. `lengths`, before any table, sets the lengths a change can be picked for in the menu, written as `30m`, `1h` or `1h30m`, from a minute to a day; without it they're 30 minutes, 1 hour and 2 hours. `lengths` under `[menu]`, as earlier versions read it, is an error.

Rules added while Nightcap runs, with `nightcap rules add`, from the menu or in Settings, live in `rules.toml`. `nightcap rules export --format nix` (or `toml`), or Settings' "Copy", gives them ready to paste into the config; once the config has a rule, Nightcap drops its own copy.

Nightcap doesn't watch the file. After writing it, run:

```sh
nightcap reload --wait 5s
```

- Exit 0: loaded.
- Exit 2: the config is invalid. Nightcap keeps the rules it had and prints the parse error. Fail the installation.
- Exit 1: Nightcap didn't come up within the wait. Warn, but don't fail: it reads the config when it starts.

## Scripting against the CLI

`nightcap status --json` and `nightcap rules list --json` print one JSON object, on one line, for scripts such as `nightcap status --json | jq -r .front.decision.state`. This JSON is the CLI's public interface; what the CLI and the app say to each other over the socket isn't, and changes in any release. The JSON changes only on purpose: `schema` says which version of it a document is, and goes up with every change, a field added among them. `message` and `config.error` are sentences for people, whose words can change in any release: match on `check` and `state` instead.

`nightcap status --json`, with a config rule keeping Figma off and Night Shift turned on in Figma until the schedule turns it off at 07:00:

```json
{
  "schema": 1,
  "nightcap": { "version": "1.2.0", "install": "self_managed" },
  "night_shift": {
    "state": "on",
    "strength": 60,
    "schedule": "22:00-07:00",
    "scheduled": "on",
    "next_on": "2026-10-17T20:00:00Z",
    "next_off": "2026-10-17T05:00:00Z"
  },
  "front": {
    "app": "com.figma.Desktop",
    "site": null,
    "decision": {
      "state": "on",
      "place": "app:com.figma.Desktop",
      "reason": {
        "kind": "rule",
        "place": "app:com.figma.Desktop",
        "says": "on",
        "set_by": "you",
        "length": "until_schedule",
        "until": "2026-10-17T05:00:00Z"
      }
    }
  },
  "tour": null,
  "rules": [
    {
      "place": "app:com.figma.Desktop",
      "says": "off",
      "set_by": "config",
      "length": "indefinite",
      "until": null,
      "snooze": {
        "place": "app:com.figma.Desktop",
        "says": "on",
        "set_by": "you",
        "length": "until_schedule",
        "until": "2026-10-17T05:00:00Z"
      }
    }
  ],
  "changes": [
    {
      "place": "app:com.figma.Desktop",
      "says": "on",
      "set_by": "you",
      "length": "until_schedule",
      "until": "2026-10-17T05:00:00Z"
    }
  ],
  "website_checking": { "state": "off", "from_config": false },
  "config": { "path": "/Users/me/.config/nightcap/config.toml", "error": null },
  "needs_attention": []
}
```

Times are RFC 3339, in UTC. A place is written as the CLI's arguments take one: `everywhere`, `app:<bundle ID>` or `domain:<domain>`.

| Field | What it says |
|---|---|
| `schema` | The version of this JSON: 1. |
| `nightcap.version` | The running Nightcap's version. |
| `nightcap.install` | `self_managed`: it updates itself, and opens at login if the user chose it. `managed`: its installer updates and starts it. `dev_build`: a dev build, which does neither. |
| `night_shift.state` | Night Shift as macOS last reported it: `on`, `off`, or `unavailable` on a Mac without it. |
| `night_shift.strength` | How warm Night Shift tints the display, 0 to 100; null until Nightcap has read it. |
| `night_shift.schedule` | As the config writes it: `off`, `sunset-to-sunrise`, or custom hours such as `22:00-07:00`; null until macOS has reported it. |
| `night_shift.scheduled` | What the schedule says now, `on` or `off`; null while the schedule, or the sunrise and sunset it follows, aren't known. |
| `night_shift.next_on`, `night_shift.next_off` | When the schedule next turns Night Shift on, and off; null while no such turn is coming, or its sunrise and sunset aren't known. |
| `front.app` | The frontmost app's bundle ID; null when it has none. |
| `front.site` | The site in front while website checking reads the front tab: the domain of the most specific rule that covers it, else the tab's own domain short of `www.`; else null. |
| `front.decision.state` | What Night Shift should be now, `on` or `off`, which the app makes it within a moment; null while nothing decides it yet. |
| `front.decision.place` | Where it's decided: the site in front, the front app, or `everywhere`. |
| `front.decision.reason` | What the decision rests on first. `"kind": "rule"`: that place's shortest rule, with its fields beside `kind`. `"kind": "schedule"`: the schedule, `until` its next turn. `"kind": "tour"`: the tour, holding Night Shift on. Null while nothing decides. |
| `tour` | Null unless the app's tour runs. Then `stands_in` says whether it holds Night Shift on in place of the schedule. Rules and changes made during the tour last until it ends. |
| `rules` | The apps' and sites' rules, as `nightcap rules list` lists them: each a rule, with its `snooze`, the change over it that says the other way until it ends, or null. |
| `changes` | Every other rule: what the menu, `on`, `off` and `toggle` set at a place, and the changes in Control Center or System Settings that Nightcap adopted, shortest first at each place. `nightcap reset` takes them off. |
| `website_checking.state` | Whether Nightcap reads the front browser's tab, `on` or `off`. |
| `website_checking.from_config` | Whether the config's `websites` sets it. |
| `config.path` | Where Nightcap reads the config, whether or not it's there. |
| `config.error` | Why the config can't be used, starting from its path; null while it can. |
| `needs_attention` | Each check that needs the user: `check`, one of `night_shift`, `accessibility`, `notifications`, `open_at_login`, `config_file` and `ad_hoc_rules`; `state`, `drifted` where something the user set up stopped working, which restoring it or accepting the change settles, or `broken` where it can only be fixed; and `message`, the CLI's sentence for it. |

A rule has:

| Field | What it says |
|---|---|
| `place` | Where it applies. |
| `says` | `on` or `off`. |
| `set_by` | `config`; `you`, from the menu, Settings or the CLI; `tour`, the user during the tour; or `outside_change`, a change in Control Center or System Settings that Nightcap adopted. |
| `length` | `indefinite`; `until_schedule`, until the schedule next turns Night Shift the other way; or `until_time`. |
| `until` | When it ends; null for an indefinite rule, or one until the schedule turns while no turn is coming. |

`nightcap rules list --json` prints `schema` and `rules` alone.

## Uninstalling

Quit Nightcap (`launchctl bootout` the LaunchAgent, or quit it from Activity Monitor), then remove the app and the CLI link. Remove the three XDG folders above, `~/Library/Application Support/Nightcap` and `~/Library/Caches/io.github.choco.nightcap`, where the sites' icons are kept, to forget everything.
