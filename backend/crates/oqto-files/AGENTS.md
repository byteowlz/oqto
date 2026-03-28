# AGENTS.md (crate: oqto-files)

Scope: `backend/crates/oqto-files/**`

- File service must be safe-by-default: strict path normalization, traversal protection, explicit workspace roots.
- Preserve predictable wire behavior (stable errors, bounded payloads, deterministic listing/read semantics).
- Avoid hidden side effects in read APIs.
- Keep large operations bounded and streaming/paginated where appropriate.
- Any authz/path boundary change requires tests.
