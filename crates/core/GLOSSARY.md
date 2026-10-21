# Control

How Nightcap decides whether Night Shift is forced off or left alone, from rules, the schedule and the user's own toggles.

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

### Toggles

**Manual state**:
The on/off the user set last, through Nightcap or by an adopted external change. It lasts until the next schedule transition. Short of overriding a force, an on/off that a switching schedule gives anyway is none, so turning Night Shift off and back on follows the schedule again.

**External change**:
A Night Shift toggle made outside Nightcap, in Control Center or System Settings.

**Adoption**:
Taking an external change as the command it amounts to, so it sets the manual state, or overrides, as that command would.

**Override**:
Turning Night Shift on while a force holds it off, from Nightcap or from outside. It snoozes the forcing rules and cancels the timed disable.

**Resume**:
Clearing the timed disable, every snooze and the manual state at once.

**Tour stand-in**:
A tour started with Night Shift off has it on for the tour's length, in place of the schedule. The manual state and timed disable wait aside until the tour ends, which puts them back unless the user chose others meanwhile (ADR-0022).

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

**Tab domain**:
What a domain rule made from the front tab names: the tab's host, short of a leading `www.`, such as `figma.com` for `www.figma.com` and `docs.google.com` for itself.
_Avoid_: registrable domain

**Website checking**:
Whether Nightcap reads the front browser's host, which domain rules need. Off until the user turns it on, unless the config sets it.
_Avoid_: website monitoring, site tracking

**Watch**:
Nightcap reading one browser's host while it's in front, which website checking allows unless that browser's own app rule forces it off.

**Browser list**:
The browsers Nightcap knows how to watch.

**Rule identity**:
A rule's kind plus its target, e.g. `app:com.figma.Desktop`. Rules with the same identity are the same rule.

**Config rule**:
A rule declared in the config file, which the user or their installer writes. Read-only at runtime.
_Avoid_: Nix rule, static rule

**Ad-hoc rule**:
A rule added at runtime and kept by Nightcap until it graduates.
_Avoid_: runtime rule, dynamic rule

**Tour rule**:
A rule the user adds while the tour runs, held in memory only until it ends, and never listed, saved or exported.

**Graduation**:
An ad-hoc rule's identity appearing in the config, at which point Nightcap drops its own copy.
_Avoid_: promotion, pruning

**Export**:
The ad-hoc rules not yet in the config, written out ready to paste into it.

**Reload**:
Reading the config again, which is the only way a change to it takes effect.

### Forcing

**Force**:
Night Shift held off by a rule or a timed disable.

**Hands off**:
No force applies, so Night Shift follows the manual state or the scheduled state.

**Snooze**:
A rule muted until the next schedule transition.
_Avoid_: pause

**Timed disable**:
Night Shift forced off until a deadline.
_Avoid_: pause

**Decision**:
Whether Night Shift is forced off or hands off for the current context, with its reasons.

**Reason**:
One thing behind a decision: a rule, a timed disable, a snooze, the manual state or the schedule.

**Wanted state**:
What Night Shift should be right now: off under a force, else the manual state, else the scheduled state.
_Avoid_: desired state

### Talking to Nightcap

**Daemon**:
The running Nightcap app, as the CLI sees it: the only writer of Night Shift, reached over its socket.
_Avoid_: server, service

**Command**:
A request to change something, such as turning Night Shift on, a timed disable or adding a rule. The menu, Settings and the CLI send the same commands.

**Status**:
The daemon's report of its decision and everything behind it: the context, rules, snoozes, grants and problems.
