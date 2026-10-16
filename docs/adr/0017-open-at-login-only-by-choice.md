# Self-managed installs open at login only when the user chooses to

ADR-0015 had a self-managed install register itself to start at login with `SMAppService.mainApp` on its first launch. Since then Nightcap gained onboarding (ADR-0016), which asks before turning on anything the user might not want, and a settings window. Starting at every login is that kind of choice: macOS announces a newly added login item with its own notification, which arrives with no context when nobody asked for it.

This replaces ADR-0015's login-item registration; the rest of ADR-0015 stands.

## Decision

- In a self-managed install, onboarding asks whether Nightcap should open at login. Nothing registers the login item otherwise.
- Nightcap records the answer. Settings shows an "Open at login" switch that mirrors the login item's state as macOS reports it; flipping it registers or unregisters the item, as often as the user likes, and records the new answer.
- When the user chose to open at login and macOS stops opening Nightcap, because the item was removed in System Settings or is waiting for approval, the "Open at login" check is a problem. The user either restores it (Login Items, or registering again) or keeps it off, which records that as their answer.
- Managed installs leave launching to their installer, and dev builds never register, as before.

## Consequences

- A user who skips onboarding's question doesn't get Nightcap at login until they turn the switch on.
- A change made in System Settings → Login Items shows in the switch at once, and is pointed out until the user restores it or accepts it.

## Rejected alternatives

- **Registering on the first launch** (ADR-0015): the user never chose it, and macOS's notification about the new login item arrives without explanation.
- **macOS's state as the only record of the choice**: a removal in System Settings, or by a cleanup tool, would silently stop Nightcap opening at login, and nobody would be told.
