# Message Sync Invariants: Pi JSONL -> hstry -> Frontend

## Status: RFC (Request for Comments)
## Date: 2026-03-13

---

## 1. Problem Statement

Users intermittently lose messages on the frontend. The agent's response is available in Pi's JSONL session file but never appears in the UI. This is the most critical path in the entire system -- **every message Pi writes must be visible to the user**.

The root cause is that we have **three independent sources of truth** with no formal consistency protocol:

1. **Pi JSONL** -- append-only log, authoritative ground truth
2. **hstry (SQLite via gRPC)** -- secondary store, used for cross-session queries
3. **Frontend state** -- ephemeral, built from streaming events + REST fetches

Any desync between these three is a user-visible bug.

---

## 2. Current Architecture (Data Flow)

```
Pi Process (agent)
  |
  |-- writes JSONL to disk (append-only, authoritative)
  |-- emits events to stdout (streaming)
  |
  v
Runner (pi_manager.rs / pi_translator.rs)
  |
  |-- reads stdout line by line
  |-- translates PiEvent -> canonical EventPayload (PiTranslator)
  |-- on AgentEnd: calls persist_to_hstry_grpc() BEFORE broadcasting
  |-- broadcasts canonical events via tokio::broadcast
  |
  v
Backend (ws_multiplexed.rs)
  |
  |-- forwards canonical events to WebSocket as WsEvent::Agent
  |-- handles get_messages: tries cache -> hstry -> runner -> Pi live
  |
  v
Frontend (useChat.ts)
  |
  |-- receives streaming events, builds messages incrementally
  |-- on agent.idle: fetches state, optionally fetches history
  |-- on session switch: fetchHistoryMessages() from REST API
  |-- mergeServerMessages() with "authoritative" or "partial" mode
```

## 3. Identified Failure Modes

### FM-1: Broadcast Channel Overflow (Lagged Receiver)

**Where**: `tokio::broadcast` channel in runner, size = 256 events

**Scenario**: Pi emits events faster than the WebSocket can consume them (e.g., rapid text_delta bursts during long tool outputs). `broadcast::Receiver::recv()` returns `Lagged(n)`, dropping `n` events.

**Impact**: Missing text deltas, missing tool results, missing agent_end/agent_idle. Frontend never learns the turn completed.

**Current mitigation**: `stream.resync_required` event is emitted, triggering a full refetch. But this is reactive, not preventive.

**Severity**: HIGH -- this is likely the primary cause of lost messages.

### FM-2: hstry Persist Race with agent.idle

**Where**: `pi_manager.rs` stdout reader task

**Scenario**: (Previously fixed) hstry persist happened AFTER broadcasting agent.idle. Frontend switching sessions during idle would fetch stale hstry data. Current code persists BEFORE broadcast, but the fix depends on sequential execution within a single tokio task.

**Residual risk**: The `incremental_persist` spawns `tokio::spawn` tasks that run concurrently. The `hstry_persist_lock` mutex serializes them, but if the lock is contended, the AgentEnd persist can be delayed.

**Severity**: MEDIUM -- mostly fixed, but lock contention under load could reintroduce.

### FM-3: Compaction Index Collision

**Where**: `persist_to_hstry_grpc()` in pi_manager.rs

**Scenario**: Pi compacts context (removes old messages, renumbers). The compacted window has fewer messages than hstry. If we naively upsert by idx, we overwrite detailed original messages with compaction summaries.

**Current mitigation**: Skip message persist when `messages.len() < existing_count`, then call `reconcile_hstry_with_jsonl_tail()` to append any JSONL messages that hstry missed.

**Residual risk**: The reconciliation depends on JSONL being available and parseable. If the JSONL file is being written to concurrently, we might read a partial last line.

**Severity**: MEDIUM

### FM-4: Pi stdout Concatenation

**Where**: `PiMessage::parse()` in types.rs

