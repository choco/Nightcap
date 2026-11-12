// Builds the design system's components from the site's, into design/components/: bundle.js, the
// components its previews mount, as `window.Nightcap` (src/bundle.tsx); bundle.css, the site's own
// stylesheet with its faces inline, so a preview draws exactly as the site does; lib/, the
// React they run on, the site's version; and index.d.ts, their types, for the design system to
// show their props. The design system loads lib/ before bundle.js.
import { execFileSync } from "node:child_process";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { build, type Plugin, type PluginOption } from "vite";
import { design } from "./plugins/design.ts";
import config from "./vite.config.ts";

const root = fileURLToPath(new URL(".", import.meta.url));
const out = new URL("../build/design/", import.meta.url);
// Where React's two entries are written: inside node_modules, so they find React.
const entries = new URL("node_modules/.tmp/bundle/", import.meta.url);
const target = new URL("../design/components/", import.meta.url);
const site = config({ command: "build", mode: "production" });

// The names, in order, that the design system lists as components, each with a card of its own.
const CARDS = ["Button", "Menu", "Scene", "Terminal"];

// React, wherever a script asks for it, is the one lib/react.js puts at `window.React`, so the
// components and React DOM share it.
const globalReact: Plugin = {
	name: "nightcap:global-react",
	enforce: "pre",
	resolveId: (source) => (source === "react" ? "\0global-react" : undefined),
	load: (id) =>
		id === "\0global-react" ? "module.exports = window.React;" : undefined,
};

/** One classic script assigning `window.<name>`. */
async function script(name: string, input: string, plugins: PluginOption[]) {
	await build({
		root,
		configFile: false,
		publicDir: false,
		logLevel: "warn",
		plugins,
		css: site.css,
		define: { "process.env.NODE_ENV": JSON.stringify("production") },
		// Classic, so JSX is `React.createElement`, which the script finds on `window.React`.
		oxc: { jsx: { runtime: "classic" } },
		build: {
			outDir: fileURLToPath(out),
			emptyOutDir: false,
			assetsInlineLimit: () => true,
			lib: {
				entry: input,
				name,
				formats: ["iife"],
				fileName: () => `${name}.js`,
				cssFileName: "bundle",
			},
		},
	});
}

await rm(out, { recursive: true, force: true });
await mkdir(entries, { recursive: true });
const react = new URL("react.js", entries);
const reactDom = new URL("react-dom.js", entries);
await writeFile(react, 'import React from "react";\nexport default React;\n');
await writeFile(
	reactDom,
	'import ReactDOM from "react-dom";\nimport client from "react-dom/client";\nexport default { ...ReactDOM, ...client };\n',
);
await script("React", fileURLToPath(react), []);
await script("ReactDOM", fileURLToPath(reactDom), [globalReact]);
await script("Nightcap", "src/bundle.tsx", [
	globalReact,
	design(),
	tailwindcss(),
]);

/** The components' types in one file: TypeScript's declarations of each module the bundle's entry
 *  reaches, its dependencies first. It's a reader's documentation, not a module, so the modules'
 *  imports of each other go, their declarations all being there, and the rest are gathered. */
async function types(): Promise<string> {
	const dir = new URL("types/", out);
	execFileSync(
		fileURLToPath(new URL("node_modules/.bin/tsc", import.meta.url)),
		[
			...["-p", "tsconfig.app.json", "--noEmit", "false"],
			...["--declaration", "--emitDeclarationOnly"],
			...["--rootDir", "src", "--outDir", fileURLToPath(dir)],
		],
		{ cwd: root, stdio: "inherit" },
	);
	const seen = new Set<string>();
	const outside = new Map<string, Set<string>>();
	const declarations: string[] = [];
	const visit = async (module: string) => {
		if (seen.has(module)) return;
		seen.add(module);
		const body: string[] = [];
		for (const line of (
			await readFile(new URL(`${module}.d.ts`, dir), "utf8")
		).split("\n")) {
			// An import for its effect, the stylesheet's, declares nothing.
			if (/^import "\./.test(line)) continue;
			const link = /^(import|export) (type )?\{([^}]*)\} from "([^"]+)";$/.exec(
				line,
			);
			if (!link) {
				body.push(line);
				continue;
			}
			const [, kind, , names, from] = link;
			if (!from.startsWith(".")) {
				const wanted = outside.get(from) ?? new Set();
				for (const name of names.split(","))
					wanted.add(name.replace(/^\s*type /, "").trim());
				outside.set(from, wanted);
				continue;
			}
			await visit(
				new URL(from, new URL(module, dir)).pathname.slice(
					new URL(dir).pathname.length,
				),
			);
			// A re-export under another name keeps the name; one under its own adds nothing.
			if (kind === "export" && names.includes(" as "))
				body.push(`export { ${names.trim()} };`);
		}
		declarations.push("", `// ${module}`, ...body);
	};
	await visit("bundle");
	return [
		"// The Nightcap components' types, from the site's source (pnpm --dir site bundle).",
		...[...outside].map(
			([from, names]) =>
				`import type { ${[...names].sort().join(", ")} } from "${from}";`,
		),
		...declarations,
	]
		.join("\n")
		.replace(/\n{3,}/g, "\n\n");
}

const files: [string, string][] = [
	["React.js", "lib/react.js"],
	["ReactDOM.js", "lib/react-dom.js"],
	["Nightcap.js", "bundle.js"],
	["bundle.css", "bundle.css"],
];
await mkdir(new URL("lib/", target), { recursive: true });
for (const [built, path] of files) {
	const text = await readFile(new URL(built, out), "utf8");
	// The design system inlines its scripts and stylesheet, which one of these would end early.
	if (/<\/script|<!--|<\/style/i.test(text))
		throw new Error(
			`${built} has text that would end the element it's inlined in`,
		);
	if (path === "bundle.js")
		await writeFile(
			new URL(path, target),
			`/* @ds-bundle: ${JSON.stringify({ format: 4, namespace: "Nightcap", components: CARDS.map((name) => ({ name })) })} */\n${text}`,
		);
	else await copyFile(new URL(built, out), new URL(path, target));
}
await writeFile(new URL("index.d.ts", target), `${await types()}\n`);
