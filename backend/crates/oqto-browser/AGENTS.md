# AGENTS.md (crate: oqto-browser)

Scope: `backend/crates/oqto-browser/**`

- Browser integration crate: keep API boundaries narrow and explicit.
- Treat browser control as untrusted I/O; validate all inputs and sanitize outputs.
- Do not leak credentials/tokens into logs, screenshots, or event payloads.
- Keep protocol-facing types stable and test transport/error mapping.
