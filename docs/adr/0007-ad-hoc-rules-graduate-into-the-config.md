# Config rules, plus ad-hoc rules that graduate into the config

Rules are declared in a config that home-manager generates: a read-only Nix store symlink that changes only on activation. New rules come up while working, though, and a rebuild for each one is too much friction. Rules added at runtime need a home that survives reboots, and a path into the config.

## Decision

**Where rules live:**

- **Config rules** come from `$XDG_CONFIG_HOME/nightcap/config.toml`, under `[rules.off]` and `[rules.on]` (ADR-0006), and are read-only.
- **Ad-hoc rules** live in `$XDG_DATA_HOME/nightcap/rules.toml`. They use the same schema as the config, and only the core writes that file.

**Graduation:**

- `rules export --format nix|toml` prints the ad-hoc rules that aren't in the config yet, ready to paste.
- On reload, an ad-hoc rule whose identity now appears in the config is deleted from the ad-hoc file, whatever either says, and the deletion is logged.
- `rules list` and `status` show each rule's source.

**Changing rules at runtime:**

- An app's or site's row in the menu makes or changes the user's rule with Always, removes it with Remove Your Rule, and snoozes any rule, config rules included, with its lengths (ADR-0018). Settings offers Remove on the user's rules and Put Back on a snoozed one. A rule made from the front tab names its host (ADR-0017).
- Config rules can only be snoozed at runtime: replacing or removing one from the menu, Settings or the CLI fails, pointing at the config.

This combines three established patterns: a writable layer over a read-only one that shows where each value came from (Emacs `custom-file`, `git config --show-origin`); export as config (Grafana's "copy JSON"); and dropping the runtime copy once the config owns it (home-manager's dconf module).

## Consequences

- A rule that has graduated and is later removed from the config doesn't come back.
- Editing `rules.toml` by hand isn't supported.

## Rejected alternatives

- **No ad-hoc rules**: every new rule would need a rebuild.
- **Session-only ad-hoc rules**: forgotten at every reboot.
- **The ad-hoc file inside the nix-config checkout**: the daemon would write into the repo, the path differs per machine, and the file stays a permanent second source of truth.
- **Re-applying the config at activation and reverting runtime changes**: the daemon would undo the user's own clicks.
- **Changing or switching off a config rule for good from the menu**: the ad-hoc layer would quietly override the config.
- **An explicit `rules prune` step**: an extra command for no benefit.
