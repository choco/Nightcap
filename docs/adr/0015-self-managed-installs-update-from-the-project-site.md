# Self-managed installs update through Sparkle from a feed on the project's site; managed installs leave updates to their installer

Nightcap is installed two ways. A package manager such as Nix puts the bundle where the user can't write it (the Nix store), starts it from a LaunchAgent, and owns its updates. Brew or a manual drag-install puts it in `/Applications`, where nothing else updates it or starts it at login. The official homebrew/cask tap only accepts apps that pass Gatekeeper, which a self-signed, un-notarized app doesn't, so brew installs come from a personal tap whose cask strips the quarantine flag. Every update must keep the Accessibility and notification grants, which are tied to the signing requirement (ADR-0004).

This replaces ADR-0014, which served the feed as a release asset. The project now gets a small website on GitHub Pages, so Pages is a publishing target anyway, and the feed URL every installed copy keeps can be one the project controls.

## Decision

At launch, Nightcap tells which kind of install it is:

- **Managed install**: the user can't write the bundle's location. The installer owns updates and launching, so Nightcap starts no updater and registers no login item.
- **Self-managed install**: everything else. Nightcap updates itself through Sparkle 2, and registers itself to start at login with `SMAppService.mainApp`.

Dev builds never start the updater or register a login item. A second instance that finds a live daemon on the socket exits, so a login item and a LaunchAgent can never run Nightcap twice.

Sparkle is set up like this:

- Updates are verified with an EdDSA key. The public key is `SUPublicEDKey` in Info.plist; the private key is a second secret in the `release` environment, used by CI's `sign_update`.
- Every release publishes `appcast.xml` as an asset, and the site's deploy workflow copies the newest one to `https://choco.github.io/nightcap/appcast.xml`, the `SUFeedURL`. The release asset stays the single source, so nothing commits the feed. A custom domain added later keeps old installs updating through GitHub's redirect.
- Nightcap checks daily, downloads automatically, and installs when it quits, at logout or restart. `nightcap update` starts Sparkle's own check, whose window installs at once; in a managed install it fails, pointing at the installer.
- An update keeps the signing requirement, so the grants survive.
- Hardened runtime stays on, with `com.apple.security.cs.disable-library-validation`: library validation rejects any framework signed by a certificate without a Team ID, which a self-signed certificate never has. Hardened runtime still blocks `DYLD_*` injection. Sparkle's helpers are re-signed inside-out with Nightcap's certificate, and the release gate allows `@rpath/Sparkle.framework` as the only non-system library.

## Consequences

- A self-managed install makes one network request a day, to the appcast. A managed install makes none (ADR-0012).
- If the site's deploy fails after a release, installs don't see that release until it's re-run.
- A process already running as the user could swap the framework inside a writable bundle, which library validation would otherwise have stopped.
- The brew cask lives in a separate tap repo and declares `auto_updates true`, so `brew upgrade` leaves updates to Sparkle.

## Rejected alternatives

- **The release asset as the feed, `releases/latest/download/appcast.xml`** (ADR-0014): every installed copy would depend on GitHub's release redirect for good, while the site makes a URL of the project's own free.
- **An appcast committed to the repo or to a Pages branch**: bot commits, for what copying the release asset does.
- **A headless `nightcap update` through a custom Sparkle user driver**: about fifteen callbacks and an IPC reply that waits on the network, for what Sparkle's own window already does.
- **No auto-update, relying on `brew upgrade` and Nix**: every self-managed install would need a manual step for each release.
- **Sparkle in every install**: in the Nix store it asks for an admin password and overwrites the store path.
- **An environment variable from the installer's LaunchAgent to mark managed installs**: a Nix-installed app opened from Finder wouldn't see it, and Sparkle would go after the store.
- **Installing each update immediately and relaunching**: every update would release the forces and flash Night Shift on mid-session (ADR-0011).
- **Hardened runtime off**: loses the `DYLD_*` protection for a process that holds an Accessibility grant.
- **A hand-written updater**: downloading, verifying and replacing a running bundle is what Sparkle exists for.
