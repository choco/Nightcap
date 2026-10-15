# CLI and core talk over a per-user Unix socket with a versioned handshake

The CLI talks to the daemon (ADR-0007). The channel needs a transport, framing, a version check (the CLI and the daemon can briefly differ between activation and the agent restarting), a location macOS won't clean up, and an answer on authentication.

## Decision

- The core's IPC thread serves a Unix socket from a 0700 directory that macOS doesn't clean up, such as `~/Library/Application Support/nightcap/`. The socket path stays under 104 bytes.
- Messages are one JSON object per line, request and response.
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
