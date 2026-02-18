# Browser Daemon Integration

Oqto can optionally start a Playwright-backed browser daemon per session. This provides the
backend foundation for the server-side browser feature. The daemon is shipped in-repo as
`backend/crates/oqto-browserd` and is installed as `oqto-browserd`.

## Configuration

Add to `config.toml`:

```toml
[agent_browser]
# Enable per-session daemon management
enabled = false
# Path to the oqto browser daemon CLI
binary = "oqto-browserd"
# Launch headed browser windows (default is headless)
headed = false
# Base port for screencast WebSocket streams
stream_port_base = 30000
# Port range size for per-session streams
stream_port_range = 10000
# Optional Chromium executable path
# executable_path = "/usr/bin/chromium"
# Optional extensions (absolute paths)
# extensions = ["/path/to/extension"]
```

## Notes

- The backend calls `oqto-browserd --session <session_id> open about:blank` to
  start the daemon.
- The daemon is stopped with `oqto-browserd --session <session_id> close` when
  the session stops or is deleted.
- Socket directories are session-scoped under `$XDG_RUNTIME_DIR/oqto/agent-browser/<session_id>`
  and created with `0700` permissions (or overridden via `AGENT_BROWSER_SOCKET_DIR`).
- The browser stream WebSocket is proxied at
  `GET /api/session/{session_id}/browser/stream`.

## Frontend

- The Sessions UI exposes a Browser tab that renders the screencast stream on a
  canvas and forwards mouse/keyboard input over the WebSocket proxy.
- The toolbar uses common keyboard shortcuts to navigate (back/forward/reload)
  and focus the address bar for URL entry.

## CLI

`oqto-browser` is a lightweight CLI for agents to talk to the browser daemon:

```bash
# Navigate (auto-launches browser)
oqto-browser --session <session_id> open https://example.com

# Snapshot current page
oqto-browser --session <session_id> snapshot

# Click or fill
oqto-browser --session <session_id> click "text=Sign in"
oqto-browser --session <session_id> fill "input[name=email]" "user@example.com"

# Close the daemon
oqto-browser --session <session_id> close
```

For any action supported by the browser daemon, use the generic command:

```bash
# Snapshot with extra options
oqto-browser --session <session_id> command snapshot --arg interactive=true --arg maxDepth=3

# Navigate with custom waitUntil
oqto-browser --session <session_id> command navigate --arg url=https://example.com --arg waitUntil=\"domcontentloaded\"

# Send a JSON payload file
oqto-browser --session <session_id> command click --file payload.json
```
