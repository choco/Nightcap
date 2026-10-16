A terminal block for the `nightcap` CLI and config snippets: `night-deep` in both themes, Martian Mono, a gold prompt.

Use it wherever the page shows a command a reader could run, or the config they could write. It stays dark in the Day theme too, because a terminal is.

The consumer provides one `nc-terminal__line` per line. A command line starts with `<span class="nc-terminal__prompt">$ </span>`; output lines use `nc-terminal__out`, with `nc-terminal__key` for the part of an output line that names a value. Copy the CLI's real output: it is the product's own voice.

- Do: real commands and real output (`nightcap status`, `nightcap disable 1h30m`).
- Don't: invented flags, shortened output, a prompt the reader would copy along with the command.
