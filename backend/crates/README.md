# Backend crates

This directory is the backend architecture map. Each crate should have a narrow responsibility and a clear dependency direction.

## Current crates

| Crate | Responsibility |
| --- | --- |
| `oqto` | Main server binary and current composition root. This should shrink over time. |
| `oqtoctl` | Operator CLI. Talks to admin HTTP/socket APIs and owns CLI-only setup helpers. |
| `oqto-runner` | Per-user daemon that owns agent harness processes and native-to-canonical event translation. |
| `oqto-files` | Workspace file access service. |
| `oqto-host` | Host OS integration: Linux users, process management, local runtime, sandbox type re-exports. |
| `oqto-eavs` | EAVS API client and Pi `models.json` generation. |
| `oqto-protocol` | Shared canonical protocol/types. No business logic. |
| `oqto-pi` | Pi wire protocol types and session-file helpers shared by server/runner code. |
| `oqto-sandbox` | Sandbox policy types and wrapper binary. |
| `oqto-usermgr` | Privileged user-management helper binary. |
| `oqto-setup` | Setup utility. |
| `oqto-scaffold` | Project scaffolding utility. |
| `oqto-browser` | Browser control integration. |

## North-star crates

These are the intended future boundaries. Create them only when moving real code.

| Future crate | Intended responsibility |
| --- | --- |
| `oqto-core` | Shared IDs, small value types, error helpers. |
| `oqto-auth` | Auth, JWT, passwords, roles, API keys, invite semantics. |
| `oqto-users` | User domain and repository/service logic. |
| `oqto-workspaces` | Personal/shared workspace domain. |
| `oqto-provisioning` | User/workspace setup, runner setup, Pi config sync, mmry/sldr provisioning. |
| `oqto-sessions` | Session lifecycle and orchestration. |
| `oqto-history` | oqto-log, import/query/search APIs, hstry interop during migration. |
| `oqto-runner-client` | Backend-side client for runner sockets/protocol. |
| `oqto-api` | Route composition and transport handlers only. |
| `oqto-container` | Container runtime integration if container mode remains. |

## Dependency direction

Allowed shape:

```text
binaries -> API/wiring -> domain crates -> adapter crates -> protocol/core
```

Rules:

1. Library crates must not depend on the `oqto` server crate.
2. Protocol/type crates must not depend on domain or adapter crates.
3. Adapter crates wrap external systems; domain crates should depend on traits/interfaces where practical.
4. HTTP/WebSocket handlers should call services, not own business logic.
5. New code should not enter `crates/oqto/src` unless it is composition, config, route mounting, or a temporary compatibility shim.

Enforcement:

- `just lint` runs `scripts/lint/backend-crate-boundaries.py`.
- The check fails if any crate adds a new dependency on the `oqto` server crate.
- `oqto-runner -> oqto` is the only temporary allowlisted legacy edge and is tracked by `oqto-3ct7.3`.

## Extraction standard

When extracting code:

1. Create the new crate with a short README.
2. Move files without changing semantics first.
3. Add compatibility re-exports only when needed to keep the diff small.
4. Run `cargo fmt -p <new-crate> -p oqto` and `cargo check -p <new-crate> -p oqto`.
5. Remove compatibility shims in follow-up slices once call sites are migrated.
