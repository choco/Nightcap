# Glossary Map

## Contexts

- [Control](./crates/core/GLOSSARY.md): decides whether Night Shift is forced off, from rules, the schedule and the user's own toggles
- [Release](./release/GLOSSARY.md): builds, signs and publishes the app so permission grants survive updates

## Relationships

- **Release → Control**: Release ships the bundle Control runs in; every permission grant Control depends on is tied to Release's signing requirement
