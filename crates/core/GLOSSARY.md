# Control

How Nightcap decides whether Night Shift is forced off or left alone, from rules, the schedule and the user's own toggles.

## Language

### Schedule

**Schedule**:
Night Shift's schedule in System Settings: off, sunset to sunrise, or custom hours. The system owns it.

**Scheduled state**:
Whether the schedule says Night Shift should be on right now.

**Schedule transition**:
A moment when the schedule starts or ends.
_Avoid_: boundary

**Strength**:
How warm Night Shift tints the display. The system owns it.
_Avoid_: temperature, warmth

### Toggles

**Manual state**:
The on/off the user set last, through Nightcap or by an adopted external change. It lasts until the next schedule transition.
_Avoid_: override

**External change**:
A Night Shift toggle made outside Nightcap, in Control Center or System Settings.

### Rules

**Context**:
The frontmost app and the host: what rules match against.

**App rule**:
Forces Night Shift off while an app is frontmost.

**Domain rule**:
Forces Night Shift off on a domain and all its subdomains.
_Avoid_: subdomain rule, exact-host rule

**Host**:
The hostname of the front browser tab: the only part of its URL Nightcap ever sees.

**Website checking**:
Whether Nightcap reads the front browser's host, which domain rules need. Off until the user turns it on.
_Avoid_: website monitoring, site tracking

**Rule identity**:
A rule's kind plus its target, e.g. `app:com.figma.Desktop`. Rules with the same identity are the same rule.

**Config rule**:
A rule declared in the Nix config. Read-only at runtime.
_Avoid_: Nix rule, static rule

**Ad-hoc rule**:
A rule added at runtime and kept by Nightcap until it graduates.
_Avoid_: runtime rule, dynamic rule

**Graduation**:
An ad-hoc rule's identity appearing in the config, at which point Nightcap drops its own copy.
_Avoid_: promotion, pruning

### Forcing

**Force**:
Night Shift held off by a rule or a timed disable.

**Hands off**:
No force applies, so Night Shift follows the manual state or the scheduled state.

**Snooze**:
A rule muted until the next schedule transition.
_Avoid_: override, pause

**Timed disable**:
Night Shift forced off until a deadline.
_Avoid_: pause
