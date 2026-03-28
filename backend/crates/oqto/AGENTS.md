# AGENTS.md (crate: oqto)

Scope: `backend/crates/oqto/**`

- This crate is the control-plane backend (HTTP/WebSocket API, authz, routing, admin).
- Keep it thin: business logic belongs in focused modules/services, not giant handlers.
- Canonical protocol is the contract. Do not add harness-specific leaks to API payloads.
- Runner interaction goes through `runner/` client/protocol boundaries. Do not embed runner daemon internals here.
- History access must go through hstry interfaces used by this crate; avoid ad-hoc persistence paths.
- Keep `main.rs` wiring-focused; extract logic into modules.
- If changing WebSocket mux behavior, preserve channel isolation and command response correlation.
