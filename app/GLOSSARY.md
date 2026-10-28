# App

What the user sees and sets up: the menu, Settings and onboarding, and the checks that compare what the user set up with what macOS is doing.

## Language

### Setup

**Choice**:
Something the user decided about how Nightcap runs, such as website checking, notifications or opening at login. Nightcap records it, so a check can tell a change the user made on purpose from one that broke what they set up.
_Avoid_: wish, preference

**Grant**:
A permission the user gave Nightcap in System Settings: Accessibility or notifications.

**Open at login**:
The choice that macOS starts Nightcap at login, which a self-managed install carries out through its login item.
_Avoid_: launch at login

**Onboarding**:
The window that sets up Night Shift with the user, then takes them through each choice, grant and suggestion, once per onboarding version. The app keeps the version the user went through, and opens it at launch unless the config and the install answer all it would ask.
_Avoid_: setup wizard, first run

**Tour**:
An optional walk-through in a floating panel: the moon, a rule the user makes and sees act, then snoozes and puts back, for an app and where it can for a site, and the peek. Offered by “Nightcap is up here” at the end of onboarding, once the moon is pointed out, and in Settings; every step can be skipped. It runs in a trial, which it begins as the user continues from its first step, with the stand-in when it finds Night Shift off, so what the user changes during it lasts only while it runs.
_Avoid_: tutorial, walkthrough

**Tour sample**:
The tour's own app, Tour Sample, which the user adds a rule for from the menu, a trial rule, to see the rule act. The tour uses it even when one of the user's apps with a rule is running.

**Suggestion**:
An app or website onboarding offers a rule for, already ticked if it has one: installed apps where colour matters, and websites that need true colour, mostly video streaming.

### Checks

**Check**:
One thing Nightcap depends on, such as a grant or a valid config file, compared with what the user set up.
_Avoid_: doctor, health check

**Problem**:
A check that needs the user: drift or broken. The menu calls it needing attention.
_Avoid_: issue, error

**Drift**:
A problem where something the user set up stopped working, such as notifications switched off after being allowed. It lasts until the user restores it or accepts the change.

**Broken**:
A problem the user can't accept, only fix: an invalid config or ad-hoc rules file, Night Shift unavailable, or sunset to sunrise without sun times for ten minutes.

**Fix**:
An action offered with a problem: a restore, an accept, or reloading a broken file. The app picks which a check offers. Most are commands; the rest happen in macOS itself, asking for a grant or opening the pane of System Settings where the user puts something back, and the app carries them out alone.

**Restore**:
A fix that puts back what the user set up, such as registering the login item again, or opening Login Items for macOS's approval.

**Accept**:
A fix that takes the change as the user's new choice, such as keeping notifications off. Each is a command, which records the choice; keeping notifications off and keeping the schedule off are refused while their check doesn't drift.
_Avoid_: dismiss, ignore

**Announce**:
Post one notification when a problem appears on its own, as the snapshot after a fact shows it, never one the user caused with a command. The app remembers what it told until the problem ends, across relaunches, and tells one that comes back again.

### Menu

**Moon**:
Nightcap's menu-bar icon: filled while Night Shift is on, an outline while it's off, and an outline with a “z”, forced off, while an app or site keeps it off with everywhere on. Dimmed while Night Shift is unavailable.

**Night Shift item**:
The menu's first item, after any problem line: Night Shift, checked while it's on everywhere, in the moon's look, with what decides the screen under it, such as “Off in Figma: your config’s rule”, “Following the schedule until 07:00” or “No schedule”. Choosing it, or tapping the shortcut, is the switch.
_Avoid_: status line

**Problem line**:
The menu's line while any problem exists, counting them; it opens Settings where the problem is.

**Place row**:
A menu item with a submenu for one place: everywhere's, Turn On or Turn Off, under the Night Shift item, then the front app's and the front site's, which is titled for “This Site” and names the site on the line under it. Each turns its place the other way for each of the lengths, until a time picked with Custom… in a small window of its own, and until the schedule next turns it back, shown as the time, “Until 07:00”, with the moment under it: “At sunrise”, “At sunset”, “When the schedule ends” or “When the schedule starts”, and left out without a schedule. As they apply, an app's or site's row offers Always, its indefinite rule, greyed under a config rule with “Your config decides”; Remove Your Rule; Put the Rule Back; and Like Everywhere Else, or Like the Rest of Safari, say, for a site whose browser has a say of its own. Everywhere's offers Back to the Schedule while a rule keeps it from what the schedule says, and nothing the switch would do anyway: the switch takes off everywhere's rules that say otherwise, shortest first, and keeps the first that says its way, so Back to the Schedule shows only while one of them says the row's way, the lengths not over a rule until a time with the row's way under it, and the schedule's turn only while the schedule or the stand-in says the other way. An app's or site's row with nothing to offer is greyed with what its place says; everywhere's is left out.
_Avoid_: rule toggle

**Site icon**:
A site's own icon, which its row in the menu, its rule in Settings and its suggestion in onboarding show once Nightcap has asked the site for it (ADR-0019), and the globe until then.
_Avoid_: favicon

### Shortcut

**Shortcut**:
Nightcap's one global keyboard shortcut, ⌃⌥⌘N unless the user records another in Settings. A tap is the switch, as the Night Shift item is; a hold is a peek.
_Avoid_: hotkey

**Peek**:
True colour while the shortcut is held, with Night Shift still on. It drops the tint and nothing else, so the core never learns of it.
_Avoid_: preview, which is the strength's while its slider moves

### Between macOS and the core

**Reporter**:
The app's part that tells the core what macOS observes, such as the front app, Night Shift's status or a grant, each as it changes.
_Avoid_: host, observer

**Executor**:
The app's part that carries out what the core decides, in the order it decided it: first what macOS must do, then what happened, for the menu, Settings and notifications to show. The fixes the app carries out alone go through it too.
_Avoid_: host, dispatcher
