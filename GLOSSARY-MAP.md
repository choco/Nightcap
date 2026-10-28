# Glossary Map

## Contexts

- [Control](./crates/core/GLOSSARY.md): decides whether Night Shift is on or off, from rules, the schedule and the user's own changes
- [App](./app/GLOSSARY.md): what the user sees and sets up, and the checks that compare that setup with what macOS is doing
- [Release](./release/GLOSSARY.md): builds, signs and publishes the app so grants survive updates

## Relationships

- **Control → App**: Control computes the checks and what Settings shows, and answers the queries from which App decides what the menu shows; App draws them and sends back commands and fixes
- **App → Control**: website checking, a choice made in App, decides whether Control watches the front browser
- **Release → App**: every grant App checks is tied to Release's signing requirement
