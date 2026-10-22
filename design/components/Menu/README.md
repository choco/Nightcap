A live mock of Nightcap's menu, hanging from the Mac menu bar: the product's main surface, recreated in the brand's tokens so it follows the theme.

Use it as the page's proof: show the menu in the state the section talks about, such as a rule keeping Night Shift off in Figma, a snooze, or a problem to fix. Keep the app's exact wording and order, as `Core::menu()` builds it:

1. While anything needs the user, the problem line ("1 Thing Needs Attention…") with the orange triangle and the problems as its subtitle, then a separator.
2. The Night Shift item: checked while Night Shift is on everywhere, the moon in its menu-bar look, what decides the screen as its subtitle ("Off in Figma: your config’s rule"), and the shortcut ("⌃⌥⌘N").
3. The strength's slider, between a small sun and a large one.
4. Everywhere's row, "Turn Off ›" or "Turn On ›", with the timer.
5. The front app's row with its icon, "Turn On in Figma ›", and what its place holds as a subtitle ("Always off: your config’s rule"). Under it, indented, the site's, "Turn Off at This Site ›", with the site's icon or the globe and the domain on the subtitle's first line.
6. "Settings… ⌘," and "Quit Nightcap ⌘Q", with the gear and the power symbol.

A subtitle breaks where the app breaks it: whole up to 160 points of 12-point text, otherwise at the word that evens out two lines. Write the break into the text.

The consumer provides the menu bar (`nc-menubar`, with the moon from `Icons/` drawn inline and `nc-menubar__dot` while there's a problem) and the items. An item's image goes in `nc-menu__image`, drawn like the moons on a 24 grid, or as `nc-menu__app` with the app's colour in `--tile`; the slider's knob sits at `--value`, from 0 to 1. Mark the hovered item `nc-menu__item--current`, disabled ones `--disabled`, the site's row `--indented`.

- Do: real app names, real times, the menu the app would show in that state.
- Don't: invent items, or show a state the app can't be in.
