# Host activity contract and AGENT_CTX injection ownership

pi sessions under Oqto emit lightweight, best-effort activity heartbeats to a host-visible directory so an external collector (gvnr's awareness plane) can see which agents are running, their state, workspace, current tool, model, and last activity. This is the first concrete implementation of the gvnr event-log emitter + the `AGENT_CTX` contract — built file-first and forward-compatible with gvnr, not as a one-off.

## AGENT_CTX injection ownership

`AGENT_CTX_*` is identity/context metadata (not a security boundary). Each fact is injected by the layer authoritative for it — never inferred downstream (this resolves the sandbox cwd-vs-canonical-path ambiguity: inject, don't guess):

- **Runner / backend** inject what they can confidently generate: `AGENT_CTX_PLATFORM_NAME=oqto`, `PLATFORM_VERSION`, `PLATFORM_SESSION_ID` (supersedes the ad-hoc `OQTO_SESSION_ID`), `WORKSPACE_ID`, `WORKSPACE_PATH` (canonical, outside-sandbox), `USER_ID`, `RUN_MODE`, baseline `HARNESS`, `VERSION`. Launch point: the single-authority env builder at `oqto/src/session/service.rs` (currently injects EAVS vars + `base_system_env`) plus the runner spawn in `oqto-runner/pi_manager.rs`. Today: zero `AGENT_CTX_*` injected — this is the gap.
- **pi-env-ctx extension** (already shipped) owns harness-native facts only: `HARNESS_SESSION_ID`, `MODEL`, `SESSION_NAME`, refreshed across turns. Its README already disclaims the platform/workspace fields as runner responsibility — the runner must now fulfill that half.

## Activity heartbeat contract

- **File-based, not socket/API.** No liveness dependency on a running collector (avoids the mmry "Service Unavailable" failure class). Payload JSON mirrors the gvnr event schema so the same emitter can later also POST to gvnr unchanged.
- **Location:** `AGENT_ACTIVITY_DIR` env var (operational config, deliberately NOT in the `AGENT_CTX_*` identity namespace). Default a neutral, configurable `~/.local/state/agent-activity`; not hardcoded to any collector (gvnr independence). **Must be added to sandbox `allow_write`** — `~/.local/state` is not writable in the development profile today (blocker).
- **Keyed by `PLATFORM_SESSION_ID`**, not pid (pids recycle; sessions outlive pids across resume): `$AGENT_ACTIVITY_DIR/pi/<session_id>.json`, rewritten in place each heartbeat with a `last_activity` timestamp; machine id in the payload.
- **Expiry split (mirrors ADR-0004 registry liveness):** the collector owns authoritative dead-detection via heartbeat-age (stale = older than N x interval), because a crashed agent cannot self-clean. The runner additionally best-effort-deletes the file on graceful `stop_session`. No cleanup daemon.
- **Privacy (mirrors ADR-0007):** tool names ok; last-user-message previews are content — truncate + opt-in; tool args / commands must be redacted, never raw secrets; in multi-user the dir is per-principal, mode 0700 (host-visible, must not be cross-readable).

## Blockers to land first

1. Add `AGENT_ACTIVITY_DIR` (and its default path) to sandbox `allow_write`.
2. Implement runner/backend `AGENT_CTX_*` platform-field injection at the env authority (needed for gvnr regardless).
3. Per-principal 0700 activity dir in multi-user.
