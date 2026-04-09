# Chat Error Lifecycle Matrix (Expected Contract)

Status: draft contract for `oqto-bcqj.1` hardening
Owner: chat/runtime
Last updated: 2026-04-09

## Why this doc exists

We have regressions where errors appear in multiple places, disappear/reappear after reload, or render as normal assistant text. The fix is not one-off patches; we need a strict lifecycle contract from Pi stream -> canonical events -> hstry persistence -> frontend rendering.

This document defines **how it is supposed to work**. Implementation and tests must converge to this contract.

---

## Core invariants (must always hold)

1. **Exactly one terminal outcome per turn**
   - Either a successful assistant message persists
   - Or one terminal assistant error message persists
   - Never both for the same failed terminal turn

2. **Retry progress is transient-durable only in frontend session state**
   - `retry.start` / in-progress errors should persist in the frontend **for the active turn** (so users can inspect what happened while retrying)
   - This persistence is session-local UI state, not hstry durability
   - They must not create durable assistant rows in hstry

3. **Terminal errors are durable and typed**
   - Persist as assistant message with `parts_json` containing `{"type":"error", ...}`
   - Must render red before and after reload

4. **No empty assistant placeholders**
   - No durable `[]`, empty string, or null assistant content rows
   - No empty assistant bubble in live stream or after reload

5. **Authoritative reload parity**
   - Reloaded UI must match the final live state for completed turns
   - No flicker between transient and durable variants

6. **Agent liveness guaranteed**
   - Failed terminal turns must end with `agent.idle` (or equivalent terminal state)
   - UI must not remain stuck in working/streaming

---

## Layer model

- **Pi raw stream**: `PiEvent::*`, `AssistantMessageEvent::*`
- **Canonical bus**: `stream.*`, `retry.*`, `agent.error`, `agent.idle`
- **Persistence (hstry)**: durable conversation rows
- **Frontend**:
  - transient turn state/banner (`error` state, retry text)
  - message timeline (durable + optimistic streamed)

Design rule: if data should survive reload, it must come from durable hstry rows in canonical shape.

---

## Event matrix (expected behavior)

## 0) Normal success (no retry)

Trigger: model returns valid completion.

Expected flow:
1. `agent.working`
2. `stream.message_start` + deltas (`stream.text_delta`, tool events, etc.)
3. `stream.message_end` (assistant)
4. `stream.done`
5. `agent.idle`

Persistence:
- Append user + assistant message rows
- Assistant parts are normal (`text`, `thinking`, `tool_*`)

UI:
- No error banner
- One assistant response bubble

---

## 1) Recoverable retry attempt(s), then success

Trigger: upstream fails attempt N, auto-retry configured, later attempt succeeds.

Expected flow:
1. `retry.start(attempt=1,...)` -> transient banner only
2. optional per-attempt `agent.error(recoverable=true)` (transient only)
3. repeat retry.start/recoverable errors for more attempts
4. `retry.end(success=true)`
5. continue streaming success response
6. `agent.idle`

Persistence:
- **No durable error row** for recoverable attempts
- Persist only final successful assistant output for the turn

UI:
- Retry banner updates attempt counts
- Banner clears on success
- No red persistent error bubble

---

## 2) Retry exhausted -> terminal generation failure

Trigger: all retry attempts fail (e.g. mock/server-error or mock/rate-limit exhaustion).

Expected flow:
1. retry progress events as above
2. `retry.end(success=false, final_error=...)`
3. `agent.error(recoverable=false, error=final_error)`
4. `agent.idle`

Persistence:
- Append **exactly one** terminal assistant error row with part type `error`
- Do not append normal assistant text duplicate for the same terminal failure
- Do not append per-attempt retry errors

UI:
- Retry banner may show while retrying
- Final state shows one red error message in timeline
- Reload shows same single red error message

---

## 3) Immediate terminal error (non-retry path)

Trigger: fatal error without retry loop (e.g. session/runtime fatal).

Expected flow:
1. `agent.working` (optional)
2. `agent.error(recoverable=false, ...)`
3. `agent.idle`

Persistence:
- Exactly one terminal assistant error row (`type:error`)

UI:
- One red persistent error message
- No duplicate plain-text assistant error row

---

## 4) Session-not-found / channel-closed recovery path

Trigger: session lost during send.

Expected flow:
1. `agent.error(...SessionNotFound...)`
2. recovery path may recreate session + refetch

Persistence:
- No duplicate terminal rows during recovery attempts
- If final send fails terminally: one durable error row

