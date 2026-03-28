# AGENTS.md (crate: oqto-sandbox)

Scope: `backend/crates/oqto-sandbox/**`

- Sandbox is a security boundary. Default-deny posture wins over convenience.
- Keep policy evaluation explicit and auditable (no hidden implicit allow paths).
- Separate static capability gates from dynamic runtime approvals/policies.
- Never weaken restrictions for backwards compatibility; remove obsolete paths instead.
- Changes must consider local + remote runner environments.
