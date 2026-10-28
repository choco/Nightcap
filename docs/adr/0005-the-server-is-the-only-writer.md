# The server is the only writer; every CLI command goes through it

The CLI is Nightcap's scripting surface: skhd, Raycast or Karabiner bind its commands. A convenient shortcut would let `on`, `off` and strength changes write CoreBrightness directly, so they work without Nightcap running.

## Decision

Every CLI command goes through the server over the socket (ADR-0002, ADR-0010). If Nightcap isn't running, the CLI exits 1, saying so.

The core reconciles every status change against what it wants (ADR-0004). A second writer's changes would reach it as outside changes, which it would have to adopt anyway.

## Rejected alternatives

- **Direct writes for on, off and strength**: two code paths, and the server would adopt those writes as outside changes anyway.
- **No CLI**: nothing for scripts or hotkeys to call.
