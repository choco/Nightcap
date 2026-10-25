// Writes each page, as the server build renders it, into the client build's HTML, then removes
// the server build: the site is static, and the browser hydrates what it's served.
import { glob, readFile, rm, writeFile } from "node:fs/promises";

const site = new URL("../build/site/", import.meta.url);
const server = new URL("../build/site-render/", import.meta.url);
const { render, pageFiles } = (await import(
	new URL("render.js", server).href
)) as {
	render: (file: string) => Promise<string>;
	pageFiles: string[];
};

// Every page built, and every page built rendered.
const built: string[] = [];
for await (const file of glob("**/*.html", { cwd: site })) built.push(file);
built.sort();
if (built.join() !== [...pageFiles].sort().join())
	throw new Error(
		`the build's pages (${built.join(", ")}) aren't render.tsx's (${pageFiles.join(", ")})`,
	);
for (const file of pageFiles) {
	const url = new URL(file, site);
	const template = await readFile(url, "utf8");
	if (!template.includes("<!--app-->"))
		throw new Error(`${file} has no <!--app--> outlet`);
	const markup = await render(file);
	// A function, so `$&` and the like in the markup are left as they are.
	await writeFile(
		url,
		template.replace("<!--app-->", () => markup),
	);
}
await rm(server, { recursive: true });
