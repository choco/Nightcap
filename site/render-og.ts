// Renders public/og.png, the social card, from the built site, as scripts/render-og-image runs it:
// the top of the page at 1200 × 630 in Nightcap's home theme, night. Reduced motion holds the page
// still and parks the lens over the hero's scene, and the clocks read the evening the prerendered
// page shows, so a render changes only when the page does.
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { preview } from "vite";

const site = fileURLToPath(new URL(".", import.meta.url));
const output = fileURLToPath(new URL("public/og.png", import.meta.url));

const server = await preview({ root: site, logLevel: "warn" });
const url = server.resolvedUrls?.local[0];
if (!url) throw new Error("vite preview has no local address");
const browser = await chromium.launch({ channel: "chrome" });
try {
	const page = await browser.newPage({
		viewport: { width: 1200, height: 630 },
		colorScheme: "dark",
		reducedMotion: "reduce",
		locale: "en-GB",
		timezoneId: "UTC",
	});
	// A Wednesday at 23:41, as Clock shows before it hydrates.
	await page.clock.setFixedTime(new Date("2026-10-21T23:41:00Z"));
	await page.goto(url, { waitUntil: "networkidle" });
	await page.evaluate("document.fonts.ready.then(() => {})");
	// The doodles are placed again once the fonts reflow the text, after a 200 ms debounce.
	await page.waitForTimeout(1000);
	await page.screenshot({ path: output });
} finally {
	await browser.close();
	await server.close();
}
