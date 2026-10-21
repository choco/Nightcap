# Control

How Nightcap decides whether Night Shift is on or off, from rules, the schedule and the user's own changes.

## Language

### Schedule

**Schedule**:
Night Shift's schedule in System Settings: off, sunset to sunrise, or custom hours. The system owns it; Nightcap changes it only when the user asks.

**Scheduled state**:
Whether the schedule says Night Shift should be on right now.

**Schedule transition**:
A moment when the schedule starts or ends.
_Avoid_: boundary

**Strength**:
How warm Night Shift tints the display. The system owns it; Nightcap changes it only when the user asks.
_Avoid_: temperature, warmth

### Changes

**Place**:
Where a change applies: everywhere, the app in front, or the site in front. A site is a domain, covering its subdomains: the most specific rule's that covers the tab, else the tab domain.

**Layer**:
One of what a place holds, from the bottom: its rule, or the schedule for everywhere; a hold; a timer. A place says what its top layer says, and an app or site without one says nothing.

**Deciding place**:
The most specific place in front that says something: the site, then the app, then everywhere, which always does. What it says is what Night Shift does.

**Length**:
How long a change lasts: for a time, until a picked time, until the schedule next turns it back, or until the user turns it back.
_Avoid_: duration

**Timer**:
A place's on or off until a time, which then gives back what was under it.
_Avoid_: timed disable, pause

**Hold**:
A place's on or off until the schedule next turns it back, or, for everywhere, until the user turns it back.
_Avoid_: manual state

**Switch**:
Everywhere's on and off: the Night Shift item, the shortcut's tap, `nightcap on`, `off` and `toggle`. It takes off everywhere's layers that say otherwise, and if the schedule still does, holds the new state until the user turns it back.
_Avoid_: toggle, as a noun

**Reset**:
Taking off a place's hold and timer, which puts its rule back or, for everywhere, the schedule.

**External change**:
A Night Shift toggle made outside Nightcap, in Control Center or System Settings.

**Adoption**:
Taking an external change as flipping what the user sees: it takes off what the deciding place says, and holds the other state over a rule or the schedule until the schedule next turns it back.

**Tour stand-in**:
A tour started with Night Shift off has it on for the tour's length, in place of the schedule. Everywhere's hold and timer wait aside until the tour ends, which puts them back unless the user chose others meanwhile (ADR-0022).

### Rules

**Context**:
The frontmost app and the host: what rules match against.

**App rule**:
Says Night Shift is off, or on, while an app is frontmost.

**Domain rule**:
Says Night Shift is off, or on, on a domain and all its subdomains. Where several cover a host, the longest decides.
_Avoid_: subdomain rule, exact-host rule

**Host**:
The hostname of the front browser tab: the only part of its URL Nightcap ever sees.

**Tab domain**:
What a domain rule made from the front tab names: the tab's host, short of a leading `www.`, such as `figma.com` for `www.figma.com` and `docs.google.com` for itself.
_Avoid_: registrable domain

**Website checking**:
Whether Nightcap reads the front browser's host, which domain rules need. Off until the user turns it on, unless the config sets it.
_Avoid_: website monitoring, site tracking

**Watch**:
Nightcap reading one browser's host while it's in front, which website checking allows unless the browser's own place says off and no site in it could say on.

**Browser list**:
The browsers Nightcap knows how to watch.

**Rule identity**:
A rule's kind plus its target, e.g. `app:com.figma.Desktop`. Rules with the same identity are the same rule, so an app or domain has one rule at most, whatever it says.

**Config rule**:
A rule declared in the config file, which the user or their installer writes. Read-only at runtime.
_Avoid_: Nix rule, static rule

**Ad-hoc rule**:
A rule added at runtime and kept by Nightcap until it graduates.
_Avoid_: runtime rule, dynamic rule

**Tour rule**:
A rule the user adds while the tour runs, held in memory only until it ends, and never listed, saved or exported.

**Graduation**:
An ad-hoc rule's identity appearing in the config, whatever either says, at which point Nightcap drops its own copy.
_Avoid_: promotion, pruning

**Export**:
The ad-hoc rules not yet in the config, written out ready to paste into it.

**Reload**:
Reading the config again, which is the only way a change to it takes effect.

**Snooze**:
A timer or hold on a rule's place that says the other way, for a time or until the schedule next turns it back. The rule waits underneath and comes back when it ends.
_Avoid_: pause, override

### Deciding

**Force**:
Night Shift held other than macOS would have it, by a rule, a hold or a timer. Nightcap releases every force on a clean exit (ADR-0011).

**Decision**:
What Night Shift should be for the current context, the place that decides it, and the layers it rests on.

**Reason**:
One layer behind a decision: a rule, a hold, a timer, the schedule or the tour.

**Wanted state**:
What Night Shift should be right now: what the deciding place says.
_Avoid_: desired state

### Talking to Nightcap

**Daemon**:
The running Nightcap app, as the CLI sees it: the only writer of Night Shift, reached over its socket.
_Avoid_: server, service

**Command**:
A request to change something, such as the switch, a timer on a place or adding a rule. The menu, Settings and the CLI send the same commands.

**Status**:
The daemon's report of its decision and everything behind it: the context, rules, each place's layers, grants and problems.
