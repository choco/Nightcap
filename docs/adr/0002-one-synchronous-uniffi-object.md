# The shell and the core talk through one synchronous UniFFI object

The shell observes facts (app switches, hosts, status changes, sun times, menu commands, timers firing) and applies Night Shift changes; the core decides (ADR-0001). The boundary between them needs generated bindings, a threading model, and a way for the core's own threads, such as the IPC server, to get the shell's attention without the core calling into Swift.

## Decision

The core exposes a single `Core` object through UniFFI, called synchronously and only from the main thread:

- `handle(Event) -> Vec<Effect>` is called for every fact the shell observes.
- `tick() -> Vec<Effect>` is called when the core's own threads have work. A `Waker` foreign trait lets the IPC thread ask for a tick, and the shell merges those requests into one `DispatchQueue.main.async { tick() }`. This is Ghostty's wakeup pattern.
- Effects are plain data, such as setting Night Shift, posting a notification, watching a browser or scheduling the next wakeup. The core computes when to wake (the next schedule transition or the end of a timer); the shell owns the wall-clock timer, because Rust's portable clocks stop while the Mac sleeps.
- What the menu and the settings window show is pulled: `menu()` and `settings()` return a snapshot, which the shell draws after each `handle` and `tick`, so no effect carries UI.
- The browser watcher hops its results to the main thread before they reach the core.
- The core mirrors the last observed Night Shift status, so the CLI's `status` never needs a round trip into Swift.

It wins because the core can be tested as a pure state machine on Linux, by replaying events and checking effects.

## Consequences

- Rust panics can't cross the FFI boundary, so the core builds with `panic = "abort"` and a panic hook that logs before the abort. Under a LaunchAgent, launchd then restarts the app (ADR-0008).
- The generated bindings live in their own Swift module. UniFFI's generated code breaks when a module's default isolation is the main actor ([uniffi-rs#2818](https://github.com/mozilla/uniffi-rs/issues/2818)), and default isolation is set per module, so the app module keeps the main actor while the bindings module stays `nonisolated`.

## Rejected alternatives

- **cbindgen plus a hand-written Swift wrapper (Ghostty's approach)**: more glue to maintain, for no gain at this size.
- **swift-bridge**: pre-1.0 with one maintainer, and Firezone moved off it to UniFFI because of callback lifetime issues.
- **Async UniFFI calls and event streams**: UniFFI's async support is not clean under Swift 6 concurrency.
- **The core calling into Swift for every action**: the core would depend on Swift and couldn't be tested on its own.
