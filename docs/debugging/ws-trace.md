# WebSocket Trace Capture

Use this when debugging streaming cutoffs, duplicate sessions, or session ID mismatches.

## Enable tracing

Open DevTools and run:

```js
localStorage.setItem("debug:ws-trace", "1");
```

Reload the page to start capturing events.

## Reproduce the issue

Perform the exact steps that trigger the bug (send messages, reload, etc.).

## Export trace

In DevTools console:

```js
window.__octoWsTraceDump?.()
```

Copy the JSON and save it to a file, for example `trace.json`.

To clear the buffer:

```js
window.__octoWsTraceClear?.()
```

## Replay trace

```bash
bun scripts/replay-ws-trace.mjs trace.json
```

The script prints a timeline and flags likely problems:
- Duplicate `session.create` calls for the same session ID
- `get_messages` responses arriving while streaming is active
