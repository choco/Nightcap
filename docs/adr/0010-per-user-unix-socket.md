# CLI and core talk over a per-user Unix socket with a versioned handshake

The CLI talks to the server (ADR-0002, ADR-0005). The channel needs a transport, framing, a version check (the CLI and the server can briefly differ between activation and the agent restarting), a location macOS won't clean up, and an answer on authentication.

## Decision

- The core's IPC thread serves a Unix socket from a 0700 directory that macOS doesn't clean up, `~/Library/Application Support/Nightcap/`. The socket path stays under 104 bytes.
- Messages are one JSON object per line, request and response.
- A request is a command or any number of queries, and the IPC thread never touches the core's state. The queries (Night Shift, what's in front, the rules, the config, the preferences, the checks, the trial, the install, or an export) are answered in the order asked, all from the one snapshot the core published after its last input, as the shell's own reads are. `nightcap status` asks everything it shows in one request. A command, the same the app sends, reloading the config and checking for updates among them, goes into the core's mailbox with the app's own inputs, and is answered once the core's thread has decided it (ADR-0002); its outcome reaches the shell like any other.
- Every connection starts with a `Hello` carrying the protocol version. A mismatch fails hard: the server answers with its own `Hello`, from which the CLI words a restart hint, and the CLI takes a reply to its hello that it can't read, as from an older server, for a mismatch too.
- Every reply carries facts, never words, which the CLI words itself: a query's answer, a command's decision, a reload's rules, an update check started, or a refusal saying why the server refused, with what it names, such as the config rule that decides there.
- The wire format is internal, and changes with the protocol version. What scripts read is the CLI's own: `--json` prints a schema the CLI maps from the answers, versioned by its `schema` field, which `docs/installing.md` documents and which changes only on purpose.
- A `subscribe` stream is reserved for status consumers such as SketchyBar.
- There is no peer authentication. Any process running as the user can already call CoreBrightness directly, so the trust boundary is the user account.

Where each part comes from:

- **The transport** is the most common one among comparable tools: AeroSpace, yabai, espanso, Karabiner.
- **The handshake** follows Glide, AeroSpace and kanata.
- **The 0700 directory** avoids AeroSpace's shared `/tmp` and umask problem.

## Rejected alternatives

- **One query per request**: `nightcap status` would ask each over a connection of its own, and an input decided between two of them would mix answers from different inputs in one status.
- **A socket in `$TMPDIR`**: macOS cleans temp files that haven't been accessed recently.
- **A Mach service declared in the plist (paneru)**: unusual code, only to avoid a race during restarts.
- **XPC with a code-signing check on the peer (Karabiner)**: protects nothing here, since any process running as the user can already reach CoreBrightness.
- **TCP (kanata)**: an unauthenticated network surface.
- **launchd socket activation**: none of the comparable tools uses it.
