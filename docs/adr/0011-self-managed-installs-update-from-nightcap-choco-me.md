# Self-managed installs update through Sparkle from a feed on nightcap.choco.me, and open at login only by choice; managed installs leave both to their installer

Nightcap is installed two ways. A package manager such as Nix puts the bundle where the user can't write it (the Nix store), starts it from a LaunchAgent, and owns its updates. Brew or a manual drag-install puts it in `/Applications`, where nothing else updates it or starts it at login. Every update must keep the Accessibility and notification grants, which are tied to the signing requirement (ADR-0003). An installed copy keeps its feed URL until it updates, so that address has to outlast the GitHub account's and the repo's names and GitHub Pages itself. The author owns `choco.me`, whose DNS is on Cloudflare and whose own site is a separate Pages repo. Starting at every login is a choice the user should make: macOS announces a newly added login item with its own notification, which arrives with no context when nobody asked for it.

## Decision

At launch, Nightcap tells which kind of install it is:

- **Managed install**: the user can't write the bundle's location. The installer owns updates and launching, so Nightcap starts no updater and registers no login item.
- **Self-managed install**: everything else. Nightcap updates itself through Sparkle 2, and opens at login if the user chooses to.

Dev builds never start the updater or register a login item. A second instance that finds a live server on the socket exits, so a login item and a LaunchAgent can never run Nightcap twice.

The project's site is the repo's GitHub Pages site, at the custom domain `nightcap.choco.me`: a CNAME record to `choco.github.io` on Cloudflare, DNS only, with HTTPS enforced by Pages. `choco.me` is verified on the GitHub account, so no other account's Pages site can claim the subdomain. Moving the site off GitHub means pointing the record elsewhere; the feed address stays.

Sparkle is set up like this:

- Updates are verified with an EdDSA key. The public key is `SUPublicEDKey` in Info.plist; the private key is a secret in the `release` environment, used by CI's `sign_update`.
- Every release publishes `appcast.xml` as an asset, and the site's deploy workflow copies the newest one to `https://nightcap.choco.me/appcast.xml`, the `SUFeedURL`. The release asset stays the single source, so nothing commits the feed.
- Nightcap checks daily, downloads automatically, and installs when it quits, at logout or restart. `nightcap update` starts Sparkle's own check, whose window installs at once; in a managed install it fails, pointing at the installer.
- An update keeps the signing requirement, so the grants survive.
- Hardened runtime stays on, with library validation (ADR-0003). Sparkle's helpers are signed inside-out with Nightcap's certificate, and the release gate allows `@rpath/Sparkle.framework` as the only non-system library.

Opening at login works like this:

- Onboarding asks whether Nightcap should open at login, and registers the login item with `SMAppService.mainApp` only on a yes. Nothing else registers it.
- Nightcap records the answer. Settings shows an "Open at login" switch that mirrors the login item's state as macOS reports it; flipping it registers or unregisters the item and records the new answer.
- When the user chose to open at login and macOS stops opening Nightcap, because the item was removed in System Settings or is waiting for approval, the "Open at login" check is a problem. The user either restores it (Login Items, or registering again) or keeps it off, which records that as their answer.

## Consequences

- A self-managed install checks the feed once a day. A managed install never does (ADR-0009).
- If the site's deploy fails after a release, installs don't see that release until it's re-run.
- A `SUFeedURL` that doesn't match the site's path exactly gets a 404, and the installs that carry it never update again; each needs a manual reinstall.
- `choco.me` must stay registered while installs exist. If it lapses, installs stop updating; whoever registers it can serve a feed but not an update, since Sparkle checks every archive against `SUPublicEDKey`.
- Installs from before the custom domain carry `https://choco.github.io/Nightcap/appcast.xml`, and follow GitHub's 301 to `nightcap.choco.me`, which keeps the path and HTTPS, until their first update gives them the new address. That redirect exists only while the repo keeps this custom domain.
- The brew cask lives in a separate tap repo and declares `auto_updates true`, so `brew upgrade` leaves updates to Sparkle.
- A user who skips onboarding's question doesn't get Nightcap at login until they turn the switch on.

## Rejected alternatives

- **The release asset as the feed, `releases/latest/download/appcast.xml`**: every installed copy would depend on GitHub's release redirect for good, while the site makes a URL of the project's own free.
- **`choco.github.io/Nightcap` as the feed's home**: every install would depend on the account's and the repo's names, the latter case-sensitive, and on GitHub Pages for good.
- **`choco.me/nightcap`, through a Cloudflare Worker proxying to the Pages site**: every update check goes through Cloudflare and a Worker as well as GitHub, and the personal site moves behind Cloudflare's proxy, for an address that only looks different. Cloudflare's rules alone can't route it: overriding the origin's Host header is Enterprise-only.
- **`choco.me/Nightcap`, by making `choco.me` the account's user site**: every other project's Pages site moves under it too, and the path keeps the repo's case-sensitive name.
- **An appcast committed to the repo or to a Pages branch**: bot commits, for what copying the release asset does.
- **A headless `nightcap update` through a custom Sparkle user driver**: about fifteen callbacks and an IPC reply that waits on the network, for what Sparkle's own window already does.
- **No auto-update, relying on `brew upgrade` and Nix**: every self-managed install would need a manual step for each release.
- **Sparkle in every install**: in the Nix store it asks for an admin password and overwrites the store path.
- **An environment variable from the installer's LaunchAgent to mark managed installs**: a Nix-installed app opened from Finder wouldn't see it, and Sparkle would go after the store.
- **Installing each update immediately and relaunching**: every update would release the forces and flash Night Shift's other state mid-session (ADR-0008).
- **Hardened runtime off**: loses the `DYLD_*` protection for a process that holds an Accessibility grant.
- **A hand-written updater**: downloading, verifying and replacing a running bundle is what Sparkle exists for.
- **Registering the login item on the first launch**: the user never chose it, and macOS's notification about the new login item arrives without explanation.
- **macOS's state as the only record of the choice**: a removal in System Settings, or by a cleanup tool, would silently stop Nightcap opening at login, and nobody would be told.
