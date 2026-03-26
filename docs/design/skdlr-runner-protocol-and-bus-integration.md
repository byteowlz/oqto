# skdlr Runner Protocol and Oqto Bus Integration (Container-Native Multi-User)

Status: Draft proposal

Owner: Oqto platform

Last updated: 2026-03-25

## 1. Problem Statement

Oqto currently has a strong Linux host model for multi-user isolation (per-user Linux accounts + systemd user services). This does not translate cleanly to Docker Desktop / container-native deployment.

We need a container-native scheduling and execution model that:

1. keeps per-user execution isolation
2. avoids systemd/logind dependencies in the scheduling path
3. integrates with Oqto's canonical event/bus architecture
4. remains transport-agnostic (Unix sockets now, TCP later, potential gRPC/queue later)

## 2. Goals and Non-Goals

### Goals

- Make `skdlr` the central schedule authority in container mode.
- Execute scheduled work via user runners (not directly in skdlr).
- Define a versioned runner execution protocol independent of transport.
- Map scheduler lifecycle to Oqto bus events for UI/observability.
- Enforce sandbox boundaries: scheduler orchestrates, runner executes in user sandbox.
- Support SQLite-first operation (single active scheduler authority).

### Non-Goals (v1)

- Active/active distributed scheduler HA.
- Exactly-once execution guarantees.
- Replacing existing runner protocol for non-scheduler features.
- Mandatory migration to Postgres.

## 3. Architecture

### 3.1 Control vs Execution Plane

- **Control plane**: Oqto backend + skdlr service
  - stores schedules
  - determines due jobs
  - dispatches to runner targets
  - records attempts/outcomes

- **Execution plane**: per-user runner runtime
  - receives execute/cancel requests
  - runs command in user container sandbox
  - streams/provides status and completion

### 3.2 Ownership Rules

- `skdlr` decides **when** jobs run.
- `runner` decides **how** jobs execute within user constraints.
- scheduler never shells out directly to host in multi-user container mode.

## 4. Protocol: Scheduler -> Runner

Protocol is JSON envelope + typed message payloads.

### 4.1 Envelope

```json
{
  "v": 1,
  "type": "job.execute",
  "request_id": "uuid",
  "tenant_id": "user-or-tenant-id",
  "runner_id": "runner-identifier",
  "ts": "2026-03-25T18:40:00Z",
  "auth": { "token": "opaque-token" },
  "payload": {}
}
```

### 4.2 Core Message Types

- `job.execute`
  - `job_id`, `schedule_id`, `idempotency_key`, `command`, `workdir`, `env`, `timeout_sec`
- `job.accepted`
  - accepted/duplicate metadata
- `job.progress` (optional)
  - phase/progress hints
- `job.completed`
  - exit code, timing, output tail/artifacts
- `job.failed`
  - error code, message, retryability hints
- `job.cancel`
  - explicit cancellation
- `runner.hello` / `runner.health`
  - capabilities and limits

### 4.3 Protocol Guarantees

- At-least-once delivery from scheduler.
- Idempotency key is mandatory on execute and completion messages.
- Runner must dedupe duplicate `job.execute` by idempotency key.
- Transport must not change protocol semantics.

## 5. Transport Abstraction

Add `RunnerClient` abstraction in control plane:

- `UnixRunnerClient` (current local/systemd mode)
- `TcpRunnerClient` (container mode)
- future adapters (gRPC, queue)

Scheduler code calls interface methods only (`dispatch`, `cancel`, `health`), never socket primitives directly.

## 6. Oqto Bus Integration

Runner protocol events are translated into bus events.

### 6.1 Event Types

- `scheduler.job.queued`
- `scheduler.job.dispatched`
- `scheduler.job.started`
- `scheduler.job.completed`
- `scheduler.job.failed`
- `scheduler.job.retry_scheduled`

### 6.2 Boundary Rules

- Runner wire protocol stays internal between control plane and execution plane.
- Oqto bus is the canonical UI/event surface.
- Frontend subscribes to scheduler events via existing bus/WS channel policy.

## 7. SQLite-First Reliability Model

v1 uses SQLite with **single active scheduler authority**.

### 7.1 Required Persistence Additions

Introduce job instance table with lease fields:

- `job_id`, `schedule_id`, `tenant_id`
- `due_at`, `status` (`queued|running|succeeded|failed|dead`)
- `lease_until`, `attempt`, `max_attempts`
- `idempotency_key`
- `last_error`, `next_retry_at`

### 7.2 Claiming Semantics

- atomic claim transaction required
- lease-based recovery for stuck jobs
- exponential backoff retries
- terminal dead state after max attempts

## 8. Sandboxing Requirements

In multi-user container mode:

- skdlr must not execute host commands directly.
- all execution is delegated to user runner runtime.
- user containers enforce sandbox:
  - non-root user
  - per-user writable volume only
  - dropped capabilities / seccomp profile
  - resource limits (cpu/memory/pids)
  - controlled network egress

## 9. Rollout Plan

1. Implement protocol structs + `RunnerClient` abstraction in Oqto.
2. Add scheduler event translation into bus.
3. Implement SQLite job instance/lease/retry model.
4. Wire skdlr dispatch through runner adapters.
5. Add container-mode runtime config and docs.
6. Validate with one control plane + multiple user runner containers.

## 10. Open Questions

- Should scheduler events be persisted in history service or remain ephemeral bus-only?
- Should per-job logs live in runner-local storage, central storage, or both?
- Should cancellation semantics be best-effort only in v1?
- What auth format should be used for TCP runner transport in v1 (opaque token vs signed token)?

## 11. Compatibility Notes

- Host/systemd mode remains valid and unchanged for native Linux deployments.
- Container mode is additive and selected by runtime config.
- This design keeps protocol stable while allowing transport migration.
