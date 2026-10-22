// Writes the page, as the server build renders it, into the client build's index.html, then
// removes the server build: the site is static, and the browser hydrates what it's served.
import { readFile, rm, writeFile } from 'node:fs/promises';

const site = new URL('../build/site/', import.meta.url);
const server = new URL('../build/site-render/', import.meta.url);
const { render } = (await import(new URL('render.js', server).href)) as { render: () => Promise<string> };
const page = new URL('index.html', site);
const template = await readFile(page, 'utf8');
if (!template.includes('<!--app-->')) throw new Error('index.html has no <!--app--> outlet');
await writeFile(page, template.replace('<!--app-->', await render()));
await rm(server, { recursive: true });
