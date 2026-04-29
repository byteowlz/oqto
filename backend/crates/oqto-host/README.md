# oqto-host

## Responsibility

Host OS integration for Oqto: Linux users, process management, local runtime orchestration, ports, environment setup, and sandbox type re-exports.

## Non-goals

No HTTP handlers, session business logic, user repository logic, or runner protocol logic. If code needs database access or runner sockets, it belongs in a higher-level domain/service crate.

## Depends on

Low-level system and utility crates plus `oqto-sandbox` for sandbox policy types.

## Used by

`oqto` and future provisioning/session crates that need host OS operations.

## Migration notes

`oqto/src/local/mod.rs` currently re-exports this crate for compatibility. Remaining `user_mmry` and `user_sldr` managers still live in `oqto` until provisioning is extracted.
