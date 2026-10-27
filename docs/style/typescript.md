# TypeScript, React and CSS

## Tooling

- **Type checking:** TypeScript runs in strict mode, with `tsc -b` in CI.
- **Lint and format:** Biome, with its recommended rules. CI runs `biome ci`.
- **Dependencies** are pinned to exact versions at least two weeks old.
  - pnpm's `minimumReleaseAge` enforces the age for transitive dependencies too.
  - Dependabot's npm `cooldown` matches it.

## React

- **Hydration:** a prerendered page hydrates into the same markup it was built with.
- **The visitor's clock, locale and window** apply after hydration, by changing text or transforms, never layout.

## CSS

- **Layout and type:** Tailwind, themed from the design tokens.
- **Measured drawings** keep their measures in a CSS module beside their component.

## Motion

WebKit's compositor runs `transform` and `opacity` keyframe animations off the main thread. These all run on the main thread instead, restyling every animated element on the page, every frame:

- any other animated property, such as `box-shadow`, a mask or a colour;
- a `reverse` direction;
- a script loop;
- a listener for animation events;
- a moving layer the page's size that has pixels to draw.

A transition keeps that main thread busy for as long as it runs, even one of opacity.

So:

- **Animate with `transform` and `opacity` keyframes,** and leave animation events unlistened.
- **After the first paint, move things only by transform.** A box that changes its size or place is a layout shift, which Lighthouse scores.
- **Check a change's idle CPU in Safari and its CLS in Lighthouse.** Safari is where the cost shows.

## Accessibility

An illustration that changes when clicked says what happened in a polite live region, in the words it shows.
