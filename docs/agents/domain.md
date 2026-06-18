# Domain docs

Oqto uses a single-context domain-doc layout.

## Domain language

Read `CONTEXT.md` for the project's domain language: nouns, boundaries, roles, and terms that should stay consistent across code, docs, issues, and ADRs.

## Architectural decisions

`docs/adr/` is the canonical decision log.

Before architecture changes, read the relevant past ADRs. When making a new architectural decision or changing an old one, add or update an ADR instead of burying the decision in chat, a PR note, or implementation comments.

## Consumer rule

Skills that diagnose bugs, improve architecture, write PRDs, or perform TDD should load `CONTEXT.md` and relevant ADRs before proposing or changing architecture.
