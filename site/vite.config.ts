import { basename, relative } from "node:path";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import {
	defineConfig,
	type HtmlTagDescriptor,
	isCSSRequest,
	type Plugin,
	type ViteDevServer,
} from "vite";
import { changelog } from "./plugins/changelog.ts";
import { design } from "./plugins/design.ts";
import { entryScript, head, PAGES } from "./src/pages.ts";

const site = fileURLToPath(new URL(".", import.meta.url));

// Each page's head goes into its HTML before Vite reads the HTML for its scripts, the entry's
// among them.
function pageHeads(): Plugin {
	return {
		name: "nightcap:page-heads",
		transformIndexHtml: {
			order: "pre",
			handler(html, { filename }) {
				if (!html.includes("<!--head-->"))
					throw new Error(`${filename} has no <!--head--> outlet`);
				return html.replace("<!--head-->", () =>
					head(relative(site, filename)),
				);
			},
		},
	};
}

// The stylesheets an entry loads, as the dev client's own <style> tags, in the order it would add
// them. The client takes the tags over and keeps them up to date. A stylesheet's imports aren't
// followed: they're the files Tailwind scans for classes.
async function devStyles(
	server: ViteDevServer,
	entry: string,
): Promise<HtmlTagDescriptor[]> {
	const client = server.environments.client;
	const seen = new Set<string>();
	const styles: HtmlTagDescriptor[] = [];
	const walk = async (url: string) => {
		if (seen.has(url)) return;
		seen.add(url);
		await client.transformRequest(url);
		const module = await client.moduleGraph.getModuleByUrl(url);
		if (!module?.id) throw new Error(`${url} isn't in the module graph`);
		if (isCSSRequest(url)) {
			// The text comes from the server's module graph: another copy of a CSS module in the
			// client's would have no importer to take its hot updates, and Vite would reload the page.
			const { default: css } = await server.ssrLoadModule(
				`${url}${url.includes("?") ? "&" : "?"}inline`,
			);
			if (typeof css !== "string") throw new Error(`${url} has no CSS`);
			styles.push({
				tag: "style",
				attrs: { "data-vite-dev-id": module.id },
				children: css,
				injectTo: "head",
			});
			return;
		}
		// Dependencies and virtual modules bring no stylesheets of the site's.
		for (const { id, url: imported } of module.importedModules)
			if (id && !id.includes("/node_modules/") && !id.startsWith("\0"))
				await walk(imported);
	};
	await walk(entry);
	return styles;
}

// The dev server serves each page rendered into its HTML's outlet, as the build's prerender.ts
// writes it, so hydration meets the same markup in both, and with its stylesheets inline, so it
// paints styled before its scripts run, as the build's does.
function renderInDev(): Plugin {
	return {
		name: "nightcap:render-in-dev",
		apply: "serve",
		transformIndexHtml: {
			order: "post",
			async handler(html, { server, filename }) {
				if (!server) throw new Error("rendering a page needs the dev server");
				const { render } = await server.ssrLoadModule("/src/render.tsx");
				const page = relative(site, filename);
				const markup = await render(page);
				return {
					html: html.replace("<!--app-->", () => markup),
					tags: await devStyles(server, entryScript(page)),
				};
			},
		},
	};
}

export default defineConfig(({ command }) => ({
	// Absolute, since Pages serves 404.html at whatever path was missing.
	base: "/",
	plugins: [
		design(),
		changelog(),
		tailwindcss(),
		react(),
		pageHeads(),
		renderInDev(),
	],
	css: {
		// Readable, and the same in the client and server builds, whose markup must match.
		modules: {
			generateScopedName: (name, filename) =>
				`${basename(filename).split(".")[0]}_${name}`,
		},
	},
	// A path that isn't a file is a 404, as on Pages, rather than the home page again.
	appType: "mpa",
	server: { fs: { allow: [".."] } },
	// The server build runs once, from outside the site's folder, so it carries its dependencies.
	ssr: command === "build" ? { noExternal: true } : {},
	build: {
		outDir: "../build/site",
		emptyOutDir: true,
		// Lightning CSS lowers the tokens' light-dark() to variables for the build target's Safari,
		// which predates light-dark().
		cssMinify: "lightningcss",
		rolldownOptions: {
			input: Object.fromEntries(
				Object.entries(PAGES).map(([file, { entry }]) => [entry, site + file]),
			),
			output: {
				assetFileNames: ({ names }) =>
					names.some((name) => name.endsWith(".woff2"))
						? "fonts/[name]-[hash][extname]"
						: "assets/[name]-[hash][extname]",
			},
		},
	},
}));
