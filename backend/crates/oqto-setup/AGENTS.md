# AGENTS.md (crate: oqto-setup)

Scope: `backend/crates/oqto-setup/**`

- Setup flows must be idempotent and safe to re-run.
- Validate environment assumptions early and fail with actionable errors.
- Never silently broaden permissions or weaken security defaults.
- Keep setup steps explicit so operators can audit what changed.
