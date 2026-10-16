A capsule button, as in the app: primary in `moon` for the one action a view is about, secondary outlined in `line-strong` for the rest.

Use `nc-button nc-button--primary` once per view, for its main action ("Download for Mac"). Everything else is `nc-button--secondary`. On a `night` or `night-deep` panel add `nc-button--on-night` to a secondary button so its text and edge use `starlight`. `nc-button--large` is for the download button in the hero and nowhere else.

The consumer provides the element (`<a>` for navigation and downloads, `<button>` for actions), its label in title case, and, for a download, the version in `nc-button__meta` set in Martian Mono. An icon goes before the label as an inline SVG at 18px.

- Do: "Download for Mac", "View on GitHub", "Copy".
- Don't: "Get started", "Enable", two primary buttons side by side.
