# Config rules plus ad-hoc rules that graduate into the config

Rules are declared in a config that home-manager generates: a read-only Nix store symlink that changes only on activation. New rules come up while working, though, and a rebuild for each one is too much friction. Rules added at runtime need a home that survives reboots, and a path into the config.

## Decision

**Where rules live:**

- **Config rules** come from `$XDG_CONFIG_HOME/nightcap/config.toml` and are read-only.
- **Ad-hoc rules** live in `$XDG_DATA_HOME/nightcap/rules.toml`. They use the same schema as the config, and only the core writes that file.

**Graduation:**

- `rules export --format nix|toml` prints the ad-hoc rules that aren't in the config yet, ready to paste.
- On reload, an ad-hoc rule whose identity now appears in the config is deleted from the ad-hoc file, and the deletion is logged.
- `rules list` and `status` show each rule's source.

Config rules can only be snoozed at runtime. The menu's rule toggles work like this:

| Current state | Toggle for this app or site |
|---|---|
| No rule matches | Adds an ad-hoc rule. For a site, it uses the tab's registrable domain. |
| An ad-hoc rule matches | Removes it. For a site, it removes the most specific one. |
| A config rule matches | Snoozes it until the next schedule transition. The menu shows that it comes from the Nix config. |
| A snoozed rule matches | Un-snoozes it. |

Removing a config rule from the CLI fails, with a pointer to snoozing.

This combines three established patterns: a writable layer over a read-only one that shows where each value came from (Emacs `custom-file`, `git config --show-origin`); export as config (Grafana's "copy JSON"); and dropping the runtime copy once the config owns it (home-manager's dconf module).

## Consequences

- A rule that has graduated and is later removed from the config doesn't come back.
- Editing `rules.toml` by hand isn't supported.

## Rejected alternatives

- **No ad-hoc rules**: every new rule would need a rebuild.
- **Session-only ad-hoc rules**: forgotten at every reboot.
- **The ad-hoc file inside the nix-config checkout**: the daemon would write into the repo, the path differs per machine, and the file stays a permanent second source of truth.
- **Re-applying the config at activation and reverting runtime changes**: the daemon would undo the user's own clicks.
- **Menu toggles that switch a config rule off persistently**: the ad-hoc layer would quietly override the config.
- **An explicit `rules prune` step**: an extra command for no benefit.
