---
status: superseded by ADR-0016
---

# Browser URLs come from Accessibility, on a dedicated thread

Domain rules need the host of the front browser tab whenever the frontmost app or the tab changes. The usual way to read a tab's URL on macOS is Apple Events. That needs one Automation consent per browser, has a default timeout of about two minutes, and through `NSAppleScript` runs only on the main thread, so one hung browser freezes app switching.

## Decision

The core decides when a browser is watched: only while it is frontmost and its host could change the verdict, meaning at least one domain rule is active and no active app rule already covers that browser. An app rule on a browser therefore means it is never watched, unless that rule is snoozed. With no domain rules at all, nothing is watched and Accessibility permission is never requested. The core starts and stops watching through effects, each start carrying a new generation number.

The shell's browser watcher runs on its own thread with its own run loop:

- it attaches an Accessibility observer to a browser when the core starts watching it, and removes the observer when the core stops;
- on focus and value changes, it reads the loaded document's URL from the browser's web area, with a short timeout per element. The address bar's text is never read, since it changes with every keystroke while the user types;
- each result is tagged with the generation it was watched under, then hopped to the main thread;
- the core drops results from older generations, and keeps the previous state while a read is in flight. If a read times out, only app rules apply.

The browser list is data: each entry maps a bundle ID to where its URL lives in the tree. Safari and Chrome are the first entries.

Why this wins:

- **One permission:** Accessibility, which the observers need anyway, with no per-browser Automation prompts.
- **Speed:** it was faster than Apple Events in testing.
- **Responsiveness:** the main thread never waits on a browser.
- **Lightness:** observers exist only while a host could matter.

## Consequences

- Browser accessibility trees are undocumented and can change with browser updates. A breakage is a one-entry fix.
- Chrome may expose the URL only with its full accessibility tree forced on. The fallback is a Chrome-only Apple Events entry, which brings back the Automation permission for Chrome alone.

## Rejected alternatives

- **Apple Events**: slower, one consent prompt per browser, a default timeout of about two minutes, and `NSAppleScript` only runs on the main thread.
- **Accessibility first, with Apple Events as a fallback for every browser**: keeps the Automation permission for a rare case.
- **Reading on the main thread**: a hung browser freezes app switching.
