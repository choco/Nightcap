# Domain Docs

This repo's domain language and decisions are split by bounded context. `GLOSSARY-MAP.md` at the root lists each bounded context, where its `GLOSSARY.md` lives, and how the bounded contexts relate. A bounded context's own glossary may define _context_ as a domain term; that meaning is unrelated.

```
/
├── GLOSSARY-MAP.md
├── docs/adr/              ← system-wide decisions
└── <bounded context dir>/
    ├── GLOSSARY.md
    └── docs/adr/          ← decisions scoped to that bounded context
```

## Before exploring

Read `GLOSSARY-MAP.md`, then the `GLOSSARY.md` of every bounded context the topic touches, plus the ADRs in root `docs/adr/` and in those bounded contexts' `docs/adr/`.

A glossary or ADR directory that doesn't exist yet is expected: proceed silently. `/domain-modeling` creates these files lazily, when a term or decision actually resolves.

## Adding a bounded context

Every term belongs to a bounded context. When the first term of a new one resolves, create `GLOSSARY.md` in its directory and add it, with its relationships, to `GLOSSARY-MAP.md`.

## Use the glossary's vocabulary

When your output names a domain concept (a commit message, a refactor proposal, a hypothesis, a test name), use the term as its bounded context's `GLOSSARY.md` defines it, steering clear of the synonyms listed under _Avoid_.

A concept missing from the glossary is a signal: either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

When your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0005 (event-sourced orders), but worth reopening because…_
