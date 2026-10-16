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
The window that takes a new user through each choice and grant, once per onboarding version.
_Avoid_: setup wizard, first run

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
A problem the user can't accept, only fix: an invalid config or ad-hoc rules file, or Night Shift unavailable.

**Fix**:
An action offered with a problem: a restore, an accept, or reloading a broken file.

**Restore**:
A fix that puts back what the user set up, such as registering the login item again.

**Accept**:
A fix that takes the change as the user's new choice, such as keeping notifications off.
_Avoid_: dismiss, ignore

**Announce**:
Post one notification when a problem appears, unless the user caused it.

### Menu

**Status line**:
The menu's first line: whether Night Shift is on, off or forced off, with the main reason under it.

**Problem line**:
The menu's line while any problem exists, counting them; it opens Settings where the problem is.

**Rule toggle**:
A menu item for the front app or site that adds, removes, snoozes or unsnoozes its rule.
