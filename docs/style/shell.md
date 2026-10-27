# Shell

- **What a script is:**
  - Scripts are bash, starting with `#!/usr/bin/env bash`.
  - A script meant for `PATH` has no extension.
- **How it opens:** with a comment saying what it does and how to call it. Then `set -euo pipefail`, and `root="$(git rev-parse --show-toplevel)"`, so every path comes from the repository's root.
- **On macOS:** a script that may run under macOS's own `/bin/bash` keeps to bash 3.2's features. That rules out associative arrays, `mapfile` and `${var,,}`.
- **Quoting:**
  - Quote every expansion.
  - Pass lists of arguments as arrays, expanded with `"${array[@]}"`.
- **Failures:** a failure says `error: …` on stderr and exits non-zero.
- **shellcheck:** it runs clean. A deliberate exception is a `# shellcheck disable=SCxxxx` on its line, with the reason.
- **Logic lives in scripts,** and CI calls them by path, so a local run and CI do the same thing.
