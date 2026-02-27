# Security Remediation Plan

Status: ACTIVE
Date: 2026-02-27
Source: `oqto-architecture-analysis.pdf` (February 2026, v1.0)

## Summary

The architecture analysis scored security at **6/10** -- "Good primitives but gaps in defense-in-depth." This plan organizes the six tracked security/stability findings into actionable phases with dependencies, scoping, and implementation notes.

---

## Phase 1: Immediate (MVP Critical) -- P0

These must be addressed before any multi-user deployment beyond the current trusted dev environment.

### 1.1 Runner Authentication (oqto-e067)

**Problem:** Runner registration uses a plain `runner_id` string over Unix/TCP sockets with no cryptographic verification. A malicious process on the same host (or network, for TCP) could impersonate a runner and intercept agent sessions.

**Current state:**
- `backend/crates/oqto/src/runner/client.rs`: Connects to Unix socket at `/run/oqto/runner-{uid}.sock` or custom pattern. No auth handshake.
- `backend/crates/oqto/src/runner/protocol.rs`: JSON-over-Unix-socket, newline-delimited. `Ping` is the only health check. No challenge-response.
- Runner-to-backend is currently Unix socket only (filesystem permissions provide *some* isolation).

**Approach -- phased:**

1. **Phase 1a: Token-based auth (quick win)**
   - Generate a random 256-bit token at runner registration time
   - Store in runner's user-owned directory (`/run/oqto/runner-{uid}/token`, mode 0600)
   - Runner sends token in first message; backend validates before accepting any commands
   - This is sufficient for Unix socket communication where filesystem permissions already limit access

2. **Phase 1b: mTLS for TCP (when remote runners ship)**
   - Only needed when runners connect over TCP (remote runners, cross-machine)
   - Generate per-runner client certificates signed by oqto CA
   - Certificate rotation via admin API
   - Runner attestation: hash of runner binary + sandbox capability proof

**Dependencies:** None. Can start immediately.
**Effort:** Phase 1a: ~2 days. Phase 1b: ~1 week.

### 1.2 hstry Write-Ahead Spool (oqto-29e1)

**Problem:** hstry is a single point of failure. If the gRPC service goes down, all message persistence fails silently. Long-running agent sessions lose their entire conversation.

**Current state:**
- `oqto-runner` reads hstry SQLite directly (per AGENTS.md) but writes go through gRPC
- No local buffering, no retry, no circuit breaker on gRPC calls
- No frontend indicator for sync status

**Approach:**

1. **Runner-side local spool (SQLite WAL)**
   - Runner maintains a local SQLite spool at `/run/oqto/runner-{uid}/spool.db`
   - All writes go to local spool first (immediate, synchronous)
   - Background task forwards to hstry gRPC with exponential backoff
   - Deduplication via message ID on hstry side
   - On runner restart, replay unacknowledged messages

2. **Circuit breaker on gRPC client**
   - Track consecutive failures; open circuit after 3 failures
   - Half-open probe every 10s
   - While open: writes accumulate in spool, reads serve from local cache
   - Metrics: spool depth, circuit state, last successful sync

3. **Frontend sync indicator**
   - Backend exposes spool depth in session status
   - Frontend shows "history sync pending" badge when spool > 0

**Dependencies:** None. Can start immediately.
**Effort:** ~1 week for spool + circuit breaker. ~2 days for frontend indicator.

---

## Phase 2: Short-Term (Pre-Scale) -- P1

These should be completed before opening to untrusted users or scaling beyond a handful of users.

### 2.1 Defense-in-Depth Sandboxing (oqto-cxxr)

**Problem:** Sandbox relies solely on bwrap. If bwrap has vulnerabilities, is misconfigured, or unavailable, agents run completely unsandboxed. No seccomp-bpf, no Landlock.

**Current state:**
- `backend/crates/oqto/src/local/sandbox.rs` (1449 lines): Implements bwrap + macOS sandbox-exec
- Configurable profiles (development/minimal/strict)
- Guard (FUSE) filesystem for runtime access control exists
- No seccomp-bpf filters, no Landlock, no capability dropping

