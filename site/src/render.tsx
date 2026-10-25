import { prerender } from "react-dom/static";
import { App } from "./App";

/** The page's markup, for index.html's outlet: prerender.ts writes it at build time, and the dev
 *  server on every request. */
export async function render(): Promise<string> {
	const { prelude } = await prerender(<App />);
	return new Response(prelude).text();
}
