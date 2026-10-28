# Control

How Nightcap decides whether Night Shift is on or off, from rules, the schedule and the user's own changes.

## Language

### Schedule

**Schedule**:
Night Shift's schedule in System Settings: off, sunset to sunrise, or custom hours. The system owns it; Nightcap changes it only when the user asks.

**Sun times**:
The sunrises and sunsets macOS works out from the Mac's location, which sunset to sunrise follows. macOS can take a minute or two to have them after login, and has none while Location Services is off.
_Avoid_: sun schedule

**Set aside**:
What macOS does to sunset to sunrise while Location Services is off: it turns the schedule off, and back on once Location Services is.

**Scheduled state**:
Whether the schedule says Night Shift should be on right now.

**Schedule transition**:
A moment when the schedule starts or ends.
_Avoid_: boundary

**Strength**:
How warm Night Shift tints the display. The system owns it; Nightcap changes it only when the user asks.
_Avoid_: temperature, warmth

### Changes

**Rule**:
A change to Night Shift: a place, what it says there, a length and who set it. At one place, the shortest rule decides.
_Avoid_: layer

**Place**:
Where a rule applies: everywhere, the app in front, or the site in front. A site is a domain, covering its subdomains: the most specific one with a rule that covers the tab, else the tab's own domain.

**Place in front**:
A place that applies now: the site in front, the front app and everywhere, most specific first. Each gives its own say, else the one it inherits from the first place after it that says something, else Night Shift as last reported. What's in front answers how each stands: what it gives, its rules and what they sit on, and what it inherits and from where.

**Deciding place**:
The most specific place in front that says something: the site, then the app, then everywhere, which always does once the schedule is known. What it says is what Night Shift does.

**Length**:
How long a rule lasts: indefinitely, until the schedule next turns it back, or until a time, picked or a length from now. A place has one rule of each length at most.
_Avoid_: duration

**Temporary rule**:
A rule until the schedule next turns it back or until a time, which gives back what was under it when it ends.
_Avoid_: timer, hold, timed disable, manual state

**Set by**:
Who set a rule: the config, the user, the user during a trial, or an outside change Nightcap adopted.

**Snooze**:
A temporary rule over a longer one at its place that says the other way. The longer one waits underneath and comes back when it ends. The rules query lists each rule with its snooze: its place's shortest rule, where that is one.
_Avoid_: pause, override

**Switch**:
Everywhere's on and off: the Night Shift item, the shortcut's tap, `nightcap on`, `off` and `toggle`. It takes off everywhere's rules that say otherwise, shortest first, and if the schedule still says otherwise, sets an indefinite one.
_Avoid_: toggle, as a noun

**Reset**:
Taking off a place's temporary rules, which puts back its indefinite rule, and for everywhere its indefinite one too, which puts back the schedule.

**Outside change**:
Night Shift switched outside Nightcap, in Control Center or System Settings.
_Avoid_: external change

**Adoption**:
Taking an outside change as flipping what the user sees: it takes off what the deciding place says, and sets the other state over an indefinite rule or the schedule until the schedule next turns it back.

**Trial**:
A stretch the app begins and ends, during which every change lives in memory only: Nightcap keeps what it found, which `state.json` and `rules.toml` go on holding, and its end puts that back, short of what would have ended meanwhile. The CLI begins none (ADR-0016).

**Stand-in**:
Night Shift on in place of the schedule, for a trial's length, when the app asks for it as the trial begins: everywhere's rules wait aside, and the user's own changes still come first.

### Rules

**Context**:
The frontmost app and the host: what rules match against.

**App rule**:
A rule at an app, which applies while the app is frontmost.

**Domain rule**:
A rule at a site: a domain and all its subdomains. Where several cover a host, the longest decides.
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
Nightcap reading one browser's host while it's in front, which website checking allows unless the browser's own place says off and no site in it could say on. Nothing is watched once Nightcap has released its forces on the way out.

**Browser list**:
The browsers Nightcap knows how to watch.

**Rule identity**:
An app's or a domain's indefinite rule's kind plus its target, e.g. `app:com.figma.Desktop`. Indefinite rules with the same identity are the same rule, so an app or domain has one at most, whatever it says.

**Config rule**:
An indefinite rule at an app or a site, declared in the config file, which the user or their installer writes. Read-only at runtime, short of a snooze.
_Avoid_: Nix rule, static rule

**Ad-hoc rule**:
An indefinite rule at an app or a site the user added at runtime, kept by Nightcap until it graduates.
_Avoid_: runtime rule, dynamic rule

**Trial rule**:
An indefinite rule at an app or a site the user adds during a trial, kept in memory only until it ends: the rules query and `nightcap status` show it, but Settings, the saved state and exports never do.

**Graduation**:
An ad-hoc rule's identity appearing in the config, whatever either says, at which point Nightcap drops its own copy.
_Avoid_: promotion, pruning

**Export**:
The ad-hoc rules not yet in the config, written out ready to paste into it.

**Reload**:
Reading the config again, which is the only way a change to it takes effect.

### Deciding

**Force**:
Night Shift held other than macOS would have it, by a rule. Nightcap releases every force on a clean exit (ADR-0008).

**Decision**:
What Night Shift should be for the current context, the place that decides it, and what it rests on: that place's rules, shortest first, then for everywhere the stand-in or the schedule.

**Reason**:
One thing behind a decision: a rule, the stand-in or the schedule.

**Wanted state**:
What Night Shift should be right now: what the deciding place says.
_Avoid_: desired state

### Talking to Nightcap

**Server**:
Nightcap's core as it runs, with the app and the CLI as its clients: the only writer of Night Shift, which decides what macOS reports and what its clients send one at a time, in the order it arrives. The CLI reaches it over its socket.
_Avoid_: daemon, service

**Command**:
A request for Nightcap to do something, such as the switch, a temporary rule at a place, adding a rule, a reload, an update check or keeping notifications off, which Nightcap may refuse. The app and the CLI send the same commands, a fix's among them where the app doesn't carry it out in macOS itself; beginning and ending a trial and finishing onboarding are the app's alone.

**Refusal**:
Why Nightcap refused a command, which then changes nothing: what the client needs to say why, such as the config rule that decides, which it words itself. The app tells it in a notification, for a fix's command too, short of a reload's, which the config file's check shows already.

**Snapshot**:
Nightcap as it stood after one input, which a client reads from whole: every query asked of it answers as of that input, whatever Nightcap decides after.

**Query**:
A question a client asks Nightcap, answered with facts, never words, and changing nothing: Night Shift and when its schedule next turns each way, what's in front, how each place in front stands and what decides there, every rule with its snooze, the config, the user's preferences, the checks, the trial and the install. A read asks any of them of one snapshot; `nightcap status` asks most of them.
