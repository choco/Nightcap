import type { ComponentType } from "react";
import { prerender } from "react-dom/static";
import { App } from "./App";
import { NotFound } from "./NotFound";
import { Privacy } from "./Privacy";
import { Terms } from "./Terms";

/** The site's pages, by the HTML file each is prerendered into, as the site serves it. */
const PAGES: Record<string, ComponentType> = {
	"index.html": App,
	"404.html": NotFound,
	"privacy/index.html": Privacy,
	"terms/index.html": Terms,
};

export const pageFiles = Object.keys(PAGES);

/** A page's markup, for its HTML's outlet: prerender.ts writes it at build time, and the dev
 *  server on every request. */
export async function render(file: string): Promise<string> {
	const Page = PAGES[file];
	if (!Page) throw new Error(`${file} isn't one of the site's pages`);
	const { prelude } = await prerender(<Page />);
	return new Response(prelude).text();
}
