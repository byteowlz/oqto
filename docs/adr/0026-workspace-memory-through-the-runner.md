# Workspace memory goes through the runner, not in-process

## Status

Accepted (2026-07-23). Supersedes the access-path detail in ADR-0010.

## Context

ADR-0010 embedded `mmry-core` and made memory a per-workspace `.mmry/mmry.jsonl`
ledger. The initial implementation read/wrote that file **in the oqto backend
process** via `MemoryFile::open_workspace`. That works today only because
multi-user provisioning puts every per-user account in a shared `oqto` group and
creates workspace dirs `2770` setgid, so the backend (in the `oqto` group) can
reach each user's `.mmry/`.

Two problems:

- **Container mode is broken.** In container runtime, the workspace is owned and
  mounted for the session's user/container; the backend process cannot resolve
  or access those paths the way the runner can. In-process reads assume a host
  path the backend can see.
- **It bypasses the isolation seam.** Oqto's rule is "actions go through
  runners" — the runner is the per-user/OS boundary that owns workspace
  filesystem access. Reaching into every user's home from the backend, gated
  only by group membership, violates that seam.

`trx` already solved the identical problem: `trx_*` operations run **through the
runner** with a `workspace_path`, using `trx-core` open in the runner process.

## Decision

Route all workspace memory operations through the oqto-runner, mirroring `trx`:

- The runner embeds `mmry-core` and services `SearchMemories`, `AddMemory`,
  `DeleteMemory`, `ListMemories`, and `UpdateMemory`, each carrying a
  `workspace_path`, via `MemoryFile::open_workspace` in `spawn_blocking`.
- The backend memory handlers resolve the workspace's runner with
  `resolve_runner_for_workspace_path` (personal or shared-workspace) and call the
  runner client. The backend no longer opens `.mmry/mmry.jsonl` itself.
- The `/api/workspace/memories*` HTTP DTOs are unchanged; only the transport
  behind them changes.

The runner runs as the workspace's user, so filesystem permissions are natural
in local mode, and container/bind-mounted workspaces are reached the same way
the runner already serves `read_file`/`list_directory`.

## Consequences

- Memory works uniformly in local and container modes and respects the runner
  isolation seam. The backend no longer needs group access to user homes for
  memory.
- The runner gains a `mmry-core` dependency (`default-features = false`).
- The previously stubbed runner memory handlers become real; their request DTOs
  gain `workspace_path`, and `ListMemories`/`UpdateMemory` are added.
- Access path is now frontend -> api -> runner -> `mmry-core` (as ADR-0010
  originally intended, correcting the in-process shortcut).