**Approach -- layered fallback chain:**

1. **Landlock (Linux 5.13+)** -- preferred on modern kernels
   - Filesystem sandboxing without setuid
   - `landlock` Rust crate for safe API
   - Composable with existing bwrap namespace isolation
   - Minimal overhead

2. **seccomp-bpf** -- syscall filtering
   - Deny-by-default allowlist for agent operations
   - Block: `ptrace`, `mount`, `reboot`, `init_module`, `kexec_load`
   - Allow: `read`, `write`, `open`, `close`, `mmap`, `brk`, `execve` (controlled), network ops
   - `libseccomp-rs` crate
   - Applied inside bwrap (or standalone if bwrap unavailable)

3. **Capability dropping**
   - `PR_SET_NO_NEW_PRIVS` before exec
   - Drop all capabilities after sandbox setup
   - `caps` Rust crate

4. **Fallback chain:** bwrap+landlock+seccomp (best) -> bwrap+seccomp -> landlock+seccomp -> seccomp-only -> log warning

**Dependencies:** None, but benefits from testing against real agent workloads.
**Effort:** ~2 weeks (Landlock + seccomp + integration testing).

### 2.2 WebSocket Reconnection & Backpressure (oqto-fezg)

**Problem:** Frontend has no exponential backoff on WebSocket reconnection. Backend slowdowns can cascade into frontend unresponsiveness. No circuit breaker.

**Current state:**
- Frontend at `frontend/src/routes/AppShellRoute.tsx`: Only a "retry" button exists, no automatic reconnection strategy
- Backend at `backend/crates/oqto/src/api/ws_multiplexed.rs`: No backpressure signaling
- No per-channel circuit breakers

**Approach:**

1. **Frontend reconnection with jittered exponential backoff**
   - Initial: 1s, max: 30s, jitter: +/- 25%
   - Max retry budget: 10 attempts before showing "connection lost" UI
   - State resync: re-fetch session list + current session messages on reconnect

2. **Backend backpressure**
   - Track send buffer depth per WebSocket connection
   - If buffer exceeds threshold: drop heartbeat events first, then non-critical events
   - Signal `backpressure: true` in event stream so frontend can show indicator
   - If sustained: close connection and let client reconnect (prevents memory leak)

3. **Circuit breaker per channel**
   - Track consecutive errors per mux channel (agent, files, terminal)
   - Open circuit for that channel only; other channels continue
   - Half-open probe with single request

**Dependencies:** None.
**Effort:** ~1 week frontend + backend.

### 2.3 Protocol Versioning (oqto-8f14)

**Problem:** No version negotiation. Adding new Part types or Event variants will break older runners/frontends with no graceful degradation.

**Current state:**
- `docs/design/canonical-protocol.md`: No version field defined
- Runner registration has no version exchange
- Frontend WebSocket handshake has no version

**Approach:**

1. **Add version to all handshakes**
   - Runner registration: `{ "protocol_version": "1.0", ... }`
   - WebSocket upgrade: `?v=1` query parameter
   - Backend validates and stores per-connection

2. **Compatibility rules**
   - Minor versions (1.0 -> 1.1): Additive only. New optional fields, new event types. Old consumers ignore unknown fields.
   - Major versions (1.x -> 2.0): Breaking. Backend supports N and N-1 simultaneously.
   - Unknown fields: `serde(deny_unknown_fields)` OFF. Unknown fields silently dropped.

3. **Version matrix documentation**
   - Table of which features require which version
   - CI tests for N and N-1 compatibility

**Dependencies:** Ideally before any public release.
**Effort:** ~3 days for initial implementation, ongoing maintenance.

### 2.4 Structured Audit Logging (enhancement to existing)

**Problem:** Analysis notes "Missing Structured Audit Trail." The protocol spec doesn't mandate security event logging.

**Current state:**
- `backend/crates/oqto/src/audit.rs` (120 lines): Basic file-based JSON audit logger
- Logs HTTP requests and WebSocket commands
- Missing: runner registration/disconnect, session create/destroy, sandbox violations, auth failures, file access patterns

