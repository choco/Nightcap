import type { ComponentType } from "react";
import { prerender } from "react-dom/static";
import { App } from "./App";
import { Changelog } from "./Changelog";
import { NotFound } from "./NotFound";
import { Privacy } from "./Privacy";
import { isPage, type PageFile } from "./pages";
import { Terms } from "./Terms";

const COMPONENTS = {
	"index.html": App,
	"404.html": NotFound,
	"privacy/index.html": Privacy,
	"terms/index.html": Terms,
	"changelog/index.html": Changelog,
} satisfies Record<PageFile, ComponentType>;

/** A page's markup, for its HTML's outlet: prerender.ts writes it at build time, and the dev
 *  server on every request. */
export async function render(file: string): Promise<string> {
	if (!isPage(file)) throw new Error(`${file} isn't one of the site's pages`);
	const Page = COMPONENTS[file];
	const { prelude } = await prerender(<Page />);
	return new Response(prelude).text();
}
