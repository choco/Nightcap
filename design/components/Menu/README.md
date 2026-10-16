A live mock of Nightcap's menu, hanging from the Mac menu bar: the product's main surface, recreated in the brand's tokens so it follows the theme.

Use it as the page's proof: show the menu in the state the section talks about, such as forced off in Figma, a timed disable, or a problem to fix. Keep the app's exact wording and order, as `Core::menu()` builds it:

1. The status line, bold and greyed ("Night Shift: Forced Off"), with its reason as a subtitle ("While Figma is in front").
2. While anything needs the user, the problem line ("1 Thing Needs Attention…") with the orange triangle and the problems as its subtitle.
3. "Turn Night Shift On" or "Off", with a subtitle saying what it does during a force; "Turn Off For ▸"; "Resume" while there's something to end.
4. The "Force Off" header over "In <app>" and, indented, "On <domain>", checked when a rule applies, with "From your config" or "Snoozed until 07:00".
5. "Settings… ⌘,", "Check for Updates…", "Quit Nightcap ⌘Q".

The consumer provides the menu bar (`nc-menubar`, with the moon from `Icons/` drawn inline and `nc-menubar__dot` while there's a problem) and the items. Mark the hovered item `nc-menu__item--current`, disabled ones `--disabled`, the status line `--status`.

- Do: real app names, real times, the menu the app would show in that state.
- Don't: invent items, or show a state the app can't be in.
