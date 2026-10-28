# CLI and core talk over a per-user Unix socket with a versioned handshake

The CLI talks to the server (ADR-0002, ADR-0005). The channel needs a transport, framing, a version check (the CLI and the server can briefly differ between activation and the agent restarting), a location macOS won't clean up, and an answer on authentication.

## Decision

- The core's IPC thread serves a Unix socket from a 0700 directory that macOS doesn't clean up, `~/Library/Application Support/Nightcap/`. The socket path stays under 104 bytes.
- Messages are one JSON object per line, request and response.
- The IPC thread never touches the core's state. A query (Night Shift, what's in front, the rules, the config, the preferences, the checks, the tour, the install, or an export) is answered from the snapshot the core published after its last input, as the shell's own queries are. `nightcap status` asks several, one per connection. A command, a reload or an update check goes into the core's mailbox with the menu-bar UI's own inputs, and is answered once the core's thread has decided it (ADR-0002); its outcome reaches the shell like any other.
- Every connection starts with a `Hello` carrying the protocol version. A mismatch fails hard with a restart hint.
- A `subscribe` stream is reserved for status consumers such as SketchyBar.
- There is no peer authentication. Any process running as the user can already call CoreBrightness directly, so the trust boundary is the user account.

Where each part comes from:

- **The transport** is the most common one among comparable tools: AeroSpace, yabai, espanso, Karabiner.
- **The handshake** follows Glide, AeroSpace and kanata.
- **The 0700 directory** avoids AeroSpace's shared `/tmp` and umask problem.

## Rejected alternatives

- **A socket in `$TMPDIR`**: macOS cleans temp files that haven't been accessed recently.
- **A Mach service declared in the plist (paneru)**: unusual code, only to avoid a race during restarts.
- **XPC with a code-signing check on the peer (Karabiner)**: protects nothing here, since any process running as the user can already reach CoreBrightness.
- **TCP (kanata)**: an unauthenticated network surface.
- **launchd socket activation**: none of the comparable tools uses it.
