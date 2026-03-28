# AGENTS.md (crate: oqto-runner-protocol)

Scope: `backend/crates/oqto-runner-protocol/**`

- This crate is the single source of truth for runner RPC + shared Pi wire types.
- Keep it dependency-light and stable. Avoid pulling in backend/runner implementation concerns.
- Treat changes as protocol changes: maintain backward/forward decode safety where possible.
- `oqto/src/runner/protocol.rs` and `oqto-runner/src/protocol.rs` are strict re-export shims.
- If adding fields, prefer additive evolution and clear defaults.
- Update related tests/fixtures whenever schema changes.
