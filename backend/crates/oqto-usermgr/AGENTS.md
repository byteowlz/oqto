# AGENTS.md (crate: oqto-usermgr)

Scope: `backend/crates/oqto-usermgr/**`

- User management is security-sensitive: favor correctness over convenience.
- Keep user identity, role, and permission transitions explicit and auditable.
- Avoid side effects across users; enforce strict tenant boundaries.
- Any auth/role change must include tests and clear migration/update paths.