**Scenario**: Pi flushes multiple JSON objects on a single line when the output buffer fills at 4096 bytes. Single `serde_json::from_str` fails with "trailing characters".

**Current mitigation**: `PiMessage::parse_all()` uses `StreamDeserializer`. But the runtime code in `LocalPiProcess::stdout_reader_task()` still calls `PiMessage::parse()` (singular), not `parse_all()`.

**Severity**: HIGH -- **this is a live bug**. The local runtime drops concatenated events silently.

### FM-5: WebSocket Disconnect During Streaming

**Where**: `forward_pi_events()` in ws_multiplexed.rs

**Scenario**: WebSocket disconnects mid-stream. Events continue to be produced by Pi but nobody is listening. On reconnect, the frontend subscribes again but the runner's broadcast channel has already sent (and possibly dropped) the events.

**Impact**: Messages that were streamed during the disconnect window are lost.

**Current mitigation**: `stream.resync_required` + `fetchHistoryMessages()`. But this depends on hstry having been persisted, which only happens on AgentEnd.

**Severity**: HIGH

### FM-6: get_messages Cache Staleness

**Where**: `PI_MESSAGES_CACHE` in ws_multiplexed.rs

**Scenario**: Cache TTL is 15 minutes. If a user opens a session, the cache is populated. Then the agent runs and finishes. The user switches away and back within 15 minutes. The cache returns stale messages (missing the latest agent response).

**Current mitigation**: Cache is bypassed during active streaming (`is_active` check), and invalidated when `age <= 2s`. But sessions that become inactive between cache population and access serve stale data.

**Severity**: MEDIUM

### FM-7: hstry External ID Mismatch

**Where**: `hstry_external_id` in pi_manager.rs

**Scenario**: Session starts with Oqto UUID as external_id. Pi reports its native session ID via `get_state` response. The reader task updates `hstry_external_id`. But if `AgentEnd` fires before `get_state` returns, messages are persisted under the Oqto UUID. Later reads use Pi's native ID and find nothing.

**Current mitigation**: Proactive `get_state` on first event. But this is a race -- if Pi processes a prompt very fast, AgentEnd can arrive before get_state response.

**Severity**: MEDIUM

### FM-8: Frontend Deferred Messages Discarded

**Where**: `useChat.ts` line ~827

**Scenario**: During streaming, `get_messages` responses are deferred (not applied). On `agent.idle`, deferred messages are explicitly discarded ("may be incomplete -- fetched mid-stream before all messages were persisted"). The idea is that streaming already built the correct state. But if streaming events were dropped (FM-1), the deferred messages were the backup, and now both are gone.

**Severity**: HIGH -- this creates a double-failure mode with FM-1.

---

## 4. Formal Sync Invariants

We define three invariants that MUST hold at all times:

### Invariant I1: JSONL Completeness

```
For any session S and time T:
  |Pi_JSONL(S, T)| >= |hstry(S, T)| >= |Frontend(S, T)|
```

Pi's JSONL is the authoritative source. hstry is a derived view. The frontend is a derived view of hstry + streaming events. No derived view can have MORE messages than its source.

### Invariant I2: Eventual Consistency

```
For any session S, there exists a bounded time delta D such that:
  after time T + D: hstry(S) = Pi_JSONL(S) AND Frontend(S) = hstry(S)
```

After any operation completes, all three stores must converge within bounded time D. We target D = 5 seconds.

### Invariant I3: Monotonic Message Delivery

```
For any session S and message sequence [m1, m2, ... mn]:
  If the frontend has displayed mi, then for all j < i, mj has been displayed.
```

Messages must never appear out of order. The frontend must never show a later message while missing an earlier one.

---

## 5. Provably Correct Sync Protocol

### 5.1 Core Mechanism: Version Vectors

Instead of relying on event-driven eventual consistency, introduce a **version vector** that provides a monotonic, comparable state identifier.

