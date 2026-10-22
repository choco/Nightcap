import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { Plugin } from 'vite';

const source = fileURLToPath(new URL('../../design/tokens.json', import.meta.url));
const licences = fileURLToPath(new URL('../../design/licenses/', import.meta.url));
const output = fileURLToPath(new URL('../src/styles/tokens.css', import.meta.url));
const appIcon = fileURLToPath(new URL('../../app/icons/AppIcon.svg', import.meta.url));
const publicDir = fileURLToPath(new URL('../public/', import.meta.url));

type Value = string | { light?: string; dark?: string };
interface Token<V = string> {
  name: string;
  value: V;
}
interface Tokens {
  color: { tokens: Token<Value>[] };
  spacing: { tokens: Token[] };
  radius: { tokens: Token[] };
  shadow: { tokens: Token[] };
  type: {
    families: Record<string, string>;
    fonts: { family: string; weight: string; style?: string; file: string }[];
  };
}

// A value that names another token, such as "{night}", is that token's variable.
const css = (value: string) => (/^\{.+\}$/.test(value) ? `var(--${value.slice(1, -1)})` : value);

// A colour missing a theme takes the other's.
function themed(token: Token<Value>, theme: 'light' | 'dark'): string {
  const { value } = token;
  if (typeof value === 'string') return css(value);
  const picked = value[theme] ?? value[theme === 'light' ? 'dark' : 'light'];
  if (picked === undefined) throw new Error(`design/tokens.json: ${token.name} has no value`);
  return css(picked);
}

/** The design system's tokens as CSS: the fonts, the colours as variables, light on :root and
 *  dark for the dark system appearance unless the page was set to light, and for an explicit
 *  dark choice; then Tailwind's theme from them. */
export function tokensCss(tokens: Tokens): string {
  const colours = tokens.color.tokens;
  const dark = colours
    .filter((token) => typeof token.value === 'object')
    .map((token) => `--${token.name}:${themed(token, 'dark')}`)
    .join(';');
  // The spacing tokens are Tailwind's 4px scale, space-N being N steps.
  for (const { name, value } of tokens.spacing.tokens) {
    const steps = Number(name.replace(/^space-/, ''));
    if (value !== `${steps * 4}px`) throw new Error(`design/tokens.json: ${name} is ${value}, off the 4px scale`);
  }
  return [
    '/* Generated from design/tokens.json by site/plugins/design.ts. */',
    ...tokens.type.fonts.map(
      (font) =>
        `@font-face{font-family:"${font.family}";src:url("../../../design/${font.file}") format("woff2");font-weight:${font.weight};font-style:${font.style ?? 'normal'};font-display:swap}`,
    ),
    `:root{${colours.map((token) => `--${token.name}:${themed(token, 'light')}`).join(';')};color-scheme:light}`,
    `@media (prefers-color-scheme: dark){:root:not([data-theme="light"]){${dark};color-scheme:dark}}`,
    `:root[data-theme="dark"]{${dark};color-scheme:dark}`,
    // Inline, so a utility reads the colour's variable, which the theme switches.
    `@theme inline{--color-*:initial;${colours.map(({ name }) => `--color-${name}:var(--${name})`).join(';')}}`,
    // Static, so every variable reaches :root for the CSS modules too, which Tailwind doesn't see.
    `@theme static{--spacing:4px;--radius-*:initial;--shadow-*:initial;--font-*:initial;${[
      ...[...tokens.radius.tokens, ...tokens.shadow.tokens].map(({ name, value }) => `--${name}:${value}`),
      ...Object.entries(tokens.type.families).map(([family, stack]) => `--font-${family}:${stack}`),
    ].join(';')}}`,
    '',
  ].join('\n');
}

function writeTokens() {
  const css = tokensCss(JSON.parse(readFileSync(source, 'utf8')));
  // Unchanged content leaves the file alone, so the dev server doesn't reload for nothing.
  if (!existsSync(output) || readFileSync(output, 'utf8') !== css) writeFileSync(output, css);
}

/** What the site takes from the rest of the repo: src/styles/tokens.css from the design system's
 *  tokens, and public/icon.svg, the app icon, both written before Vite reads anything and again
 *  when their sources change while serving; and the fonts' licences beside the built site's
 *  fonts. */
export function design(): Plugin {
  return {
    name: 'nightcap:design',
    config() {
      writeTokens();
      mkdirSync(publicDir, { recursive: true });
      copyFileSync(appIcon, `${publicDir}icon.svg`);
    },
    configureServer(server) {
      server.watcher.add([source, appIcon]);
      server.watcher.on('change', (path) => {
        if (path === source) writeTokens();
        if (path === appIcon) copyFileSync(appIcon, `${publicDir}icon.svg`);
      });
    },
    generateBundle() {
      if (this.environment.name !== 'client') return;
      for (const licence of readdirSync(licences)) {
        this.emitFile({ type: 'asset', fileName: `fonts/${licence}`, source: readFileSync(licences + licence) });
      }
    },
  };
}