**Approach:**

1. **Extend AuditEvent types**
   - `runner_connected`, `runner_disconnected`, `runner_auth_failed`
   - `session_created`, `session_destroyed`, `session_forked`
   - `sandbox_violation` (from guard/bwrap)
   - `auth_failed`, `token_revoked`
   - `file_access` (from oqto-files, with user + path)

2. **Structured format**
   - Keep JSON-lines format (already in place)
   - Add severity levels (info, warn, alert)
   - Add correlation IDs (request_id, session_id, runner_id)

3. **Rotation and retention**
   - Log rotation (daily, max 30 days)
   - Configurable in config.toml

**Dependencies:** Benefits from runner auth (1.1) being implemented first.
**Effort:** ~3 days.

---

## Phase 3: Long-Term (Enterprise) -- P2/P3

### 3.1 PostgreSQL Backend for hstry (oqto-35fg)

- Feature-flagged `--features postgres` in hstry
- Migration tool sqlite-to-postgres
- Connection pooling, read replicas
- Only needed at 500+ concurrent users

### 3.2 Runner Resource Quotas (cgroups)

- Per-user cgroup limits (CPU, memory, IO)
- Prevents noisy-neighbor between users on same host
- Enforced by runner at process spawn time
- `systemd` slice-based approach (runner service already runs per-user)

### 3.3 eavs.env / Key Storage Hardening

**Problem (from analysis):** "eavs virtual keys in eavs.env -- if world-readable, lateral movement is trivial."

**Current state:** Keys are now embedded in `models.json` rather than standalone `eavs.env` files (based on the comment in `session/service.rs`: "No eavs.env loading needed -- Pi reads the key from models.json").

**CONFIRMED VULNERABILITY (oqto-m7br, P0):** `scripts/admin/eavs-provision.sh:265` writes `models.json` with mode `644` (world-readable). Any user on the system can read another user's eavs API key. **Immediate fix: change to `600`.**

**Action:**
- **IMMEDIATE:** Fix file permissions from 644 to 600 (oqto-m7br)
- Audit all `write_file_as_user` calls for sensitive content
- Consider moving to tmpfs-backed secrets (inspired by stereOS `/run/stereos/secrets/`)
- Long-term: kernel keyring integration

---

## Dependency Graph

```
Phase 1 (parallel, no deps):
  1.1 Runner Auth Token ──────────────────────┐
  1.2 hstry Write-Ahead Spool ────────────────┤
                                               │
Phase 2 (after Phase 1):                       v
  2.1 Defense-in-Depth Sandbox ───── (independent)
  2.2 WebSocket Backpressure ─────── (independent)
  2.3 Protocol Versioning ────────── (before public release)
  2.4 Structured Audit Logging ───── (benefits from 1.1)
                                               │
Phase 3 (after Phase 2):                       v
  3.1 PostgreSQL for hstry ───────── (when >500 users)
  3.2 cgroup Resource Quotas ─────── (when multi-user at scale)
  3.3 Key Storage Hardening ──────── (before untrusted users)
```

## Tracked Issues

| Issue | Phase | Priority | Title |
|-------|-------|----------|-------|
| oqto-m7br | 1 | P0 | models.json written with 644 -- API keys world-readable |
| oqto-e067 | 1 | P0 | Runner mTLS authentication and attestation |
| oqto-29e1 | 1 | P0 | hstry gRPC HA with local spool fallback |
| oqto-cxxr | 2 | P1 | Defense-in-depth sandboxing with seccomp-bpf |
| oqto-fezg | 2 | P1 | Circuit breaker and backpressure for WebSocket |
| oqto-8f14 | 2 | P1 | Protocol versioning for canonical protocol |
| oqto-35fg | 3 | P1 | PostgreSQL backend for hstry |

## Estimated Total Effort

| Phase | Effort | Timeline |
|-------|--------|----------|
| Phase 1 | ~2 weeks | Immediate |
| Phase 2 | ~4 weeks | Next sprint |
| Phase 3 | ~6 weeks | Quarter+ out |
