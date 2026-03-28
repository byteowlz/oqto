# AGENTS.md (crate: oqto-protocol)

Scope: `backend/crates/oqto-protocol/**`

- Canonical protocol crate shared across components.
- Keep message/event/part definitions harness-agnostic and UI-friendly.
- Avoid embedding runtime-specific policy or transport details in protocol types.
- Prefer additive changes; avoid breaking enum/tag semantics without explicit migration.
- When changing types used by frontend bindings, ensure generated TS stays in sync.
