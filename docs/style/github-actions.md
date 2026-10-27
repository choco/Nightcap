# GitHub Actions

- **Pinning:** actions are pinned to a full commit SHA, with the version as a comment. Pin a release that has been out at least two weeks.
- **Permissions:** the top-level `permissions:` is the least the workflow needs, and a job raises it only for itself.
- **Secrets** live in an environment that only the jobs needing them deploy to.
- **Logic lives in the repository's scripts,** and jobs call them by path.
- **Gating:** to make one workflow wait on another's checks, call the other as a reusable workflow (`workflow_call`) and list it under `needs:`.
- **Path filters:**
  - `paths`, with negated patterns, decides which pushes run a workflow, and a later pattern overrides an earlier one.
  - `paths` and `paths-ignore` can't both be set on one event.
- **actionlint** runs clean, with shellcheck on the `run:` steps. Custom runner labels are declared in `.github/actionlint.yaml`.
