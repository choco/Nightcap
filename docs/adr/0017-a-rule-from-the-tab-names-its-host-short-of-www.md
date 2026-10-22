# A rule made from the front tab names its host, short of `www.`

A rule made from the front tab's row is a domain rule, covering the domain it names and every subdomain (ADR-0006). Naming the tab's registrable domain, the label below the public suffix, is right for `www.figma.com`, and too broad wherever one domain holds separate services: a rule made on `docs.google.com` would cover all of `google.com`, and one made on `tv.apple.com` all of `apple.com`, with nothing on screen to say so, and the tour's site step, on `nightcap.choco.me`, all of `choco.me`.

## Decision

- A rule made from the front tab names the tab's host, short of a leading `www.`: `figma.com` from `www.figma.com`, `docs.google.com` from itself. It still covers that host's subdomains.
- The `www.` stays where removing it would leave only a top-level domain, as on `www.com`.
- No Public Suffix List is needed, so Nightcap carries none.

## Consequences

- A site spread over sibling subdomains, such as `music.youtube.com` beside `www.youtube.com`, needs a rule for each made from the menu, or one broader rule written in the config. A rule that's too narrow shows itself: the warmth comes back on the part it misses.

## Rejected alternatives

- **The registrable domain**: right for single-identity sites, and silently far too broad on shared ones.
- **Both in a submenu**: a choice at every rule for a case the config already covers.
- **Dropping other common prefixes, like `m.`**: they name a different version of a site as often as the same site.
