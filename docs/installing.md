# Installing Nightcap

This is the contract for anyone packaging Nightcap: a Nix module, a Homebrew cask, or a script. Nightcap needs macOS 27 on Apple silicon.

## What a release publishes

Every release on GitHub carries three files:

- `Nightcap-<version>.zip`: the signed app, `Nightcap.app`.
- `SHA256SUMS`: the zip's SHA-256. Check downloads against it.
- `appcast.xml`: Sparkle's feed for this release. Installed copies read it from `https://choco.github.io/nightcap/appcast.xml`, which always serves the newest one.

## Installing the app

1. **Install the bundle byte for byte.** Unzip it with `ditto -x -k` or anything that keeps symlinks, and change nothing inside it: no re-signing, no patching, no fixups (in Nix, `dontFixup = true`). Every permission the user grants is tied to the bundle's signature, so a modified bundle loses Accessibility and notifications.
2. **Decide who owns updates.**
   - If your installer updates Nightcap, keep the bundle's folder read-only to the user, as the Nix store is. Nightcap then sees a *managed install*: it never updates itself and never adds a login item, and `nightcap update` refuses, pointing at you.
   - If the folder is writable, as `/Applications` is, Nightcap is *self-managed*. It checks the appcast once a day, downloads updates in the background, installs them when it quits, and adds itself as a login item.
3. **Start it.** A managed install starts `Contents/MacOS/Nightcap` from a LaunchAgent, pointing straight at the installed path, with `KeepAlive` so launchd restarts it. A self-managed install starts itself at login.
4. **Put the CLI on `PATH`.** It's `Contents/Helpers/nightcap`; link it, don't copy it.

A downloaded zip carries the quarantine flag. Nightcap isn't notarized, so a cask removes the flag after installing: `xattr -dr com.apple.quarantine Nightcap.app`.

## Where Nightcap keeps its files

Nightcap follows the XDG base directories. launchd passes no XDG variables, so a LaunchAgent that wants anything other than the defaults sets them in its `EnvironmentVariables`:

| File | Default | Variable |
|---|---|---|
| `nightcap/config.toml`: the rules you declare | `~/.config` | `XDG_CONFIG_HOME` |
| `nightcap/rules.toml`: rules added at runtime | `~/.local/share` | `XDG_DATA_HOME` |
| `nightcap/state.json`: snoozes, the manual state | `~/.local/state` | `XDG_STATE_HOME` |

Each must be an absolute path; anything else falls back to the default. The CLI talks to the app over `~/Library/Application Support/Nightcap/nightcap.sock`, which depends only on `$HOME`.

## The config

`config.toml` holds only rules. An unknown key is an error, so a typo can't hide a rule:

```toml
[rules]
app = ["com.figma.Desktop", "com.adobe.Photoshop"]
```

Nightcap doesn't watch the file. After writing it, run:

```sh
nightcap reload --wait 5s
```

- Exit 0: loaded.
- Exit 2: the config is invalid. Nightcap keeps the rules it had and prints the parse error. Fail the installation.
- Exit 1: Nightcap didn't come up within the wait. Warn, but don't fail: it reads the config when it starts.

## Uninstalling

Quit Nightcap (`launchctl bootout` the LaunchAgent, or quit it from Activity Monitor), then remove the app and the CLI link. Remove the three XDG folders above and `~/Library/Application Support/Nightcap` to forget everything.
