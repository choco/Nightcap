# Swift

## Tooling

- **Language and isolation:** Swift 6 language mode, with strict concurrency. App targets use main-actor default isolation (`SWIFT_DEFAULT_ACTOR_ISOLATION = MainActor`). Generated or foreign code that can't take that default goes in a module of its own.
- **Warnings:** `SWIFT_TREAT_WARNINGS_AS_ERRORS = YES` on every target you own.
- **Formatting:** swift-format, as Xcode ships it (`xcrun swift-format`).
  - A `.swift-format` at the root lists every rule. A partial `rules` map turns off the rules it leaves out.
  - CI runs `lint --strict`.
- **Long string literals:** swift-format can't break them, so split them by hand. Split copy into concatenated sentences, and a log message into a multiline literal with escaped line ends.

## UI

- **AppKit throughout.**
- **Views draw the state they're given and report what the user does.** Logic that decides is plain Swift beside the view, importing no AppKit, so a test can compile it alone.
- **A `switch` over an enum from another module lists every case,** so a new case fails to compile wherever it needs handling.
- **Each shared piece has one home:** drawing helpers, art, a picker, a panel.

## Logging

`Logger(subsystem:category:)`, with private data interpolated as `privacy: .private`.

## Tests

- **Swift Testing:** `import Testing`, `@Test` and `#expect`. Under main-actor default isolation, an `XCTestCase` subclass's inherited initialisers don't compile.
- **Logic is tested in an unhosted bundle,** which compiles the files under test. It never launches the app, so tests can't take the app's sockets, defaults or system state.
- **Real collaborators:** tests build them, for example an in-memory instance, rather than hand-writing their output.
