# oqto-eavs

## Responsibility

EAVS integration: admin/client API calls, provider metadata types, virtual key requests, and Pi `models.json` generation.

## Non-goals

No user provisioning workflow, no session environment injection, and no HTTP route handlers. Those should call this crate from higher-level services.

## Depends on

HTTP, serialization, time, and error-handling crates.

## Used by

`oqto`, `oqtoctl`, admin/provisioning workflows, and future `oqto-provisioning`.

## Migration notes

`oqto/src/eavs/mod.rs` re-exports this crate for compatibility while call sites migrate to `oqto_eavs` directly.
