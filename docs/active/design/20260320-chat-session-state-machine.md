# Chat Session State Machine (Frontend)

Date: 2026-03-19

## Goal

Make invalid chat persistence/transport states unrepresentable in the frontend by introducing an explicit session state machine for identity, turn lifecycle, and transport epochs.

## Implementation

### New module

- `frontend/features/chat/hooks/chat-state-machine.ts`

Defines:

- `SessionIdentityState`
  - `unbound` (provisional client id)
  - `bound` (canonical runner id + optional hstry/pi ids)
- `TurnState`
  - `idle`, `sending`, `streaming`, `reconciling`, `error`
- `TransportState`
  - `disconnected`, `connecting`, `connected`, `reconnecting` with monotonic epoch
- `MessageSyncState`
  - `idle` / `syncing`, source-tagged revision counter
- `MessageSyncSource`
  - `history`, `resync`, `ws_get_messages`, `ws_messages`, `watchdog`

Transition helpers:

- `bindIdentity()`
- `resetIdentity()`
- `transitionTurn()`
- `transitionTransport()`
- `beginMessageSync()` / `completeMessageSync()`
- `selectMessageMergeMode()`
- `deriveUiFlags()`

### `useChat.ts` integration

`frontend/features/chat/hooks/useChat.ts` now uses the machine as the authority for UI streaming/waiting flags and sync orchestration:

- Added `machineRef` and `applyTurnState()`
- `isStreaming` / `isAwaitingResponse` are derived from turn state
- Added explicit identity binding in `ensureSession()`
  - alias remap (`hstry_id -> session_id`) now also binds canonical runner id
- Send path now enforces canonical runner id after `ensureSession()`
- Connection state updates now apply transport transitions with epoch tracking
- Session switch/new/reset paths reset identity and turn state
- Added `applyServerMessages()` as the only path for applying server snapshots
  - records sync source
  - advances sync revision
  - picks merge mode from state (`selectMessageMergeMode`) instead of ad-hoc logic

## Invariants now enforced

1. Non-idle turn states require a bound session identity.
2. Outbound sends resolve to canonical `runnerId` once bound.
3. Illegal turn transitions are ignored by reducer helpers.
4. Stale transport updates (older epoch) are ignored.

## Tests

- Added `frontend/tests/chat-state-machine.test.ts`
  - rejects non-idle transitions while unbound
  - validates canonical idle→sending→streaming→reconciling→idle lifecycle
  - validates transport epoch monotonicity
  - validates sync merge-mode selection (`partial` during streaming ws_get_messages)
  - validates sync revision lifecycle
  - validates identity reset behavior
