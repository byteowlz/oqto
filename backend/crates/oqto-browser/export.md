# Session: oqto-browser socket connection error

**Agent:** OpenCode  
**Session ID:** `ses_424f9e460ffeaAl9cB0ZDjcxg0`  
**Created:** 2026-01-20 11:49:29  
**Duration:** 1.1m  

---

## Message 1: Assistant
**Timestamp:** 11:49:29

The error shows that `oqto-browser` expects a daemon to be running and listening on a Unix socket. Let me explore the codebase to understand how this works and what daemon needs to be started.

### Tool Executions

#### ✅ task - "Explore oqto-browser codebase"
**Status:** ✅ Completed

**Input:**
```
{
      "description": "Explore oqto-browser codebase",
      "prompt": "Explore the oqto-browser crate to understand:\n1. How the browser daemon is supposed to be started\n2. What creates the socket at /tmp/agent-browser-default.sock\n3. Is there a separate daemon binary or server command?\n4. How does the architecture work - is this a client that connects to an external daemon?\n\nLook at:\n- Cargo.toml for dependencies and binary definitions\n- src/main.rs and any daemon/server related code\n- Any README or documentation\n- The socket path construction logic\n\nReturn a summary of:\n- How to start the daemon\n- What binary/command launches the server\n- The overall architecture",
      "subagent_type": "explore"
    }
```
**Output:**
```
Now I have a complete picture. Let me provide the summary:

---

## Summary: oqto-browser Architecture

### Overview

`oqto-browser` is a **client-only CLI** that communicates with an **external daemon** called `agent-browser`. It is NOT the daemon itself - it's a lightweight Rust CLI wrapper for talking to the daemon over Unix sockets (or TCP on Windows).

### Key Components

| Component | Location | Role |
|-----------|----------|------|
| `oqto-browser` | `/home/wismut/byteowlz/oqto/backend/crates/oqto-browser` | Rust CLI client for Oqto's integration |
| `agent-browser` | `/home/wismut/byteowlz/external-repos/agent-browser` | External Vercel project - the actual daemon + CLI |

### How the Daemon Gets Started

1. **The daemon is part of `agent-browser`** (external project), not `oqto-browser`
2. **Auto-start mechanism**: When you run any `agent-browser` command (e.g., `open`, `navigate`), the daemon starts automatically if not already running
3. **Oqto backend integration**: The `AgentBrowserManager` in `/home/wismut/byteowlz/oqto/backend/crates/oqto/src/agent_browser.rs` starts the daemon by calling:
   ```
   agent-browser --session <session_id> open about:blank
   ```

### What Creates the Socket at `/tmp/agent-browser-default.sock`

The **Node.js daemon** in `agent-browser` creates the socket. See `/home/wismut/byteowlz/external-repos/agent-browser/src/daemon.ts`:

```typescript
// Line 58: Socket path construction
return path.join(os.tmpdir(), `agent-browser-${sess}.sock`);

// Line 252-257: Server listens on Unix socket
const socketPath = getSocketPath();
server.listen(socketPath, () => {
  // Daemon is ready
});
```

### Architecture Diagram

```
+-------------------+          Unix Socket              +---------------------+
|   oqto-browser    |  ------>  /tmp/agent-browser-    |   agent-browser     |
|   (Rust CLI)      |           {session}.sock         |   (Node.js Daemon)  |
+-------------------+                                   +---------------------+
        |                                                       |
        |  JSON-over-newline protocol                           |
        |  (send command, receive response)                     v
        |                                               +---------------------+
        +-----------------------------------------------> Playwright/Chromium |
                                                        +---------------------+
```

### How to Start the Daemon

**Option 1: Automatic start (recommended)**
```bash
agent-browser open https://example.com
```
The daemon auto-launches on first command.

**Option 2: Direct daemon start (for debugging)**
```bash
AGENT_BROWSER_DAEMON=1 node dist/daemon.js
# Or
AGENT_BROWSER_SESSION=mysession agent-browser open about:blank
```

**Option 3: Via Oqto backend (production)**
Enable in `config.toml`:
```toml
[agent_browser]
enabled = true
binary = "agent-browser"  # Path to agent-browser CLI
```

The backend will call `agent-browser --session <session_id> open about:blank` when sessions start.

### Session Isolation

Each session gets isolated:
- **Socket**: `/tmp/agent-browser-{session}.sock`
- **PID file**: `/tmp/agent-browser-{session}.pid`
- **Stream port file**: `/tmp/agent-browser-{session}.stream`

Default session is `"default"` if not specified.

### Binary/Command Summary

| Binary | Purpose |
|--------|---------|
| `oqto-browser` | Oqto's Rust CLI client (this crate) |
| `agent-browser` | External CLI + daemon (Vercel project) |

The `oqto-browser` crate is essentially a **thin wrapper** that mirrors `agent-browser`'s protocol but is built in Rust for consistency with the Oqto backend. Both communicate with the same daemon using the same JSON protocol over Unix sockets.

<task_metadata>
session_id: ses_424f9c52bffeMe19f8lxW0WCQp
</task_metadata>
```

### Step-Start Part

### Step-Finish Part

---

## Message 2: Assistant
**Timestamp:** 11:50:27

Based on the exploration, the issue is clear:

**`oqto-browser` is a client-only CLI** that connects to an external daemon called `agent-browser`. The daemon isn't running, which is why you're getting the "No such file or directory" error.

## How to fix it

You need to start the `agent-browser` daemon first. It's a separate Node.js project. Try:

```bash
# Option 1: Use agent-browser directly (it auto-starts the daemon)
agent-browser open https://openshovelshack.com

# Then oqto-browser can connect to the same session
oqto-browser open https://example.com
```

If `agent-browser` isn't installed or available:

```bash
# Check if it exists
which agent-browser

# Or check the external-repos location
ls ~/byteowlz/external-repos/agent-browser/
```

If you need to build/install `agent-browser`:

```bash
cd ~/byteowlz/external-repos/agent-browser
bun install
bun run build
# Then either add to PATH or run directly
```

## Architecture Summary

| Component | Role |
|-----------|------|
| `agent-browser` | Node.js daemon + CLI that controls Playwright/Chromium |
| `oqto-browser` | Rust CLI client that talks to the daemon via Unix socket |

The socket `/tmp/agent-browser-default.sock` is created by the Node.js daemon when it starts, not by `oqto-browser`.

### Step-Start Part

### Step-Finish Part

---

