# Release

How a signed Nightcap.app gets from a tagged commit to a published release that keeps its permission grants across updates.

## Language

**Signing requirement**:
The rule macOS uses to recognise the app across updates: the bundle ID plus the hash of the signing certificate. Every permission grant is tied to it.

**Release**:
A tagged build of Nightcap.app, signed, verified against the signing requirement, and published as a zip.

**Managed install**:
An install whose bundle the user can't write, such as one in the Nix store. Its installer owns updates and starting at login.

**Self-managed install**:
An install that updates itself and starts itself at login, such as one from brew or a manual drag-install.

**Appcast**:
The feed published with every release that tells self-managed installs which release is newest.

**Dev build**:
A locally signed build under its own bundle ID, which runs beside the installed app without sharing its grants or state.
