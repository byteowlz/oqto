# AGENTS.md (crate: oqto-runner)

Scope: `backend/crates/oqto-runner/**`

- This crate is the per-user runner daemon that owns harness processes.
- Prioritize reliability: process lifecycle, state transitions, and event ordering must be deterministic.
- Translate native harness events into canonical events here (not in backend/frontend).
- Keep security boundaries strict: runner executes user-scoped actions, never cross-user shortcuts.
- Do not import control-plane business logic from `oqto`; communicate via protocol/client boundaries.
- Favor small modules over growing monoliths in daemon/server handlers.
