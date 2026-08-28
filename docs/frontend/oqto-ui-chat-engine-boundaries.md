# Chat engine boundaries: legacy inventory → OqtoUI engine contract

Spike output for `oqto-a9j4.1`. Classifies the legacy chat stack into KEEP
(port behind the engine seam) vs DISCARD (replaced by durable-authority
rendering), and sketches the engine contract for
`frontend/src/oqto-ui/chat/engine/`.

## Why the legacy engine loses messages (root cause, with receipts)

The legacy `useChat` renders from a **socket-accumulated array**
(`setMessages` + `streamingMessageRef` + ~20 coordination refs). Visible
state is owned by the event stream, so any missed/duplicated/reordered
event corrupts it permanently. Concrete loss vectors found:

- `useChat.ts` `case "messages"`: server message snapshots are **discarded
  while a live turn is active** (`if (!liveTurnActive)`) — server truth is
  dropped to protect optimistic accumulation.
- `stream.message_start` creates display messages with
  `createTempMessageId()`; reconciliation to durable IDs depends on later
  events arriving correctly.
- Optimistic user messages are matched by `client_id` against later
  snapshots; a missed match duplicates or orphans the message.
- Recovery paths (`runReconnectReconcile`, `recoverSessionOnError`,
  response watchdog, streaming throttle flush) each mutate the same array
  from different triggers.

OqtoUI inverts ownership: **the timeline always renders oqto-log pages**
(durable cursors, stable IDs); stream events only decorate the page cache
until the store confirms. A dropped event then costs staleness, never loss.

## Classification

### KEEP — port behind the engine seam

| Legacy source | What it is | Engine destination |
|---|---|---|
| `lib/ws-manager.ts` (1,932 lines) | Mux transport: backoff reconnect, ping, connect timeout, epochs, per-session subscribe lifecycle, session-ready gating, command acks, persisted outbox, resync scheduling, visibility/online handling | `engine/transport.ts` (trimmed to agent channel; files/terminal/trx channels stay legacy until their views port) |
| `lib/ws-mux-types.ts` (agent subset) | Typed command/event wire contract | `engine/wire-types.ts` (import or re-declare narrow agent subset) |
| ws-manager command methods (`agentPrompt/Steer/FollowUp/Abort/Compact/SetModel/SetThinkingLevel/Fork/GetForkPoints/CycleModel/...`) | Send/cancel/config command surface with ack semantics | `engine/commands.ts` |
| `stream-tool-dispatch.ts` (326) | Delta/tool-call part assembly rules | Rewritten pure in `engine/projection.ts` (logic kept, React/ref context dropped) |
| `chat-state-machine.ts` | Turn/transport/identity state transitions (already pure, tested) | `engine/turn-machine.ts` (near-verbatim) |
| `response-command-handlers.ts` | Response payload parsers (already pure) | `engine/projection.ts` helpers |
| Watchdog concept (`armResponseWatchdog`) | No-first-token timeout surfaced to UI | `engine/turn-machine.ts` timeout input (UI decides presentation) |

### DISCARD — replaced by durable-authority rendering

| Legacy mechanism | Replacement |
|---|---|
| `setMessages` array as visible truth; `streamingMessageRef`; `messagesRef`; batched/throttled snapshot application (`StreamingThrottle`, `flushStreamingUpdate`, `applyThrottledSnapshot`) | Timeline renders `useTimeline` pages; deltas write into the page cache keyed by stable message id; React renders what the cache holds |
| `createTempMessageId()` display messages | Streaming turn renders from a single `TurnDraft` cache entry keyed by `(session_id, turn)`; never persisted, replaced by store rows on turn end |
| Optimistic user message + `client_id` matching | Composer renders the in-flight prompt locally until the store page containing it confirms (echo arrives via page refetch, matched by server id) |
| `applyServerMessages` merge heuristics + `liveTurnActive` snapshot dropping | Turn end / reconnect / doubt ⇒ invalidate page query; durable pages win unconditionally |
| `fetchHistoryMessages` REST path + session message cache | Existing `useTimeline` keyset pagination (already shipped) |
| `createPiSessionId()` client-minted session ids | Session creation returns the platform id before first send (identity rule: no `pending-*`/`tmp:*`) |
| `runReconnectReconcile` / `recoverSessionOnError` bespoke recovery | Reconnect = re-attach subscription + invalidate active page (resumability lives in the store) |
| jotai/context wiring, `use-app` coupling, title-change side channels | Platform contract callbacks |

## Engine contract sketch

```
frontend/src/oqto-ui/chat/engine/
  wire-types.ts    // agent channel commands/events (typed, no any)
  transport.ts     // connect/reconnect/subscribe/ack/outbox; emits WireEvent
  projection.ts    // pure: WireEvent -> TurnDraftUpdate | TurnLifecycle | ConfigChange
  turn-machine.ts  // pure: turn/transport/identity transitions + watchdog timeout
  engine.ts        // composes the above; public surface:
```

```ts
type ChatEngine = {
  attach(sessionId: string): Detach;            // subscribe + ready gating
  send(sessionId, text, opts): Promise<void>;    // prompt|steer|follow_up + attachments
  abort(sessionId): Promise<void>;
  setModel(sessionId, modelId): Promise<void>;
  setThinkingLevel(sessionId, level): Promise<void>;
  onTurn(cb: (u: TurnUpdate) => void): Unsub;    // draft deltas + lifecycle
  onConnection(cb: (s: ConnState) => void): Unsub;
};
```

Integration (the only stateful glue, in `useChatSession`):
- `TurnUpdate.draft` → `queryClient.setQueryData` on a `turnDraft` entry
  rendered after the last durable page row.
- `TurnUpdate.ended | resync | reconnected` → drop draft, invalidate the
  session's page query. Durable truth replaces decoration.
- Tail-follow/scroll stays in the view (native scroll anchoring,
  `oqto-a9j4.5`).

## Proof obligations carried to `oqto-a9j4.4`

- Authority: oqto-log pages via `/chat-history/{id}/messages/page`.
- Event source: ws mux agent channel (hints only).
- ID mapping: server/store ids only; drafts keyed by `(session_id, turn)`
  and never persisted.
- Reconnect/reload: re-attach + invalidate; no client reconciliation of
  history.
- Regression: kill transport mid-turn ⇒ timeline converges to store
  (test with deterministic fake transport in the scripted adapter).

## Port order

1. `wire-types.ts` + `transport.ts` (trimmed agent-channel port, fake
   transport for tests) — `oqto-a9j4.3`.
2. `projection.ts` + `turn-machine.ts` (pure, React-free tests: partial
   deltas, out-of-order tool results, duplicate events, mid-turn
   disconnect, thinking suppression) — `oqto-a9j4.2`.
3. `engine.ts` + `useChatSession` glue + composer wiring — `oqto-a9j4.4`.
4. Legacy freeze + parity grind — `oqto-a9j4.6`.
