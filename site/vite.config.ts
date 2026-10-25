import { basename } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";
import { design } from "./plugins/design.ts";

// The dev server serves the page rendered into index.html's outlet, as the build's prerender.ts
// writes it, so hydration meets the same markup in both.
function renderInDev(): Plugin {
	return {
		name: "nightcap:render-in-dev",
		apply: "serve",
		transformIndexHtml: {
			order: "post",
			async handler(html, { server }) {
				if (!server) throw new Error("rendering the page needs the dev server");
				const { render } = await server.ssrLoadModule("/src/render.tsx");
				const markup = await render();
				return html.replace("<!--app-->", () => markup);
			},
		},
	};
}

export default defineConfig(({ command }) => ({
	// Relative, so the built site works under any path.
	base: "./",
	plugins: [design(), tailwindcss(), react(), renderInDev()],
	css: {
		// Readable, and the same in the client and server builds, whose markup must match.
		modules: {
			generateScopedName: (name, filename) =>
				`${basename(filename).split(".")[0]}_${name}`,
		},
	},
	// A path that isn't a file is a 404, as on Pages, rather than the page again.
	appType: "mpa",
	server: { fs: { allow: [".."] } },
	// The server build runs once, from outside the site's folder, so it carries its dependencies.
	ssr: command === "build" ? { noExternal: true } : {},
	build: {
		outDir: "../build/site",
		emptyOutDir: true,
		rolldownOptions: {
			output: {
				assetFileNames: ({ names }) =>
					names.some((name) => name.endsWith(".woff2"))
						? "fonts/[name]-[hash][extname]"
						: "assets/[name]-[hash][extname]",
			},
		},
	},
}));
