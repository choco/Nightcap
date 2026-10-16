# CI builds, signs and publishes; installers use the signed release unchanged

Permission grants survive an update only if the new build satisfies the same signing requirement, so every build that reaches a Mac must be signed with the same certificate (ADR-0004). The shell needs current Xcode and Swift. Nightcap is installed by package managers whose packaging lives outside this repo, such as nix-darwin and home-manager in the author's nix-config. The repo is public, so a release downloads without credentials.

## Decision

In GitHub Actions:

1. **Every push:** a Linux job runs fmt, clippy and the core's tests, and a macOS job builds the core and the shell unsigned.
2. **Every `v*` tag:** a job on the `xcode-27` runner builds the core with cargo and the shell with Xcode, then assembles the bundle. The `macos-26` runner tops out at the macOS 26 SDK, below the app's 27.0 deployment target.
3. **Versions** are set before signing. `CFBundleShortVersionString` comes from the tag, which must match the Cargo version. `CFBundleVersion` is the commit count.
4. **Signing:** the job imports the certificate from an Actions secret into a temporary keychain and signs inside-out with `/usr/bin/codesign`, called by absolute path.
5. **Verification gate:** the release fails unless the signed app satisfies the signing requirement committed in the repo, and its requirement string matches that one exactly.
6. **Packaging:** the app is zipped in a way that preserves symlinks, with no extended attributes.
7. **Publishing:** a Linux job publishes the release with `Nightcap-<version>.zip`, a `SHA256SUMS` asset and the appcast (ADR-0015).

This repo ships no installer. `docs/installing.md` is the contract every installer relies on. Its hard rule: install the bundle byte-for-byte unchanged. A strip or re-sign replaces the signature and loses every grant; nixpkgs' fixup phase does exactly that unless `dontFixup` is set.

Why this wins:

- The private key never has to reach a keychain on the Macs, e.g. during `sudo darwin-rebuild switch`.
- A new Mac needs neither the certificate nor any credentials.
- The shell can use current Xcode and Swift.
- Installers stay thin: fetch, check the hash, copy.
- It is how Ghostty and Glide ship. The verification gate exists because Glide once shipped unsigned releases through a broken shim.

This rests on grants surviving an update to a new path signed with the same certificate, which holds on macOS 27: a store-path app started by launchd, and a self-managed install updated by Sparkle, both kept Accessibility and notifications.

## Consequences

- **The key is exposed in CI.** The certificate's private key effectively *is* the Accessibility grant, and it lives in GitHub secrets. Signing happens in an isolated job with actions pinned to commit SHAs. The secret lives in a `release` environment that only `v*` tags can deploy to, so no other workflow or branch can read it.
- **Every change you run day-to-day goes through a release.** Rolling back means installing the previous release; every release stays published.

## Rejected alternatives

- **Nix builds from source with nixpkgs' Swift, and activation signs**: every Mac needs the certificate in its keychain; `codesign` would have to reach the keychain during `sudo darwin-rebuild switch`; nixpkgs' Swift 6.2.4 limits the shell to the 26 SDK and rules out SwiftUI macros. Ghostty's own flake skips macOS for the same reasons.
- **Both paths**: two signing paths, with the key in both places.
- **Nix packaging in this repo's flake**: it ties every release to one installer. The flake is a development shell only.
- **The app registers itself through `SMAppService` in every install**: an imperative step a managed installer can't see. Only self-managed installs register, and only when the user chooses to (ADR-0015).
- **A `service install` command that writes its own plist**: it bakes store paths into the plist, the pitfall seen in yabai, skhd, rift and espanso.
