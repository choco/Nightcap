# Releases are signed with a Developer ID and notarized

ADR-0004 chose a self-signed certificate because Nightcap ran only on the author's Macs. Nightcap now has a public site with a download button, and a self-signed app reaches people through warnings: Gatekeeper blocks its first launch until they find "Open Anyway" in System Settings, refuses outright to mount a DMG signed with that certificate, and keeps the app out of Homebrew's official cask tap. The project now has an Apple Developer Program membership.

## Decision

- **Signing**: every release and dev build is signed with the team's Developer ID Application certificate, releases with a secure timestamp. `release/sign` picks the certificate by the team that the signing requirement names.
- **Signing requirement**: the Developer ID designated requirement, which names the bundle ID, Apple's Developer ID anchor and the team ID rather than one certificate. A renewed certificate keeps every grant, so no certificate has to outlive the app.
- **Notarization**: CI submits the signed app with `notarytool`, signing in with an App Store Connect team key with the Developer role, kept in the `release` environment. It staples the ticket to the app before packaging, so the zip and the DMG carry the same notarized app. The DMG stays unsigned: Gatekeeper mounts an unsigned image without asking and checks the app inside when it opens, so one submission covers both downloads.
- **Release gate**: it also requires a valid stapled ticket and Gatekeeper's verdict "Notarized Developer ID".
- **Library validation** is on. Sparkle is signed by the same team, so the app carries no `com.apple.security.cs.disable-library-validation`, and a framework swapped inside a writable bundle doesn't load.
- **Changing certificates** is an ordinary Sparkle update: Sparkle accepts a new code signature when the EdDSA key stays the same, so the two never change in the same release.

This replaces ADR-0004 and, in ADR-0003 and ADR-0015, the parts that rest on it: the self-signed certificate, the DMG signed with it, `disable-library-validation`, and a cask that strips the quarantine flag.

## Consequences

- Moving off the self-signed certificate costs every existing install and dev build one re-grant of Accessibility and notifications.
- A release needs Apple's notary service. When a submission isn't accepted, the release fails with the service's log.
- New releases need an active membership. Published releases keep opening if it lapses, but not if their certificate is revoked: revoking is how a leaked key is fixed, and it stops every release signed with that certificate from opening. Grants survive the reinstall, because the requirement names the team.
- Any Developer ID certificate issued to the team satisfies the signing requirement, so the Apple Account is as sensitive as the key.

## Rejected alternatives

- **Keeping the self-signed certificate (ADR-0004)**: every first launch goes through System Settings, which the site and the DMG have to explain, and a signed DMG can't be opened at all.
- **Pinning the certificate's hash in the signing requirement, as ADR-0004 did**: every renewal would cost every grant again. The team ID is what Apple keeps across renewals.
- **Signing and notarizing the DMG too**: a second submission for nothing anyone sees, since the stapled app inside already carries the ticket.
- **An app-specific password for `notarytool`**: it rides on the Apple Account's own sign-in, while a team key reaches only App Store Connect, with the Developer role, and is revoked on its own.
