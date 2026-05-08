# oqto-sandbox

## Responsibility

Sandbox policy types and sandbox wrapper binary for restricting agent processes.

## Non-goals

No session orchestration, no user provisioning, and no backend route handlers.

## Depends on

Low-level process/config/logging crates needed to apply sandbox policies.

## Used by

`oqto-host`, `oqto`, deploy/runtime flows, and the `oqto-sandbox` binary.

## Migration notes

Sandbox configuration is security-sensitive. Per-workspace overrides may add restrictions but must not silently weaken host-level policy.
