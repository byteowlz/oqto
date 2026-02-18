# Rekey Session Bug Fix

## Issue

When the frontend creates a session with a provisional ID (e.g., `pending-123`) and Pi (the agent harness) returns its own real session ID (e.g., `pi-abc456`), the frontend must migrate all internal tracking from the provisional ID to the real ID while preserving streaming state and messages. This process is called "rekeying."

### Root Cause

The `rekeyedFromRef` ref was set when a rekey was detected (in the `get_state` response handler), but it was only cleared in one specific code path:

```typescript
// Only cleared when there are cached messages
if (previousId && rekeyedFromRef.current === previousId && messagesRef.current.length > 0) {
    rekeyedFromRef.current = null;
}
```

This caused several issues:

1. **Stale ref persists**: If there were no cached messages, the ref was never cleared
2. **False positives in `isReKeyDuringStreaming`**: The stale ref could cause the check to incorrectly return `true` when switching to a completely different session
3. **Lost or incorrect state**: When switching sessions, streaming state and messages could be incorrectly preserved or cleared

### Example Bug Scenario

1. User creates session with provisional ID `pending-123`
2. Pi responds with real ID `pi-abc456`
3. `get_state` response triggers rekey: `rekeyedFromRef.current = "pending-123"`
4. User immediately switches to another session before messages are cached
5. Later, user returns to a session, but `rekeyedFromRef` still has `"pending-123"`
6. `isReKeyDuringStreaming` check incorrectly thinks this is a rekey scenario
7. Streaming state is NOT reset, causing incorrect UI state

## Solution

### Fix 1: Always clear `rekeyedFromRef` when session ID changes

In `frontend/features/chat/hooks/useChat.ts`, added logic to clear the ref when the session ID changes, regardless of whether there are cached messages:

```typescript
// Clear the rekey ref if the session ID changed.
// This must happen BEFORE the isReKeyDuringStreaming check
// to avoid false positives when switching to a different session.
// The ref is only relevant for the specific previousId -> activeSessionId transition.
if (previousId && previousId !== activeSessionId && rekeyedFromRef.current === previousId) {
    rekeyedFromRef.current = null;
    // Also clean up session aliases to prevent memory leaks
    const alias = sessionAliasRef.current.get(previousId);
    if (alias) {
        sessionAliasRef.current.delete(previousId);
        sessionAliasRef.current.delete(alias);
    }
}
```

### Fix 2: Clear session aliases on session close

In `frontend/lib/ws-manager.ts`, added cleanup of session aliases when a session is closed:

```typescript
agentCloseSession(sessionId: string, id?: string): void {
    this.subscribedSessions.delete(sessionId);
    this.sessionReady.delete(sessionId);
    this.pendingSubscriptions.delete(sessionId);
    this.pendingMessages.delete(sessionId);
    this.agentSessionHandlers.delete(sessionId);

    // Clean up session aliases to prevent memory leaks
    const alias = this.sessionAliases.get(sessionId);
    if (alias) {
        this.sessionAliases.delete(sessionId);
        this.sessionAliases.delete(alias);
    }

    this.send({
        channel: "agent",
        session_id: sessionId,
        cmd: "session.close",
        id,
    });
}
```

### Fix 3: Clear refs in `newSession` and `resetSession`

In `frontend/features/chat/hooks/useChat.ts`, added cleanup of rekey tracking when creating a new session or resetting an existing one:

```typescript
const newSession = useCallback(async () => {
    // Clear local state
    setMessages([]);
    streamingMessageRef.current = null;
    isStreamingRef.current = false;
    setIsStreaming(false);
    setIsAwaitingResponse(false);
    setError(null);
    messageIdRef.current = 0;
    // Clear rekey tracking since we're creating a completely new session
    rekeyedFromRef.current = null;
    sessionAliasRef.current.clear();
    await ensureSession();
}, [ensureSession]);
```

Similar cleanup added to `resetSession`.

## Testing

Created comprehensive test suite in `frontend/tests/rekey-session.test.ts` that covers:

- Setting and clearing of `rekeyedFromRef`
- Detection of rekey during streaming
- Session alias management
- Bug scenarios and their fixes

All tests pass.

## Known Limitations

If a provisional ID is reused for multiple rekeys, some stale one-way alias entries may remain in the `sessionAliasRef` Map. This is a minor issue because:

1. Stale entries point to IDs that don't exist in other maps
2. They don't affect the current session's operation
3. They'll be cleaned up on page refresh or app restart

This could be addressed in a future improvement by tracking aliases with timestamps and periodically cleaning up stale entries.

## Files Changed

- `frontend/features/chat/hooks/useChat.ts`: Fixed rekeyedFromRef lifecycle, added cleanup in newSession and resetSession
- `frontend/lib/ws-manager.ts`: Added session alias cleanup in agentCloseSession
- `frontend/tests/rekey-session.test.ts`: Added comprehensive test suite for rekey functionality

## Related Code

- `backend/crates/oqto/src/runner/pi_manager.rs`: Backend rekey migration logic (`migrate_hstry_conversation_on_rekey`)
- `frontend/features/chat/hooks/useChat.ts`: Frontend rekey detection and handling
- `frontend/lib/ws-manager.ts`: WebSocket manager with session alias tracking
