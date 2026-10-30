# Nightcap writes Night Shift's strength and schedule only on the user's command

Night Shift's strength and schedule belong to macOS: System Settings shows them and keeps them, and the core only reads the schedule, to compute schedule transitions (ADR-0004). But many people who hear about Nightcap don't know what Night Shift is, so onboarding sets it up with them, how warm it gets and when it comes on, before it says what Nightcap adds. The menu's strength slider sets the strength too. `CBBlueLightClient` writes both: `setStrength:commit:`, `setMode:` and `setSchedule:` all work on macOS 27.0.1 and on macOS 15, and macOS applies an edited schedule at once.

## Decision

- The system still owns the strength and the schedule. They live in System Settings, which shows them, and the user can change them there at any time.
- Nightcap writes them only when the user asks it to: onboarding's Night Shift steps, the menu's strength slider, a command such as `nightcap strength`, or the config. Nothing writes them on its own: no rule, schedule transition or check, and Nightcap never puts back a value the user changed elsewhere.
- Onboarding's Night Shift steps ask only what the config leaves out and the Mac can do: the strength unless the config sets it or it can't be read, the schedule unless the config sets it or Night Shift is unavailable. The app decides which, from the answers to `config()` and `night_shift()`, as it decides whether onboarding opens, which it does while something is left to ask: a config that answers both steps and sets website checking still leaves the apps to ask unless it has a rule of its own, and a self-managed install still asks about opening at login (ADR-0011).
- The config can set either, under `[night_shift]`. A value from the config takes effect when it changes: at launch and on reload, Nightcap compares each with the value it last applied from the config, kept in `state.json`, and writes only those that differ. A change made since, in System Settings or Nightcap's menu, stands until the config's value changes again. A value the config leaves out stays whatever macOS has.
- Nightcap keeps no copy of what macOS has, only of what it last applied from the config. It reads both values back from macOS: the schedule from the status callback, and the strength by asking, since a strength change triggers no callback.
- A schedule Nightcap writes arrives as an edited schedule, which ends every rule waiting on the schedule's next turn (ADR-0018), as an edit in System Settings does.
- The core sends the writes as effects, `SetSchedule { schedule }` and `SetStrength { value }`, answering the commands `SetSchedule(schedule)` and `Strength(value)`.

## Consequences

- Three more private methods, which every macOS version Nightcap claims has to be checked for (ADR-0013). The adapter checks their signatures like the others. Where they're missing, onboarding's Night Shift steps open Night Shift's settings in System Settings instead, and the slider doesn't show.
- Writing a schedule can switch Night Shift on or off at once, since macOS applies the new schedule straight away: choosing "By hand" in onboarding, or `schedule = "off"` in the config, while Night Shift is on lets it go off with the schedule, and Nightcap doesn't switch it back.

## Rejected alternatives

- **Sending the user to System Settings to set Night Shift up**: the people onboarding is for don't know where it is or what its slider does, and the step would end in another app's window.
- **Nightcap owning the schedule and strength, with its own copy of each**: two schedules would disagree, Control Center would still follow macOS's, and the core relies on macOS performing schedule transitions (ADR-0004).
- **Putting back what onboarding or the config set when it changes**: Nightcap would fight the user's own changes in System Settings, as reverting outside changes would (ADR-0018).
- **Applying the config's values at every launch or reload**: every login, and every installer run of `nightcap reload`, would undo a change the user made in System Settings.
- **Locking them while the config sets them, as the config locks `websites`**: Nightcap can't lock System Settings, so the lock would cover only Nightcap's own controls, and the two would disagree.
