Nightcap is a small Mac app that switches Night Shift off while chosen apps and websites are in front, and gives it back when you leave them. The brand is the app icon: a navy night sky with a gold crescent. Everything else comes from one contrast the product is about: a screen warmed by Night Shift's amber, and the same screen in true colour. Night is the home theme; Day is its morning.

## Voice

Write the way the app talks: plain, specific, and about what the user sees.

- Address the reader as "you". Nightcap is "Nightcap", never "we".
- Name real things: apps by their names, times as times. "While Figma is in front", "Following the schedule until 07:12", "Snoozed until 07:00".
- Say what happens, in the active voice. "Turn On", "Not Now", "Open at Login". A button never says "Enable" or "Activate".
- Menu items and buttons use macOS title case ("Turn Night Shift On", "Check for Updates…"). Headlines and running text use sentence case.
- An ellipsis means the action opens something more: "Settings…", "Add Website…", "1 Thing Needs Attention…".
- Use the product's own terms: force off, snooze, timed disable, website checking, check. A problem is a problem, never an "issue".
- Be dry, not cute. The wit is in the name and the moon, so the words can stay straight: "Nightcap switches Night Shift off while the apps and websites you choose are in front, and gives it back when you leave them."
- Say what Nightcap doesn't do when it matters: "It reads the front tab's address with Accessibility and keeps only the site's name."
- No emoji, no exclamation marks.

## Colour

- Lay pages on `ground`, sections on `surface`, floating things on `surface-raised` with `shadow-lift`. Divide with `line`; give controls their edge with `line-strong`.
- Set text in `ink`, secondary text in `ink-muted`. Gold words use `moon-text`, warm words `ember-text`.
- `true-colour` is ember's counterpart: a mark (a dot, a highlight) for what the screen looks like with Night Shift off. Pair the two to tell a warm screen from a true one.
- `night` and `night-deep` are the brand's navies in both themes. Use them for night panels: the hero, the terminal, a feature that happens after dark. Text on them is `starlight` or `starlight-muted`, whatever the theme.
- `moon` is the one accent. Spend it on the primary action, the moon itself and at most one highlight per view. Text on it is `on-moon`.
- `ember` and `tint` show Night Shift's warmth and nothing else: the tinted half of a before/after, a screen warming as the schedule starts. Never use them for buttons, links or status.
- `ok`, `drift`, `broken` and `off` are the app's check states, with the app's own values. Each always travels with its shape (circle-check, triangle, x-circle, minus) and a word, since colour alone doesn't tell them apart, and the light values are too faint on their own.
- Every focusable thing gets a 2px `focus` ring, 2px out.

## Type

- Set headlines in Caprasimo (`display-xl`, `display-l`, `display-m`): a soft, heavy serif with the warmth of a bedtime story. Use one `display-xl` per page. Keep Caprasimo at 40px or larger on wide screens; phones may take it down to 32px to keep a headline on two lines. Never set it in capitals or on more than two lines.
- Set everything else in Figtree: `lead` under a headline, `body` for running text, `title` for cards and steps, `small` for captions, `label` in capitals for eyebrows.
- Set commands, config, rule identities, versions and times in Martian Mono (`code`, `data`): anything a user might type back.
- All three faces are under the SIL Open Font License; their licences sit in `licenses/` and ship with the font files.
- The wordmark is "Nightcap" in Caprasimo at `display-m` or larger, beside or under the app icon, in `ink` (or `starlight` on a night panel). It has no separate logo file.
- Keep running text near 65 characters a line; balance headlines.

## Space and shape

- Space with the `space-` steps only. Gutters are `space-4` on phones and `space-12` on wide screens; sections sit `space-18` apart on phones and `space-30` on wide screens.
- Buttons are capsules (`radius-pill`), as in the app. Cards, groups, the terminal and the menu mock use `radius-md`; chips and small tiles `radius-sm`. `radius-icon` is for the app icon's tile alone.
- Only floating things cast a shadow, and it's always `shadow-lift`.

## Motion

- The main idea moves: the screen warms and cools. A `tint` overlay fades in over 900ms, ease-in-out, when "Night Shift" starts, and out over 450ms when Nightcap forces it off, the same direction every time. A true-colour circle, cut out of the tint, follows the pointer and wanders over the nearest illustration when the pointer rests.
- Everything else moves slowly and by itself, like a night sky: doodles twinkle, drift and sway, suns and orbit rings turn over a minute or more, an illustration bobs a few pixels. Headlines rise in once, on load, with a little spring; buttons spring on hover.
- Motion never carries meaning on its own, and never loops fast enough to pull the eye from the text.
- With reduced motion, swap states instantly and keep everything still.

## Imagery

- Show the real thing: the Mac menu bar with the moon in it, the Nightcap menu, a screen with and without the tint. Build these as live mocks with the `Menu` component and `tint`, not screenshots, so they follow the theme.
- Illustrations stay geometric: moons, discs, crescents cut from `night` and `moon`, a sun at the edge of a hero or a dusk panel. No mascots, no faces.
- Doodles are line art in the iconography's style (24 grid, 1.5px strokes): moons, suns, stars, sparkles and small dots, scattered evenly through a page's margins and sparsely across its middle, never over text. Most are drawn in a faint `ink-muted`, a fifth or so in a faint gold; on phones keep about one in four.

## In the app

The app is a native Mac app and looks like one; this book's web look stays on the web.

- Use the system font, the system's window, group and control colours, and the user's accent colour for primary buttons and links. Build it in AppKit.
- Carry the brand over where the app already shares it: the app icon and the menu-bar moons, `ok`, `drift`, `broken` and `off` with their shapes, capsule buttons, 12-point groups and 7-point tiles, and the voice.
- Allow two brand moments: the wordmark in onboarding's header, in Caprasimo at 26 points (the app bundles the face with its licence), and the gold of onboarding's current step, `moon-text` by day and `moon` at night.
- Keep Figtree and Martian Mono, the navy surfaces, gold primary buttons, doodles, the tint and ambient motion on the web.

## Iconography

- The app icon (`Logos/nightcap-app-icon.svg`, from the repo's `app/icons/AppIcon.svg`) stands for Nightcap wherever a mark is needed: downloads, the footer, favicons.
- The menu-bar moons (`Icons/`, from the app's asset catalog) mean Night Shift on (filled), off (outline) and forced off (outline with a "z"). They're macOS template images, drawn in black.
- Check states use the app's shapes: circle-check, triangle with "!", x-circle, minus-circle (see `StateMark`).
- Draw any other glyph to match the moons: a 24 grid, 2px strokes, round joins. No emoji.