```rust
/// A version vector for a session's message state.
/// This is the single source of truth for "how many messages does this session have?"
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageVersion {
    /// Monotonically increasing counter. Incremented on every message
    /// append/update in hstry. This is NOT the message count -- it's a
    /// logical clock that increases on any mutation.
    pub version: u64,
    /// Number of messages in the session.
    pub message_count: u32,
    /// Hash of the last message's content (for quick divergence detection).
    pub last_message_hash: u64,
}
```

**Flow**:

1. **hstry** maintains a `version` counter per conversation. Every `append_messages` or `write_conversation` increments it atomically.

2. **Runner** includes the `MessageVersion` in every `agent.idle` event:
   ```json
   {
     "event": "agent.idle",
     "session_id": "...",
     "message_version": { "version": 42, "message_count": 15, "last_message_hash": 12345 }
   }
   ```

3. **Frontend** compares its local version with the received version:
   - If `local.version == server.version`: no action needed (in sync)
   - If `local.version < server.version`: fetch messages from hstry (authoritative)
   - If `local.version > server.version`: impossible (bug -- local has more than server)

4. **On session load/switch**: Frontend always fetches messages from hstry REST endpoint, which returns `MessageVersion` alongside messages. This becomes the baseline.

### 5.2 Proof Sketch: No Message Loss

**Claim**: Under this protocol, if Pi writes a message to JSONL and the runner persists it to hstry, the frontend will display it within bounded time D.

**Proof**:

1. Pi writes message M to JSONL (authoritative, append-only)
2. Pi emits AgentEnd with M in the message array
3. Runner calls `persist_to_hstry_grpc()` which calls `append_messages()` on hstry
4. hstry atomically increments `version` and stores M
5. Runner broadcasts `agent.idle` with the new `MessageVersion`
6. Frontend receives `agent.idle`:
   - Case A: Frontend already has M from streaming events, AND `local.version == server.version` -> done
   - Case B: Frontend missed streaming events (FM-1), `local.version < server.version` -> fetches from hstry -> gets M -> done
   - Case C: Frontend was disconnected (FM-5), reconnects, gets resync event -> fetches from hstry -> gets M -> done

In all cases, M is eventually displayed. QED.

**The key insight**: The version vector makes the "am I in sync?" check O(1) and deterministic. No heuristics, no timeouts, no "maybe we should refetch just in case."

### 5.3 Handling Compaction

When Pi compacts, message_count can decrease. The version still increases. The frontend sees `server.version > local.version` and refetches. The refetch returns the complete hstry (which preserves pre-compaction messages via the reconcile mechanism). Invariant I1 holds because hstry never deletes messages on compaction -- it only skips overwrites.

### 5.4 Handling Broadcast Overflow (FM-1)

The `tokio::broadcast` channel overflow is the root cause of most failures. Two fixes:

**Fix A: Replace broadcast with unbounded mpsc per subscriber**

Instead of a shared `broadcast::channel` that drops events when any subscriber is slow, give each WebSocket connection its own `mpsc::unbounded_channel`. Events are cloned into each subscriber's channel. No backpressure, no drops.

**Cost**: Memory proportional to (events_in_flight * num_subscribers). Acceptable for our scale (1-10 concurrent connections per user).

**Fix B: Keep broadcast but detect and recover**

Keep broadcast, but on `Lagged(n)`:
1. Emit `stream.resync_required` with `dropped_count: n`
2. Include the current `MessageVersion` in the resync event
3. Frontend compares versions and fetches if needed

Fix A is cleaner. Fix B is the current approach but needs the version vector to be reliable.

**Recommendation**: Fix A (mpsc per subscriber) for the event pipeline. Broadcast is the wrong primitive for a system where message loss is unacceptable.

---

## 6. Test Strategy

### 6.1 Unit Tests (Rust)

#### T1: PiTranslator Bijection Test

**Property**: For every possible PiEvent, `translate()` produces a non-empty `Vec<EventPayload>` that can be round-tripped back to a CanonMessage.

