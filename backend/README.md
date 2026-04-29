# Oqto Backend

The backend is the Oqto control plane. It exposes HTTP/WebSocket APIs, talks to per-user runners, coordinates workspace/session state, and integrates host services such as EAVS, hstry/oqto-log, and system users.

## Mental model

```text
Frontend
  -> oqto                thin server binary: config, state wiring, routes
      -> API layer       HTTP/WebSocket transport
      -> domain crates   users, workspaces, sessions, history, provisioning
      -> adapter crates  host OS, EAVS, runner sockets, files, sandbox
  -> oqto-runner         per-user agent process daemon
```

Binaries should stay thin. Business logic should live in domain crates. External systems should live behind adapter crates.

## Current key binaries

| Binary | Package | Responsibility |
| --- | --- | --- |
| `oqto` | `crates/oqto` | Main backend server and current composition root |
| `oqtoctl` | `crates/oqtoctl` | Operator CLI for users, sessions, admin APIs, setup helpers |
| `oqto-runner` | `crates/oqto-runner` | Per-user agent daemon and harness process supervisor |
| `oqto-files` | `crates/oqto-files` | Workspace file access server |
| `oqto-sandbox` | `crates/oqto-sandbox` | Sandbox policy and wrapper binary |

## Crate map

See `crates/README.md` for the authoritative crate responsibilities, dependency direction, and extraction plan.

## Development commands

From repo root, prefer `just` recipes. From `backend/`:

```bash
cargo fmt -p <crate>
cargo check -p <crate>
cargo test -p <crate>
cargo clippy -p <crate>
```

Deploy builds use the `deploy-fast` Cargo profile for local iteration. Strict release builds still use the normal `release` profile.

## Backend standard

1. New code should go into the crate that owns the domain, not into `crates/oqto` by default.
2. `crates/oqto` should trend toward only server startup, config loading, dependency wiring, and route mounting.
3. Do not introduce upward dependencies into the `oqto` binary crate.
4. Keep adapters explicit: host OS in `oqto-host`, EAVS in `oqto-eavs`, runner socket access in a runner-client crate once extracted.
5. If a folder needs explanation, add a short README there instead of adding another loose doc elsewhere.