UI:
- If failure happened while idle/background, suppress noisy user-visible error
- If in-flight send fails terminally, show one user-meaningful error path

---

## 5) Watchdog timeout (frontend recovery)

Trigger: no terminal events received in watchdog window.

Expected flow:
1. local timeout guard triggers resync/fetch
2. eventually `agent.idle` or recovery state

Persistence:
- Prefer durable backend-side terminal status when available
- Frontend fallback message should not duplicate durable terminal errors

UI:
- transient warning allowed
- must not leave stuck spinner or duplicate timeline rows

---

## 6) Tool call errors (not model generation failure)

Trigger: tool execution fails but turn may continue.

Expected flow:
- `tool.end(is_error=true)` inside assistant message lifecycle

Persistence/UI:
- tool_result part marked error within the assistant message
- should not emit global terminal `agent.error` unless generation itself fails terminally

---

## Persistence rules (hstry)

For terminal generation error rows:
- `role = assistant`
- `parts_json = [{"type":"error","text":"..."}]` (id optional but stable is preferred)
- `content` may mirror text for searchability, but renderer should use parts
- `idx` strictly append-only and monotonic

Forbidden rows:
- assistant rows with empty/placeholder content (`[]`, empty string, null)
- mixed duplicate terminal rows (`type:text` + `type:error`) for same failure

---

## Frontend rendering contract

1. Transient retry/error state is split in two scopes:
   - **Turn-local persistent (frontend only):** non-terminal retry attempt history for the current turn
   - **Conversation durable (hstry):** terminal outcome messages only
2. Timeline errors come from durable error parts (or explicitly typed equivalent), rendered red.
3. On `agent.error(recoverable=true)`, frontend may update turn-local retry history, but must not append durable-like assistant timeline rows.
4. On `agent.error(recoverable=false)`, frontend should converge to one durable terminal error row from hstry.
5. On `agent.idle`, authoritative fetch/reconcile should converge to one canonical representation.

---

## Trace capture workflow (for regressions)

When retry/error behavior diverges from this contract, capture all layers in one run:

1. Enable runner stream tracing and restart services:
   - `just restart-debug` (local)
   - or `just deploy-host-debug <host>` (deploy path)
2. Run end-to-end repro script:
   - `just trace-retry-e2e`
3. Artifacts are written under `/tmp/oqto-e2e-trace-<timestamp>/`:
   - frontend screenshots
   - frontend console logs
   - runner stream trace JSONL (`pi.raw_line`, `pi.parsed_message`, `pi.event`, `runner.canonical_payload`)
4. Correlate with hstry rows for the created conversations.

This gives a deterministic `Pi -> runner -> frontend -> hstry` timeline for debugging duplicate user rows, missing transient retry errors, and wrong terminal error rendering.

## State-machine mapping

The chat state machine enforces transport/turn transitions, while this matrix defines semantics.

- `retry.start` -> keep `turn.kind` in `streaming` (or `error` with `recoverable=true` if no stream), update turn-local retry history.
- `agent.error(recoverable=true)` -> `turn.kind = error` but `syncRequired = false` and `durableExpected = false`.
- `agent.error(recoverable=false)` -> `turn.kind = error` with `syncRequired = true` and `durableExpected = true`.
- `agent.idle` after terminal path -> `syncing -> idle` convergence.

Implementation note: state machine likely needs explicit error metadata (`errorKind`, `durableExpected`, `syncRequired`) to avoid ad-hoc UI decisions in event handlers.

## Surgical regression test matrix (to implement)

Backend unit/integration:
- `retry_recoverable_attempts_do_not_persist_error_rows`
- `retry_exhausted_persists_single_terminal_error_row`
- `terminal_error_persists_error_part_not_text_part`
- `agent_end_filters_empty_assistant_placeholders`

Frontend hook/state tests:
- `agent_error_recoverable_persists_in_turn_retry_history_no_timeline_row`
- `agent_error_terminal_results_single_error_timeline_row_after_sync`
- `reload_keeps_single_red_terminal_error`
- `no_empty_assistant_bubble_on_error_stream_done_idle`

E2E (mock provider):
- `mock/server-error`: one red terminal message only; no duplicates; no stuck working
- `mock/rate-limit`: retry banner transient + one final red message if exhausted
- both scenarios verify before and after reload parity

---

## How to use this document

- Any bugfix touching retry/error flow must update tests against this contract.
- If behavior deviates intentionally, update this doc first and justify why.
- Keep matrix scenario names aligned with test names for fast triage.
