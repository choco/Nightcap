# The site and the appcast live on nightcap.choco.me, a domain the project owns

ADR-0015 put the appcast on GitHub Pages' default address, `https://choco.github.io/Nightcap/appcast.xml`. An installed copy keeps its `SUFeedURL` until it updates, and that address rests on the GitHub account's name, the repo's name with its case, and GitHub Pages itself: a change to any of them leaves those installs without updates. The author owns `choco.me`, whose DNS is on Cloudflare and whose own site is a separate Pages repo.

## Decision

- The Nightcap repo's Pages site has the custom domain `nightcap.choco.me`: a CNAME record to `choco.github.io` on Cloudflare, DNS only, with HTTPS enforced by Pages. `SUFeedURL` is `https://nightcap.choco.me/appcast.xml`.
- `choco.me` is verified on the GitHub account, so no other account's Pages site can claim the subdomain.
- Installs that carry the old address follow GitHub's 301 from `choco.github.io/Nightcap/` to the custom domain, which keeps the path and HTTPS, and pick up the new `SUFeedURL` with their first update.
- Moving the site off GitHub means pointing the record elsewhere; the feed address stays.

## Consequences

- `choco.me` must stay registered while installs exist. If it lapses, installs stop updating; whoever registers it can serve a feed but not an update, since Sparkle checks every archive against `SUPublicEDKey`.
- GitHub's redirect exists only while the repo keeps this custom domain, so an install that never updated past a build with the old address depends on it.

## Rejected alternatives

- **`choco.github.io/Nightcap`, as ADR-0015 had it**: every install depends on the account's and the repo's names and on GitHub Pages for good.
- **`choco.me/nightcap`, through a Cloudflare Worker proxying to the Pages site**: every update check goes through Cloudflare and a Worker as well as GitHub, and the personal site moves behind Cloudflare's proxy, for an address that only looks different. Cloudflare's rules alone can't route it: overriding the origin's Host header is Enterprise-only.
- **`choco.me/Nightcap`, by making `choco.me` the account's user site**: every other project's Pages site moves under it too, and the path keeps the repo's case-sensitive name.
