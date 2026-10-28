/** A page of the site: `entry` names the module in src/entries/ that hydrates it, and the
 *  build's input, after which its chunks are named. A page that isn't `listed` is kept out of
 *  search results. The strings go into the HTML as they are. */
interface Page {
	entry: string;
	title: string;
	listed?: { path: string; description: string; cardTitle: string };
}

/** The site's pages, by the HTML file each is built into, as the site serves it. Every file is
 *  the same template, which Vite needs one of for each path it serves. */
export const PAGES = {
	"index.html": {
		entry: "home",
		title: "Nightcap",
		listed: {
			path: "/",
			description:
				"Nightcap brings Night Shift to your menu bar: set how warm it gets, switch it or peek past it with a shortcut, and keep it off for the apps and websites you choose.",
			cardTitle: "Nightcap: Night Shift, made yours",
		},
	},
	"404.html": { entry: "not-found", title: "Not found · Nightcap" },
	"privacy/index.html": {
		entry: "privacy",
		title: "Privacy policy · Nightcap",
		listed: {
			path: "/privacy/",
			description:
				"Nightcap collects nothing about you: what it keeps stays on your Mac. What it asks the internet, and why.",
			cardTitle: "Nightcap's privacy policy",
		},
	},
	"terms/index.html": {
		entry: "terms",
		title: "Terms of service · Nightcap",
		listed: {
			path: "/terms/",
			description:
				"The terms for using Nightcap, a free and open-source Mac app, and its site: free to use, provided as it is.",
			cardTitle: "Nightcap's terms of service",
		},
	},
	"changelog/index.html": {
		entry: "changelog",
		title: "Changelog · Nightcap",
		listed: {
			path: "/changelog/",
			description:
				"What changed in each version of Nightcap, the Mac app that switches Night Shift off, or on, for the apps and websites you choose.",
			cardTitle: "Nightcap's changelog",
		},
	},
} satisfies Record<string, Page>;

export type PageFile = keyof typeof PAGES;

export function isPage(file: string): file is PageFile {
	return Object.hasOwn(PAGES, file);
}

const ORIGIN = "https://nightcap.choco.me";

/** A page's head, for its HTML's `<!--head-->` outlet. */
export function head(file: string): string {
	if (!isPage(file)) throw new Error(`${file} isn't one of the site's pages`);
	const { entry, title, listed }: Page = PAGES[file];
	return [
		'<meta charset="utf-8">',
		'<meta name="viewport" content="width=device-width, initial-scale=1">',
		`<title>${title}</title>`,
		...(listed
			? [
					`<meta name="description" content="${listed.description}">`,
					`<link rel="canonical" href="${ORIGIN}${listed.path}">`,
				]
			: ['<meta name="robots" content="noindex">']),
		'<meta name="color-scheme" content="light dark">',
		...(listed
			? [
					'<meta property="og:type" content="website">',
					`<meta property="og:url" content="${ORIGIN}${listed.path}">`,
					'<meta property="og:site_name" content="Nightcap">',
					`<meta property="og:title" content="${listed.cardTitle}">`,
					`<meta property="og:description" content="${listed.description}">`,
					`<meta property="og:image" content="${ORIGIN}/og.png">`,
					'<meta property="og:image:width" content="1200">',
					'<meta property="og:image:height" content="630">',
					`<meta property="og:image:alt" content="Night Shift, made yours: Nightcap's menu open in the Mac menu bar, keeping Night Shift off while Figma is in front">`,
					'<meta name="twitter:card" content="summary_large_image">',
				]
			: []),
		'<meta name="theme-color" content="#dde5f6" media="(prefers-color-scheme: light)">',
		'<meta name="theme-color" content="#16224b" media="(prefers-color-scheme: dark)">',
		'<link rel="icon" href="/icon.svg" type="image/svg+xml">',
		'<link rel="apple-touch-icon" href="/apple-touch-icon.png">',
		`<script>
  // The footer's appearance switch, applied before the first paint.
  try {
    const theme = localStorage.getItem('nightcap-theme');
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
  } catch {}
</script>`,
		`<script type="module" src="/src/entries/${entry}.tsx"></script>`,
	].join("\n");
}
