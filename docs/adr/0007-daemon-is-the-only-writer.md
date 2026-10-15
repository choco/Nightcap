# The daemon is the only writer; every CLI command goes through it

The CLI is the control surface before the menu bar exists, and the scripting surface after: skhd, Raycast or Karabiner bind its commands. A convenient shortcut would let `on`, `off` and strength changes write CoreBrightness directly, so they work without a running daemon.

## Decision

Every CLI command goes through the daemon over the socket (ADR-0013). If the daemon is down, the CLI exits non-zero with a `launchctl kickstart` hint.

The core reconciles every status change against what it wants (ADR-0005). A second writer's changes would reach it as external changes, which it would have to adopt anyway.

## Rejected alternatives

- **Direct writes for on, off and strength**: two code paths, and the daemon would adopt those writes as external changes anyway.
- **No CLI**: nothing to see or control before the menu bar, and no scripting.
