# oqtoctl

## Responsibility

Operator CLI for Oqto. It manages users, sessions, admin APIs, setup helpers, local database bootstrapping, and troubleshooting commands.

## Non-goals

No backend server state, no long-running services, and no direct reuse as a domain library. Shared logic should move into domain/adapter crates, not be imported from this binary.

## Depends on

Admin HTTP/socket APIs, `oqto-eavs`, database helpers for bootstrap paths, and CLI/runtime crates.

## Used by

Operators, setup scripts, deploy scripts, admin scripts, and E2E tests.

## Migration notes

This was extracted from `oqto` to reduce the server crate. Some CLI-only setup code may later move into provisioning/domain crates if it becomes shared.
