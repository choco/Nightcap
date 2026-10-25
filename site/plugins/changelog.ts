import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Marked, Parser, type Token } from "marked";
import type { Plugin } from "vite";

const source = fileURLToPath(new URL("../../CHANGELOG.md", import.meta.url));
const id = "virtual:changelog";
const resolved = `\0${id}`;

// A version's heading: "1.1.0 · 25 October 2026".
const heading = /^(\d+\.\d+\.\d+) · (\d{1,2} [A-Z][a-z]+ \d{4})$/;

const marked = new Marked({
	renderer: {
		heading({ depth, text }) {
			if (depth !== 2) return false;
			const match = heading.exec(text);
			if (!match)
				throw new Error(
					`CHANGELOG.md: "## ${text}" isn't a version and its day, as "## 1.1.0 · 25 October 2026"`,
				);
			const [, version, day] = match;
			return `<h2 id="v${version}"><span class="version">${version}</span> <span class="day">${day}</span></h2>\n`;
		},
	},
});

/** CHANGELOG.md's title, the paragraph under it as plain text, and its versions as HTML, each
 *  version's heading anchored as `v<version>`, which the app links to after an update. */
function render(markdown: string) {
	const tokens: Token[] = marked
		.lexer(markdown)
		.filter((token) => token.type !== "space");
	const [title, lead] = tokens;
	if (
		title?.type !== "heading" ||
		title.depth !== 1 ||
		lead?.type !== "paragraph" ||
		!lead.tokens
	)
		throw new Error("CHANGELOG.md doesn't open with its title and a paragraph");
	const parser = new Parser();
	return {
		title: title.text,
		lead: parser.parseInline(lead.tokens, parser.textRenderer),
		html: marked.parser(tokens.slice(2)),
	};
}

/** The repo's CHANGELOG.md as `virtual:changelog`, rendered at build time, so the page ships its
 *  HTML and not a markdown parser. */
export function changelog(): Plugin {
	return {
		name: "nightcap:changelog",
		resolveId(name) {
			return name === id ? resolved : undefined;
		},
		load(name) {
			if (name !== resolved) return undefined;
			this.addWatchFile(source);
			const { title, lead, html } = render(readFileSync(source, "utf8"));
			return [
				`export const title = ${JSON.stringify(title)};`,
				`export const lead = ${JSON.stringify(lead)};`,
				`export const html = ${JSON.stringify(html)};`,
			].join("\n");
		},
	};
}
