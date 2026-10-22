# CI signs with a Developer ID, notarizes and publishes; installers use the signed release unchanged

macOS ties every permission grant to the app's signing requirement, so a grant survives an update only if the new build satisfies the same one. Gatekeeper opens a downloaded app without warnings only if it's signed with a Developer ID and notarized; otherwise the first launch waits for "Open Anyway" in System Settings, a DMG won't mount, and Homebrew's official cask tap refuses the app. The shell needs current Xcode and Swift. Nightcap is installed by package managers whose packaging lives outside this repo, such as nix-darwin and home-manager in the author's nix-config, and by hand from the project's site. The repo is public, so a release downloads without credentials, and the project has an Apple Developer Program membership.

## Decision

In GitHub Actions:

1. **Every push:** a Linux job runs fmt, clippy and the core's tests, a macOS job builds the core and the shell unsigned, and another job type-checks and builds the site.
2. **Every `v*` tag:** a job on the `xcode-27` runner builds the core with cargo and the shell with Xcode 27 against the macOS 27 SDK, then assembles the bundle. The `macos-26` runner tops out at the macOS 26 SDK, and the app uses macOS 27's APIs where they exist (ADR-0013).
3. **Versions** are set before signing. `CFBundleShortVersionString` comes from the tag, which must match the Cargo version. `CFBundleVersion` is the commit count.
4. **Signing:** the job imports the team's Developer ID Application certificate from an Actions secret into a temporary keychain, and `release/sign` signs inside-out with `/usr/bin/codesign`, called by absolute path, with a secure timestamp. It picks the certificate by the team that the signing requirement names. Dev builds are signed with the same certificate, under their own bundle ID, `io.github.choco.nightcap.dev`, with their own socket and state directories, so they never touch the installed app.
5. **The signing requirement** is the Developer ID designated requirement, committed in the repo: the bundle ID, Apple's Developer ID anchor and the team ID, rather than one certificate. A renewed certificate keeps every grant.
6. **Notarization:** CI submits the signed app with `notarytool`, signing in with an App Store Connect team key with the Developer role, kept in the `release` environment, and staples the ticket to the app before packaging, so the zip and the DMG carry the same notarized app. The DMG stays unsigned: Gatekeeper mounts an unsigned image without asking and checks the app inside when it opens, so one submission covers both downloads.
7. **Release gate:** the release fails unless the signed app satisfies the committed requirement, its requirement string matches that one exactly, it carries a valid stapled ticket, and Gatekeeper's verdict is "Notarized Developer ID". Library validation is on: Sparkle is signed by the same team, so the app needs no `com.apple.security.cs.disable-library-validation`, and a framework swapped inside a writable bundle doesn't load.
8. **Packaging:** the app is zipped in a way that preserves symlinks, with no extended attributes. For first installs from the site, the same app also goes into a DMG to drag into Applications.
9. **Publishing:** a Linux job publishes the release with `Nightcap-<version>.zip`, `Nightcap-<version>.dmg`, a `SHA256SUMS` asset and the appcast (ADR-0011).
10. **Changing certificates** is an ordinary Sparkle update: Sparkle accepts a new code signature when the EdDSA key stays the same, so the two never change in the same release.

This repo ships no installer. `docs/installing.md` is the contract every installer relies on. Its hard rule: install the bundle byte-for-byte unchanged. A strip or re-sign replaces the signature and loses every grant; nixpkgs' fixup phase does exactly that unless `dontFixup` is set.

Why this wins:

- The private keys never have to reach a keychain on the Macs, e.g. during `sudo darwin-rebuild switch`.
- A new Mac needs neither a certificate nor any credentials.
- The shell can use current Xcode and Swift.
- A download from the site opens like any other app.
- Installers stay thin: fetch, check the hash, copy.
- It is how Ghostty and Glide ship. The release gate exists because Glide once shipped unsigned releases through a broken shim.

This rests on grants surviving an update to a new path that satisfies the same requirement, which holds on macOS 27: a store-path app started by launchd, and a self-managed install updated by Sparkle, both kept Accessibility and notifications.

## Consequences

- **The keys are exposed in CI.** The certificate's private key and the notary key live in GitHub secrets, in a `release` environment that only `v*` tags can deploy to, so no other workflow or branch can read them. Signing happens in an isolated job with actions pinned to commit SHAs. Any Developer ID certificate issued to the team satisfies the signing requirement, so the Apple Account is as sensitive as the key.
- **A release needs Apple's notary service.** When a submission isn't accepted, the release fails with the service's log.
- **New releases need an active membership.** Published releases keep opening if it lapses, but not if their certificate is revoked: revoking is how a leaked key is fixed, and it stops every release signed with that certificate from opening. Grants survive the reinstall, because the requirement names the team.
- **Every change you run day-to-day goes through a release.** Rolling back means installing the previous release; every release stays published.

## Rejected alternatives

- **A self-signed certificate, valid for 20 years or more**: no membership, but every first launch goes through System Settings, a DMG signed with it can't be opened, Homebrew's official tap refuses the app, and its requirement pins the certificate's hash, so re-issuing it, even with the same key, costs every grant.
- **Yearly certificates pinned by hash**: a re-grant every year. AeroSpace's renewals have twice cost its users their Accessibility grant.
- **Ad-hoc signing with a fixed identifier**: grants are still lost on update ([nixpkgs#517790](https://github.com/NixOS/nixpkgs/issues/517790)).
- **Signing and notarizing the DMG too**: a second submission for nothing anyone sees, since the stapled app inside already carries the ticket.
- **An app-specific password for `notarytool`**: it rides on the Apple Account's own sign-in, while a team key reaches only App Store Connect, with the Developer role, and is revoked on its own.
- **Nix builds from source with nixpkgs' Swift, and activation signs**: every Mac needs the certificate in its keychain; `codesign` would have to reach the keychain during `sudo darwin-rebuild switch`; nixpkgs' Swift lags the SDK the shell needs. Ghostty's own flake skips macOS for the same reasons.
- **Both paths**: two signing paths, with the key in both places.
- **Nix packaging in this repo's flake**: it ties every release to one installer. The flake is a development shell only.
- **The app registers itself through `SMAppService` in every install**: an imperative step a managed installer can't see. Only self-managed installs register, and only when the user chooses to (ADR-0011).
- **A `service install` command that writes its own plist**: it bakes store paths into the plist, the pitfall seen in yabai, skhd, rift and espanso.
