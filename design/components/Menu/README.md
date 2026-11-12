Nightcap's menu, recreated live so it follows the theme: the product's main surface, and the page's proof. It comes in two looks: `mac`, macOS's own, translucent, at the real menu's measures in em of the text its parent sets (13px is the real size); and `night`, a night panel in the site's colours, for the site's own illustrations.

Show the menu in the state the section talks about, such as a rule keeping Night Shift off in Figma. Its rows are the app's, in its order and words, as `Core::menu()` builds them: the Night Shift item, checked while Night Shift is on everywhere, with the moon in its menu-bar look, what decides the screen as its subtitle ("Off in Figma: your config’s rule") and the shortcut; the strength's slider between a small sun and a large one; "Turn Off ›"; the front app's row with its icon and what its place holds ("Turn On in Figma ›", "Always off: your config’s rule"), and under it, indented, the site's; then "Settings…" and "Quit Nightcap".

The consumer provides the `entries` (`MENUS` holds the menu with Mail, Figma or Safari in front) and the menu's placement, through `className`. A subtitle breaks where the app breaks it, so write the break into its lines.

- Do: real app names, real times, the menu the app would show in that state.
- Don't: invent rows, or show a state the app can't be in.