```rust
#[test]
fn translator_produces_output_for_all_event_types() {
    let mut translator = PiTranslator::new();
    for event in all_pi_event_variants() {
        let payloads = translator.translate(&event);
        assert!(!payloads.is_empty(),
            "PiEvent::{:?} produced no canonical events", event);
    }
}
```

#### T2: hstry Convert Round-Trip Test

**Property**: `AgentMessage -> proto Message -> SerializableMessage` preserves all semantic content.

```rust
#[test]
fn agent_message_roundtrip_preserves_content() {
    let original = test_agent_message_with_all_fields();
    let proto = agent_message_to_proto(&original, 0);
    let serializable = SerializableMessage::from(&proto);

    assert_eq!(serializable.role, "assistant");
    assert!(!serializable.content.is_empty());
    assert!(!serializable.parts_json.is_empty());

    // Parse parts_json back and verify structure
    let parts: Vec<serde_json::Value> = serde_json::from_str(&serializable.parts_json).unwrap();
    assert!(parts.iter().any(|p| p["type"] == "text"));
}
```

#### T3: Message Index Monotonicity Test

**Property**: Messages persisted to hstry always have strictly increasing indices.

```rust
#[test]
fn persisted_indices_are_monotonically_increasing() {
    // Simulate a sequence of persist calls with varying message counts
    // (normal, compacted, extended) and verify hstry idx values are
    // always monotonically increasing and never collide.
}
```

#### T4: parse_all Concatenated JSON Test

**Property**: `PiMessage::parse_all()` correctly splits N concatenated JSON objects.

```rust
#[test]
fn parse_all_handles_arbitrary_concatenation_counts() {
    for n in 1..=10 {
        let event = make_event();
        let concat = event.repeat(n);
        let results = PiMessage::parse_all(&concat);
        assert_eq!(results.len(), n);
        assert!(results.iter().all(|r| r.is_ok()));
    }
}
```

#### T5: stdout_reader Uses parse_all (Regression)

**Property**: The local runtime's stdout reader calls `parse_all`, not `parse`.

This is a **code-level invariant test** -- essentially a linter. Verify that `LocalPiProcess::stdout_reader_task` and `RunnerPiProcess::process_line` use `parse_all`.

#### T6: Compaction Guard Test

**Property**: When Pi has fewer messages than hstry (compaction), `persist_to_hstry_grpc` does NOT overwrite existing messages.

```rust
#[tokio::test]
async fn compaction_does_not_overwrite_existing_messages() {
    let client = MockHstryClient::with_messages(20);
    let pi_messages = make_agent_messages(5); // compacted

    persist_to_hstry_grpc(&client, "session-1", "oqto-1", "runner-1",
                          &pi_messages, Path::new("/tmp"), None).await.unwrap();

    // Verify original 20 messages are untouched
    assert_eq!(client.message_count("session-1"), 20);
}
```

### 6.2 Integration Tests (Rust)

#### T7: Full Pipeline Test: Pi Event -> hstry -> Canonical Message

**Setup**: Mock Pi process that emits a known sequence of events to stdout.

```rust
#[tokio::test]
async fn full_pipeline_persists_all_messages() {
    let (pi_process, event_stream) = MockPiProcess::new();

    // Simulate a complete conversation
    pi_process.emit(PiEvent::AgentStart);
    pi_process.emit(PiEvent::MessageStart { message: user_message("hello") });
    pi_process.emit(PiEvent::MessageEnd { message: user_message("hello") });
    pi_process.emit(PiEvent::MessageStart { message: assistant_message("hi there") });
    pi_process.emit(PiEvent::MessageEnd { message: assistant_message("hi there") });
    pi_process.emit(PiEvent::AgentEnd { messages: vec![
        user_message("hello"),
        assistant_message("hi there"),
    ]});

    // Wait for persist
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify hstry has both messages
    let hstry_messages = hstry_client.get_messages("session-1", None, None).await.unwrap();
    assert_eq!(hstry_messages.len(), 2);
    assert_eq!(hstry_messages[0].role, "user");
    assert_eq!(hstry_messages[1].role, "assistant");

    // Verify canonical events were broadcast
    let events: Vec<_> = event_stream.collect().await;
    assert!(events.iter().any(|e| matches!(e.payload, EventPayload::AgentIdle { .. })));
}
```

