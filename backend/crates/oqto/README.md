# oqto

## Responsibility

Main backend server binary and current composition root. It loads config, wires dependencies, mounts HTTP/WebSocket routes, and starts the control plane.

## Non-goals

New domain logic should not be added here by default. Prefer extracting or using a focused crate for users, sessions, workspaces, history, provisioning, host OS integration, or external clients.

## Depends on

Domain and adapter crates such as `oqto-host`, `oqto-eavs`, `oqto-protocol`, `oqto-files`, and `oqto-sandbox`.

## Used by

Deployment and local development as the `oqto` server binary.

## Migration notes

This crate is intentionally shrinking. Compatibility shims may remain temporarily to keep refactors small, but the target is a thin server/wiring crate.
