# Self-managed installs update through Sparkle; managed installs leave updates to their installer

Nightcap is installed two ways. A package manager such as Nix puts the bundle where the user can't write it (the Nix store), starts it from a LaunchAgent, and owns its updates. Brew or a manual drag-install puts it in `/Applications`, where nothing else updates it or starts it at login. The official homebrew/cask tap only accepts apps that pass Gatekeeper, which a self-signed, un-notarized app doesn't, so brew installs come from a personal tap whose cask strips the quarantine flag. Every update must keep the Accessibility and notification grants, which are tied to the signing requirement (ADR-0004).

## Decision

At launch, Nightcap tells which kind of install it is:

- **Managed install**: the user can't write the bundle's location. The installer owns updates and launching, so Nightcap starts no updater and registers no login item.
- **Self-managed install**: everything else. Nightcap updates itself through Sparkle 2, and registers itself to start at login with `SMAppService.mainApp`.

Dev builds never start the updater or register a login item. A second instance that finds a live daemon on the socket exits, so a login item and a LaunchAgent can never run Nightcap twice.

Sparkle is set up like this:

- Updates are verified with an EdDSA key. The public key is `SUPublicEDKey` in Info.plist; the private key is a second secret in the `release` environment, used by CI's `sign_update`.
- The feed is an `appcast.xml` published as an asset of every release, read from `https://github.com/choco/nightcap/releases/latest/download/appcast.xml`, which always redirects to the newest release.
- Nightcap checks daily, downloads automatically, and installs when it quits, at logout or restart. `nightcap update` installs at once; in a managed install it fails, pointing at the installer.
- An update keeps the signing requirement, so the grants survive.
- Hardened runtime stays on, with `com.apple.security.cs.disable-library-validation`: library validation rejects any framework signed by a certificate without a Team ID, which a self-signed certificate never has. Hardened runtime still blocks `DYLD_*` injection. Sparkle's helpers are re-signed inside-out with Nightcap's certificate, and the release gate allows `@rpath/Sparkle.framework` as the only non-system library.

## Consequences

- A self-managed install makes one network request a day, to the appcast. A managed install makes none (ADR-0012).
- A process already running as the user could swap the framework inside a writable bundle, which library validation would otherwise have stopped.
- The brew cask lives in a separate tap repo and declares `auto_updates true`, so `brew upgrade` leaves updates to Sparkle.

## Rejected alternatives

- **No auto-update, relying on `brew upgrade` and Nix**: every self-managed install would need a manual step for each release.
- **Sparkle in every install**: in the Nix store it asks for an admin password and overwrites the store path.
- **An environment variable from the installer's LaunchAgent to mark managed installs**: a Nix-installed app opened from Finder wouldn't see it, and Sparkle would go after the store.
- **Installing each update immediately and relaunching**: every update would release the forces and flash Night Shift on mid-session (ADR-0011).
- **An appcast committed to the repo or served from GitHub Pages**: bot commits or a second publishing target, for what a release asset already does.
- **Hardened runtime off**: loses the `DYLD_*` protection for a process that holds an Accessibility grant.
- **A hand-written updater**: downloading, verifying and replacing a running bundle is what Sparkle exists for.