#### T8: Broadcast Overflow Recovery Test

```rust
#[tokio::test]
async fn broadcast_overflow_triggers_resync() {
    let (event_tx, _) = broadcast::channel::<CanonicalEvent>(4); // tiny buffer

    // Emit 10 events (overflow guaranteed)
    for i in 0..10 {
        let _ = event_tx.send(make_event(i));
    }

    // Subscribe late
    let mut rx = event_tx.subscribe();
    match rx.recv().await {
        Err(broadcast::error::RecvError::Lagged(n)) => {
            assert!(n > 0);
            // Verify resync_required would be emitted
        }
        _ => panic!("Expected lagged error"),
    }
}
```

#### T9: Version Vector Consistency Test

```rust
#[tokio::test]
async fn version_vector_increments_on_every_mutation() {
    let hstry = TestHstryClient::new();

    let v1 = hstry.get_version("session-1").await;
    hstry.append_messages("session-1", vec![msg1]).await;
    let v2 = hstry.get_version("session-1").await;
    hstry.append_messages("session-1", vec![msg2]).await;
    let v3 = hstry.get_version("session-1").await;

    assert!(v2.version > v1.version);
    assert!(v3.version > v2.version);
    assert_eq!(v3.message_count, v1.message_count + 2);
}
```

### 6.3 End-to-End Tests (Frontend + Backend)

#### E2E-1: Message Delivery Under Normal Conditions

```typescript
test('every Pi response appears on frontend', async () => {
  // 1. Open session
  // 2. Send prompt
  // 3. Wait for agent.idle
  // 4. Verify message count matches Pi JSONL
  // 5. Verify message content matches Pi JSONL
});
```

#### E2E-2: Message Delivery After Reconnect

```typescript
test('messages survive WebSocket reconnect', async () => {
  // 1. Send prompt
  // 2. While streaming, kill WebSocket connection
  // 3. Wait for auto-reconnect
  // 4. Wait for agent.idle
  // 5. Verify all messages present (from hstry fetch)
});
```

#### E2E-3: Message Delivery After Session Switch

```typescript
test('switching sessions loads complete history', async () => {
  // 1. Send prompt in session A, wait for idle
  // 2. Switch to session B
  // 3. Switch back to session A
  // 4. Verify all messages from session A are present
  // 5. Verify message_version matches server
});
```

#### E2E-4: Message Count Invariant

```typescript
test('frontend message count always matches hstry', async () => {
  // After every agent.idle event:
  // 1. GET /api/chat-history/{session}/messages -> count N
  // 2. Assert frontend displays exactly N messages
  // This is the single most important invariant test.
});
```

#### E2E-5: Compaction Does Not Lose Messages

```typescript
test('compaction preserves all messages on frontend', async () => {
  // 1. Send many prompts to fill context window
  // 2. Trigger compaction (auto or manual)
  // 3. Verify frontend still shows all pre-compaction messages
  // 4. Verify hstry still has all pre-compaction messages
});
```

#### E2E-6: JSONL Is Superset Of hstry

```bash
#!/usr/bin/env bash
# Periodic health check: compare JSONL and hstry message counts
# for all active sessions.
for session_file in ~/.pi/agent/sessions/**/*.jsonl; do
    session_id=$(basename "$session_file" .jsonl | cut -d_ -f2-)
    jsonl_count=$(grep -c '"role"' "$session_file")
    hstry_count=$(hstry query "SELECT COUNT(*) FROM messages WHERE conversation_id IN (SELECT id FROM conversations WHERE external_id='$session_id')")
    if [ "$hstry_count" -lt "$jsonl_count" ]; then
        echo "DESYNC: session $session_id has $jsonl_count in JSONL but $hstry_count in hstry"
    fi
done
```

