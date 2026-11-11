# Glossary Map

## Contexts

- [Control](./crates/core/GLOSSARY.md): decides whether Night Shift is on or off, from rules, the schedule and the user's own changes, and checks what it depends on against what the user set up
- [App](./app/GLOSSARY.md): what the user sees and sets up, and the fixes it offers for the problems the checks find
- [Release](./release/GLOSSARY.md): builds, signs and publishes the app so grants survive updates

## Relationships

- **Control → App**: Control computes the checks, as facts, and answers the queries, each read's from one snapshot, from which App decides what the menu, Settings and onboarding show, and whether onboarding opens; App words them and the checks, picks each check's fixes, draws them and sends back commands, a fix's among them, and announces the problems that appear from each outcome's snapshot
- **App → Control**: website checking, a choice made in App, decides whether Control watches the front browser
- **App → Control**: the tour runs in a trial, which App begins, asking for the stand-in when it finds Night Shift off, and ends
- **Release → App**: every grant App checks is tied to Release's signing requirement
