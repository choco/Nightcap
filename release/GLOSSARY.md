# Release

How a signed Nightcap.app gets from a tagged commit to a published release that keeps its grants across updates.

## Language

**Certificate**:
The Developer ID Application certificate Apple issues to the project's team, which every release and dev build is signed with. A renewed one keeps every grant.

**Signing requirement**:
The rule macOS uses to recognise the app across updates: the bundle ID plus the team its certificate was issued to. Every grant is tied to it.

**Release gate**:
The checks a signed app must pass before it's published: it satisfies the signing requirement exactly, Gatekeeper accepts it as notarized, it links only allowed libraries, and it can accept updates.
_Avoid_: verification gate

**Release**:
A tagged build of Nightcap.app, signed, notarized, passed through the release gate, and published as a zip and a DMG with their checksums and the appcast.

**Installer**:
What puts a release on a Mac, such as the Nix package or the brew cask. A managed install's installer also starts and updates it.

**Managed install**:
An install whose bundle the user can't write, such as one in the Nix store. Its installer owns updates and starting at login.

**Self-managed install**:
An install that updates itself, and opens at login if the user chooses, such as one from brew or a manual drag-install.

**Appcast**:
The feed published with every release that tells self-managed installs which release is newest.

**Dev build**:
A locally signed build under its own bundle ID, which runs beside the installed app without sharing its grants or state.
