---
status: superseded by ADR-0017
---

# One self-signed certificate, valid for 20+ years

macOS ties every permission grant to the app's signing requirement. For a self-signed app, that is the bundle ID `io.github.choco.nightcap` plus the hash of the signing certificate itself. Re-issuing the certificate, even with the same key, changes the hash and loses every grant. Nightcap runs only on the author's own machines, so there is no notarization.

## Decision

The certificate is created once, self-signed, with a validity of 20+ years. It is kept in three places: an offline backup, the `release` environment secret, used only by the signing job of tag builds, and the dev machine's keychain. The signing requirement is committed in the repo, and every release is verified against it (ADR-0003).

Dev builds are signed locally with the same certificate but under the bundle ID `io.github.choco.nightcap.dev`, with their own socket and state directories, so they never interfere with the installed app.

## Rejected alternatives

- **Yearly certificates**: a re-grant every year. AeroSpace's renewals have twice cost its users their Accessibility grant.
- **Ad-hoc signing with a fixed identifier**: grants are still lost on update ([nixpkgs#517790](https://github.com/NixOS/nixpkgs/issues/517790)).
