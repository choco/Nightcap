import { basename, relative } from "node:path";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";
import { changelog } from "./plugins/changelog.ts";
import { design } from "./plugins/design.ts";
import { head, PAGES } from "./src/pages.ts";

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

// The dev server serves each page rendered into its HTML's outlet, as the build's prerender.ts
// writes it, so hydration meets the same markup in both.
function renderInDev(): Plugin {
	return {
		name: "nightcap:render-in-dev",
		apply: "serve",
		transformIndexHtml: {
			order: "post",
			async handler(html, { server, filename }) {
				if (!server) throw new Error("rendering a page needs the dev server");
				const { render } = await server.ssrLoadModule("/src/render.tsx");
				const markup = await render(relative(site, filename));
				return html.replace("<!--app-->", () => markup);
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
