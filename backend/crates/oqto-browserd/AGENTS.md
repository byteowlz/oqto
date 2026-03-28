# AGENTS.md (crate: oqto-browserd)

Scope: `backend/crates/oqto-browserd/**`

- Browser daemon should be robust under crashes/restarts and stale sessions.
- Prioritize deterministic lifecycle management (start, attach, stream, stop).
- Keep external process boundaries explicit and observable.
- Avoid mixing daemon runtime logic with control-plane concerns.
