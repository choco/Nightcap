# Website checking is opt-in; while it's on, the front browser's host comes from Accessibility, on a dedicated thread

Domain rules need the host of the front browser tab whenever the frontmost app or the tab changes. The usual way to read a tab's URL on macOS is Apple Events. That needs one Automation consent per browser, has a default timeout of about two minutes, and through `NSAppleScript` runs only on the main thread, so one hung browser freezes app switching. Reading through Accessibility avoids all three, but Accessibility is a broad permission, and asking for it deserves the user's consent up front. The menu should also be able to offer a first rule for the current site, before any domain rule exists.

## Decision

**Website checking** is a choice the user makes, in onboarding, Settings or the CLI; `websites` in the config, when set, wins over it.

- **Off:** nothing is watched, Accessibility is never requested, and domain rules do nothing.
- **On:** the frontmost browser is watched whenever one is in front, so the menu can offer a rule for the current site and `status` shows its host. Observers exist only while a browser is in front. The app asks for Accessibility when website checking is turned on without it, in Settings or with the CLI, as it sees the change, and onboarding asks on a page of its own. If the grant goes missing while it's on, the Accessibility check is a problem: the user either grants it again or stops checking websites.
- **A browser its own app rule keeps off** is watched like any other: a site decides over its browser (ADR-0006, ADR-0018), so the site's row in the menu can turn Night Shift on there, and `status` shows its host.
- **On the way out**, once Nightcap has released its forces, nothing is watched (ADR-0008).

Nightcap tracks the grant through the Darwin notification `com.apple.tcc.access.changed`, which `tccd` posts right after each write to its database, and reads it with TCC's private `TCCAccessCheckAuditToken`, which asks `tccd` and is already correct when the notification arrives. To ask, it shows the macOS prompt while Nightcap has no entry in the Accessibility list, and opens the pane in System Settings once it has one, switched on or off, since the prompt then never appears.

The shell's browser watcher runs on its own thread with its own run loop:

- it attaches an Accessibility observer to a browser when the core starts watching it, and removes the observer when the core stops;
- on focus and title changes, it reads the loaded document's URL from the browser's web area, with a short timeout per element. The address bar's text is never read, since it changes with every keystroke while the user types;
- each result is tagged with the generation it was watched under, then hopped to the main thread;
- the core drops results from older generations, and keeps the previous state while a read is in flight. If a read times out, only app rules apply.

The browser list holds the browsers that keep the page's URL on the first element under the focused window carrying one, links aside: Safari, Safari Technology Preview, Orion, Chrome, Edge, Brave, Arc, Dia, Firefox and Zen. One search serves them all. Firefox and Zen build their tree only once a client asks for the app's role, so the watcher asks for it whenever it starts watching a browser, and reads again, for up to two seconds, while it finds no page. Opera and Vivaldi aren't on the list: Opera shows Accessibility no page at all, and Vivaldi shows the address only in its address bar. Neither takes `AXManualAccessibility`, the switch some Chromium browsers have for it.

Why this wins:

- **Consent:** one permission, Accessibility, asked only after the user chose website checking, with no per-browser Automation prompts.
- **Speed:** it was faster than Apple Events in testing.
- **Responsiveness:** the main thread never waits on a browser.
- **Lightness:** observers exist only while a browser is in front, and not at all with website checking off.

## Consequences

- Browser accessibility trees are undocumented and can change with browser updates. A browser whose URL moves out of the search's reach drops off the list until the search finds it again.
- Watching Firefox or Zen turns on its accessibility engine, which can cost it some speed until it quits.
- Reading the grant and telling the prompt from the pane use TCC's private `TCCAccessCheckAuditToken` and `TCCAccessPreflight`, looked up at runtime. Without the first, Nightcap falls back to `AXIsProcessTrusted`; without the second, it opens the pane.
- Bringing a browser its own app rule keeps off to the front changes Night Shift once the first read reports the host, as with any browser.

## Rejected alternatives

- **Watching only while a domain rule could change the decision, and asking for Accessibility once one exists**: the menu couldn't offer a first site rule, and the permission request arrived as a surprise when a rule was added.
- **Not watching a browser its own app rule keeps off while no site in it could say on**: a site's row is how the user makes one say on there, so the menu could never offer it, and only the CLI could turn a site on in that browser.
- **A one-off read of the host when the menu opens**: a second way to read the host, and `status` and the menu would disagree about it.
- **`AXIsProcessTrusted` and the distributed `com.apple.accessibility.api` notification for the grant**: `AXIsProcessTrusted` answers from an in-process cache refreshed by that notification, which System Settings posts before `tccd` writes the change, so the old value can stick.
- **A public Accessibility call to read the grant**: the system-wide element's focused application fails with "cannot complete", not "API disabled", while the grant is off, so it can't tell a missing grant from a busy app. A call into another app does fail with "API disabled", but may answer from the same in-process cache as `AXIsProcessTrusted`.
- **Apple Events**: slower, one consent prompt per browser, a default timeout of about two minutes, and `NSAppleScript` only runs on the main thread.
- **Accessibility first, with Apple Events as a fallback for every browser**: keeps the Automation permission for a rare case.
- **Reading on the main thread**: a hung browser freezes app switching.
