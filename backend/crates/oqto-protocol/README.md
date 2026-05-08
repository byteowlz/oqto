# oqto-protocol

## Responsibility

Shared protocol and canonical type definitions used across backend, runner, and frontend type generation.

## Non-goals

No business logic, no database access, no process management, and no HTTP server implementation.

## Depends on

Only small serialization/type dependencies.

## Used by

`oqto`, `oqto-runner`, generated frontend types, and future domain crates that need canonical payloads.

## Migration notes

Keep this crate stable and low-dependency. If adding a dependency here feels convenient, it is probably the wrong crate.
