The app's four check states as a mark and a word: ok, drift, broken and off, each with its own shape so colour is never the only signal.

Use it wherever the page shows what Nightcap checks: a grant, a file, Night Shift itself. The shapes are the app's SF Symbols, redrawn at 18px: circle-check (`ok`), triangle with "!" (`drift`), x-circle (`broken`), minus-circle (`off`).

The consumer provides the state class (`nc-mark--ok`, `--drift`, `--broken`, `--off`), the inline SVG for that state and its word. The word says what's true ("Allowed", "Switched off in System Settings"), not the state's name.

- Do: always pair the mark with a word.
- Don't: use a bare coloured dot, or `drift` for anything but a problem.