---

## 7. Concrete Implementation Plan

### Phase 1: Fix Live Bugs (Immediate)

1. **Fix FM-4**: Change `LocalPiProcess::stdout_reader_task()` and `RunnerPiProcess::process_line()` to use `PiMessage::parse_all()` instead of `PiMessage::parse()`.

2. **Fix FM-8**: On `agent.idle`, if streaming events were dropped (detected via missing text or tool results), DO NOT discard deferred messages. Instead, apply them.

### Phase 2: Version Vector (1-2 days)

1. Add `version` column to hstry `conversations` table (auto-increment trigger)
2. Runner reads version after `persist_to_hstry_grpc()` and includes in `agent.idle`
3. Frontend compares local vs server version on every `agent.idle`
4. If mismatched: authoritative fetch from hstry

### Phase 3: Replace Broadcast with Per-Subscriber Channels (1 day)

1. Replace `tokio::broadcast` in runner with `Vec<mpsc::UnboundedSender<CanonicalEvent>>`
2. Each WebSocket subscription gets its own unbounded channel
3. Remove `stream.resync_required` (no longer needed -- events never dropped)

### Phase 4: Continuous Verification (Ongoing)

1. Health check cron job comparing JSONL vs hstry counts (E2E-6)
2. Frontend telemetry: report version mismatches as metrics
3. E2E test suite in CI running E2E-1 through E2E-5

---

## 8. Mathematical Proof: Convergence Bound

**Theorem**: Under the version vector protocol with per-subscriber channels, the system converges within `D = T_persist + T_broadcast + T_render` where:
- `T_persist` = time to write to hstry (< 50ms for SQLite)
- `T_broadcast` = time to clone event to all subscribers (< 1ms)
- `T_render` = time for frontend to process event and re-render (< 100ms)

**Total D < 200ms** under normal conditions.

**Proof by construction**:

1. Pi emits event E at time t0.
2. Runner receives E at t0 + epsilon (stdout pipe, negligible).
3. Runner persists to hstry: completes at t0 + T_persist.
4. Runner broadcasts canonical event: all subscribers receive at t0 + T_persist + T_broadcast.
5. Frontend processes event: rendered at t0 + T_persist + T_broadcast + T_render.

Since channels are unbounded per-subscriber, step 4 never blocks or drops.
Since hstry persist happens before broadcast (step 3 before step 4), any fetch after agent.idle sees the latest data.
Since version vectors are monotonic, stale fetches are always detectable and retriable.

**Failure mode**: If hstry gRPC is down, step 3 fails. The runner still broadcasts streaming events (text deltas, tool calls), so the frontend shows the response in real-time. When hstry recovers, the next `agent.idle` triggers a version check and reconciliation. **Message loss requires BOTH hstry failure AND event channel failure simultaneously** -- a compound failure that is detectable and alertable.

---

## 9. Summary of Changes

| Change | Fixes | Effort | Impact |
|--------|-------|--------|--------|
| Use `parse_all` in stdout readers | FM-4 | 30 min | HIGH |
| Per-subscriber unbounded channels | FM-1, FM-5, FM-8 | 1 day | CRITICAL |
| Version vector in hstry | FM-2, FM-6 | 1-2 days | HIGH |
| Invalidate cache on agent.idle | FM-6 | 30 min | MEDIUM |
| Unit tests T1-T6 | Regression | 1 day | HIGH |
| Integration tests T7-T9 | Regression | 1 day | HIGH |
| E2E tests E2E-1 to E2E-6 | Regression | 2 days | CRITICAL |
| Health check script | FM-3 | 1 hour | MEDIUM |
