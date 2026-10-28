# The core is a server on its own thread with one mailbox, and the shell its one listener

The shell observes facts (app switches, hosts, status changes, sun times, timers firing) and carries out what the core decides; the core decides (ADR-0001). Two clients send it commands: the menu-bar UI, in the same process, and the CLI, over the socket (ADR-0010). The boundary needs generated bindings, a threading model, and one order in which the platform's facts and both clients' commands are decided, so that a CLI command's effects can't overtake a menu click's.

## Decision

The core runs on a thread of its own, with one mailbox, behind a single `Core` object exposed through UniFFI:

- `Core::start(…, listener)` binds the socket first, so a second instance fails before any file is read, then starts the core's thread and the socket's.
- `report(Fact)` takes what macOS observed, `send(Command)` a command from the menu, Settings, onboarding or the shortcut, the same commands as the CLI's, and `send_ui(UiCommand)` one of the UI's own: the tour's start and end, onboarding's finish and the fixes, which never go on the wire (ADR-0016). Each puts the input into the mailbox and returns at once; whoever sends one never sees what follows from it.
- The thread takes one input at a time, in the order they arrived, and decides it with `react(&mut Inner, Input) -> Outcome`, a plain function over the state the thread alone owns. It publishes a snapshot of that state, then hands the outcome to the listener.
- An outcome is the effects and the events one input led to, in the order decided. Effects are what macOS must do, such as setting Night Shift, watching a browser or scheduling the next wakeup. Events are what happened, told once, as facts rather than sentences: an external change adopted, the first launch after an update, a problem that appeared, a command from the UI refused. The shell words the notifications they call for.
- The listener is a UniFFI foreign trait the shell implements, called on the core's thread between inputs and outside any lock. The shell's `CoreHost` hops each outcome to the main queue, which keeps the core's order, carries out its effects through the adapters that drive macOS and hands its events to the clients that subscribed, then has the UI redraw once.
- The CLI's commands, reloads and update checks go into the same mailbox; the socket's thread answers once the core's thread has decided them, and their outcomes reach the listener like any other. The queries, the same in process and over the socket (`night_shift()`, `front()`, `rules()`, `config()`, `preferences()`, `checks()`, `tour()`, `about()`), and the shell's other reads (`menu()`, `settings()`, `onboarding()`, `tour_report()`, `export()`), answer from the last snapshot without waiting on the mailbox, so a read can lag one input behind; the outcome that follows tells the shell to read again.
- `stop()` is the one call that waits. It puts quitting into the mailbox behind everything already there and returns what releases every force (ADR-0008); the shell carries out the outcomes still on their way, then that one, before the app exits.
- The core computes when to wake (the next schedule transition or the end of a rule); the shell owns the wall-clock timer, because Rust's monotonic clocks stop while the Mac sleeps, and reports `TimerFired` when it's due.

It wins because one order holds for every input whoever sends it, the CLI is answered without the shell's main thread, and the core can still be tested on Linux as a function from inputs to outcomes.

## Consequences

- Rust panics can't cross the FFI boundary, so the core builds with `panic = "abort"` and a panic hook that logs before the abort. Under a LaunchAgent, launchd then restarts the app (ADR-0008).
- The generated bindings live in their own Swift module. UniFFI's generated code breaks when a module's default isolation is the main actor ([uniffi-rs#2818](https://github.com/mozilla/uniffi-rs/issues/2818)), and default isolation is set per module, so the app module keeps the main actor while the bindings module stays `nonisolated`.
- A debug build of the core also exports `Core.staged(Scenario)`: a core in memory, with no thread, no socket and no disk, brought to a scene by its own code from the files' text and a list of facts and commands. It decides each input before `report` or `send` returns. The Swift tests and the dev build's snapshots use it rather than copying what the core would say, and the Rust tests drive it through `Staged`, which returns each input's outcome. It's compiled under `debug_assertions`, not behind a cargo feature, so every debug build has it and no release build does: the release bindings stay as they are, and release Swift code that names it doesn't compile.
- The shell can't wait for what its own input led to. What depends on the launch's outcome, such as onboarding or the menu-bar item, follows it as it's carried out, and a shortcut tap that leaves Night Shift on ends the peek once the next outcome is carried out (ADR-0015).
- `nightcap status` shows the strength as the shell last read it, at most 30 seconds old: CoreBrightness announces no strength change, and a query never reaches the shell.

## Rejected alternatives

- **Returning effects from a synchronous call on the main thread**: every CLI request, queries too, waits for a turn on the shell's main thread, which the shell schedules when a waker asks, so a busy main thread delays every answer. The order is the main thread's, and holds only while every input goes through it.
- **An async runtime (tokio)**: it pays off when many I/O tasks share it, and the core has one mailbox and a socket with a client or two, which a thread and `std::sync::mpsc` serve. Its timers stop while the Mac sleeps like any monotonic clock, and an async interface would make the Swift side async too.
- **cbindgen plus a hand-written Swift wrapper (Ghostty's approach)**: more glue to maintain, for no gain at this size.
- **swift-bridge**: pre-1.0 with one maintainer, and Firezone moved off it to UniFFI because of callback lifetime issues.
- **Async UniFFI calls and event streams**: UniFFI's async support is not clean under Swift 6 concurrency.
- **The core calling into Swift for every action**: the core would depend on Swift and couldn't be tested on its own. The listener takes whole outcomes, as data, so the core still never calls a macOS API.
