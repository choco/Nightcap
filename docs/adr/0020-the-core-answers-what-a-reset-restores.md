# The core answers what a reset restores

ADR-0018 left the menu to infer command consequences from the rules and inheritance in each place's query. Reset then required Swift to know which rules it removes, and could change what a place inherits when it removes Adoption's linked rule. That knowledge belongs with the command.

## Decision

This supersedes only ADR-0018's choice that the core forecasts nothing and the app deduces what a reset leaves. Its rules and commands are unchanged.

Each place in `front()` includes `reset_to`: the place's own indefinite rule, a less specific place's say, the stand-in or the schedule that resetting it would restore at the snapshot's time. It is absent while none would say anything. This is the state at that place, not necessarily what decides the screen: a more specific place can still decide.

The core computes it with the same rule removal that Reset uses, on a copy, including Adoption's link. Asking changes nothing, performs no effects and logs no reset. It neither advances time nor predicts changes in context. Swift chooses which actions to offer and words their titles; its reset titles follow this result rather than reimplementing rule removal and inheritance.

## Rejected alternatives

- **Keep inferring reset in Swift:** duplicates command knowledge and misses changes to inheritance when a linked rule is removed.
- **Only return on or off:** the menu still has to reconstruct what supplies that state to name the action.
- **A general command-preview interface:** requires modeling refusals, writes and every command's consequences; the current caller needs only reset.
- **Return menu items from Rust:** presentation and wording remain the App context's.
