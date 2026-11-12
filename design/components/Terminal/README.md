A terminal session with the `nightcap` command: a `night-deep` night panel in both themes, because a terminal is, set in Martian Mono at `code`, with a faint inner edge and `shadow-glow` under it.

Use it wherever the page shows commands a reader could run. Each line is a `Command`, which draws the gold prompt and `nightcap` in bold before its children, or that command's `Output`, muted. An `Arg` is gold and never breaks inside; `State` is Night Shift's state in output, "off" in `ember-text` or, with `on`, "on" in `ok`. Long lines wrap, as a terminal window's would; on phones the text comes down to 11px, narrowed.

The consumer provides the lines, an `aria-label` saying what the session shows, and the terminal's placement, through `className`. Copy the CLI's real output: it is the product's own voice.

- Do: real commands and real output (`nightcap off --for 1h30m`, `nightcap status --json`).
- Don't: invented flags, shortened output, a prompt the reader would copy along with the command.
