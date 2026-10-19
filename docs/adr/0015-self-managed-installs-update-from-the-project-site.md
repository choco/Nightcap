---
status: superseded in part by ADR-0017 and ADR-0019
---

# Self-managed installs update through Sparkle from a feed on the project's site, and open at login only by choice; managed installs leave both to their installer

Nightcap is installed two ways. A package manager such as Nix puts the bundle where the user can't write it (the Nix store), starts it from a LaunchAgent, and owns its updates. Brew or a manual drag-install puts it in `/Applications`, where nothing else updates it or starts it at login. The official homebrew/cask tap only accepts apps that pass Gatekeeper, which a self-signed, un-notarized app doesn't, so brew installs come from a personal tap whose cask strips the quarantine flag. Every update must keep the Accessibility and notification grants, which are tied to the signing requirement (ADR-0004). The project has a small website on GitHub Pages, so the feed URL every installed copy keeps can be one the project controls. Starting at every login is a choice the user should make: macOS announces a newly added login item with its own notification, which arrives with no context when nobody asked for it.

## Decision

At launch, Nightcap tells which kind of install it is:

- **Managed install**: the user can't write the bundle's location. The installer owns updates and launching, so Nightcap starts no updater and registers no login item.
- **Self-managed install**: everything else. Nightcap updates itself through Sparkle 2, and opens at login if the user chooses to.

Dev builds never start the updater or register a login item. A second instance that finds a live daemon on the socket exits, so a login item and a LaunchAgent can never run Nightcap twice.

Sparkle is set up like this:

- Updates are verified with an EdDSA key. The public key is `SUPublicEDKey` in Info.plist; the private key is a second secret in the `release` environment, used by CI's `sign_update`.
- Every release publishes `appcast.xml` as an asset, and the site's deploy workflow copies the newest one to `https://choco.github.io/Nightcap/appcast.xml`, the `SUFeedURL`. The site's path is the repo's name, and it is case-sensitive. The release asset stays the single source, so nothing commits the feed. A custom domain added later keeps old installs updating through GitHub's redirect.
- Nightcap checks daily, downloads automatically, and installs when it quits, at logout or restart. `nightcap update` starts Sparkle's own check, whose window installs at once; in a managed install it fails, pointing at the installer.
- An update keeps the signing requirement, so the grants survive.
- Hardened runtime stays on, with `com.apple.security.cs.disable-library-validation`: library validation rejects any framework signed by a certificate without a Team ID, which a self-signed certificate never has. Hardened runtime still blocks `DYLD_*` injection. Sparkle's helpers are re-signed inside-out with Nightcap's certificate, and the release gate allows `@rpath/Sparkle.framework` as the only non-system library.

Opening at login works like this:

- Onboarding asks whether Nightcap should open at login, and registers the login item with `SMAppService.mainApp` only on a yes. Nothing else registers it.
- Nightcap records the answer. Settings shows an "Open at login" switch that mirrors the login item's state as macOS reports it; flipping it registers or unregisters the item and records the new answer.
- When the user chose to open at login and macOS stops opening Nightcap, because the item was removed in System Settings or is waiting for approval, the "Open at login" check is a problem. The user either restores it (Login Items, or registering again) or keeps it off, which records that as their answer.

## Consequences

- A self-managed install makes one network request a day, to the appcast. A managed install makes none (ADR-0012).
- If the site's deploy fails after a release, installs don't see that release until it's re-run.
- A `SUFeedURL` that doesn't match the site's path exactly gets a 404, and the installs that carry it never update again; each needs a manual reinstall.
- A process already running as the user could swap the framework inside a writable bundle, which library validation would otherwise have stopped.
- The brew cask lives in a separate tap repo and declares `auto_updates true`, so `brew upgrade` leaves updates to Sparkle.
- A user who skips onboarding's question doesn't get Nightcap at login until they turn the switch on.

## Rejected alternatives

- **The release asset as the feed, `releases/latest/download/appcast.xml`**: every installed copy would depend on GitHub's release redirect for good, while the site makes a URL of the project's own free.
- **An appcast committed to the repo or to a Pages branch**: bot commits, for what copying the release asset does.
- **A headless `nightcap update` through a custom Sparkle user driver**: about fifteen callbacks and an IPC reply that waits on the network, for what Sparkle's own window already does.
- **No auto-update, relying on `brew upgrade` and Nix**: every self-managed install would need a manual step for each release.
- **Sparkle in every install**: in the Nix store it asks for an admin password and overwrites the store path.
- **An environment variable from the installer's LaunchAgent to mark managed installs**: a Nix-installed app opened from Finder wouldn't see it, and Sparkle would go after the store.
- **Installing each update immediately and relaunching**: every update would release the forces and flash Night Shift on mid-session (ADR-0011).
- **Hardened runtime off**: loses the `DYLD_*` protection for a process that holds an Accessibility grant.
- **A hand-written updater**: downloading, verifying and replacing a running bundle is what Sparkle exists for.
- **Registering the login item on the first launch**: the user never chose it, and macOS's notification about the new login item arrives without explanation.
- **macOS's state as the only record of the choice**: a removal in System Settings, or by a cleanup tool, would silently stop Nightcap opening at login, and nobody would be told.
