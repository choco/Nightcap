# Sites' icons come from the sites, asked for as the menu opens, and are kept

The menu's site row and the Rules tab show a globe for every site, while apps show their own icons. A site's icon would tell sites apart at a glance, but only the site has it within reach: the browsers keep theirs where Nightcap can't read them, and an icon service would learn every site the user opens. Nightcap otherwise reaches the network only to check for updates (ADR-0011).

## Decision

- While website checking is on, opening the menu asks for the icons of the site in front and of every site with a rule, skipping a site whose icon is kept and under a month old, or whose last ask, under a day ago, found none.
- Onboarding's sites page, which follows the choice to check websites, asks likewise for the sites it suggests. They're the same for everyone, so the asks say nothing about the user's browsing.
- Nightcap asks the site itself, over HTTPS: its home page, read for `<link rel="icon">` and `apple-touch-icon` without running anything, then the one nearest 32 pixels and at least that, an SVG next, then `/favicon.ico`. Requests are ephemeral: no cookies sent or kept, and no URL cache.
- Only sites on the internet are asked: never an address, nor a name only the local network knows, such as one ending in `.local`, for the site, an icon its page names or a redirect, so macOS never asks the user about the local network.
- Icons are kept in `~/Library/Caches/<bundle ID>/Site Icons/`, one 64-pixel PNG per site, named by its domain; a site that had none gets an empty marker.
- The menu, the Rules tab and onboarding's suggestions show a kept icon, and the globe otherwise. An icon that arrives while one of them shows appears in it.
- Nothing about icons is logged (ADR-0009).

## Consequences

- Nightcap requests pages and icons from sites the user has open, as itself rather than as their browser: a site sees a visit with no cookies, from the user's address.
- The icon folder lists the sites in front when the user opened the menu, private windows included, and the sites with rules. It's a cache: deleting it costs only asking again.
- A site that serves its icon only to browsers, or only after a login, keeps the globe.
- A kept icon can be a month old.

## Rejected alternatives

- **Asking for every site that comes to the front**: a fuller record of the user's browsing, and requests for sites they never looked at in the menu.
- **An icon service, such as Google's or DuckDuckGo's**: a third party would learn every site.
- **The browsers' own icon caches**: Safari's needs Full Disk Access; Chrome's is an undocumented database that changes with Chrome.
- **LinkPresentation's `LPMetadataProvider`**: it doesn't document what it loads or runs, and fetches more than an icon.
- **Only `/favicon.ico`**: it misses sites that declare their icon in their pages alone, like Nightcap's own.
