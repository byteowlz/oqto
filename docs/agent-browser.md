# Browser Daemon Integration

Octo can optionally start a Playwright-backed browser daemon per session. This provides the
backend foundation for the server-side browser feature. The daemon is shipped in-repo as
`backend/crates/octo-browserd` and is installed as `octo-browserd`.

## Configuration

Add to `config.toml`:

```toml
[agent_browser]
# Enable per-session daemon management
enabled = false
# Path to the octo browser daemon CLI
binary = "octo-browserd"
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

- The backend calls `octo-browserd --session <session_id> open about:blank` to
  start the daemon.
- The daemon is stopped with `octo-browserd --session <session_id> close` when
  the session stops or is deleted.
- Socket directories are session-scoped under `$XDG_RUNTIME_DIR/octo/agent-browser/<session_id>`
  and created with `0700` permissions (or overridden via `AGENT_BROWSER_SOCKET_DIR`).
- The browser stream WebSocket is proxied at
  `GET /api/session/{session_id}/browser/stream`.

## Frontend

- The Sessions UI exposes a Browser tab that renders the screencast stream on a
  canvas and forwards mouse/keyboard input over the WebSocket proxy.
- The toolbar uses common keyboard shortcuts to navigate (back/forward/reload)
  and focus the address bar for URL entry.

## CLI

`octo-browser` is a lightweight CLI for agents to talk to the browser daemon:

```bash
# Navigate (auto-launches browser)
octo-browser --session <session_id> open https://example.com

# Snapshot current page
octo-browser --session <session_id> snapshot

# Click or fill
octo-browser --session <session_id> click "text=Sign in"
octo-browser --session <session_id> fill "input[name=email]" "user@example.com"

# Close the daemon
octo-browser --session <session_id> close
```

For any action supported by the browser daemon, use the generic command:

```bash
# Snapshot with extra options
octo-browser --session <session_id> command snapshot --arg interactive=true --arg maxDepth=3

# Navigate with custom waitUntil
octo-browser --session <session_id> command navigate --arg url=https://example.com --arg waitUntil=\"domcontentloaded\"

# Send a JSON payload file
octo-browser --session <session_id> command click --file payload.json
```
