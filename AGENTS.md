# Octo - AI Agent Workspace Platform

Octo is a self-hosted platform for managing AI coding agents. Supports local mode (native processes) and container mode (Docker/Podman).

**New to Octo?** Start with the [SETUP.md](./SETUP.md) guide for installation and prerequisites.

---

## Debugging

Tmux is always available, use it to debug the logs of the running backend and frontend.

Use agent-browser + tmux for end-to-end testing

---

## Architecture Overview

```
Frontend                          Backend                           Runner (per user)
   |                                 |                                    |
   |-- Single WebSocket ------------>|                                    |
   |   (multiplexed channels)        |                                    |
   |                                 |-- Unix/TCP socket ---------------->|
   |                                 |   (runner protocol)                |
   |                                 |                                    |
   |   {channel:"agent", ...}        |   Canonical Commands              |-- Agent Process A
   |   {channel:"files", ...}        |   Canonical Events                |-- Agent Process B
   |   {channel:"terminal", ...}     |                                   |-- hstry (gRPC)
```

### Core Components

| Component | Purpose |
|-----------|---------|
| **Frontend** | React/TypeScript app speaking the canonical protocol via multiplexed WebSocket |
| **Backend (octo)** | Stateless relay: routes commands to runners, forwards events to frontend |
| **Runner (octo-runner)** | Per-user daemon: owns agent processes, translates native events to canonical format |
| **hstry** | Chat history service (gRPC API, SQLite-backed). All reads/writes go through gRPC. |

### The Canonical Protocol

The frontend speaks a **harness-agnostic canonical protocol**. Users can select which harness to use (Pi, opencode, future agents), but the message format and UI rendering is identical regardless of harness.

- **Messages** are persistent (stored in hstry) with typed **Parts**: text, thinking, tool_call, tool_result, image, file_ref, etc.
- **Events** are ephemeral UI signals: stream.text_delta, agent.working, tool.start, agent.idle, etc.
- **Commands** flow from frontend to runner: prompt, abort, set_model, compact, fork, etc.

See `docs/design/canonical-protocol.md` for the full specification.

### Harnesses

A **harness** is an agent runtime that the runner can spawn. The runner translates the harness's native protocol into canonical format.

| Harness | Binary | Status |
|---------|--------|--------|
| **pi** | `~/.bun/bin/pi` | Primary harness |
| **opencode** | TBD | Planned |
| *(custom)* | Any RPC-compatible agent | Extensible |

Each runner advertises which harnesses it supports. The frontend shows a harness picker when creating sessions.

### Runtime Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| `local` | Direct process spawn | Single-user, development |
| `runner` | Via `octo-runner` daemon | Multi-user Linux isolation |
| `container` | Inside Docker/Podman | Full container isolation |

### Key Binaries

| Binary | Purpose |
|--------|---------|
| `octo` | Main backend server |
| `octoctl` | CLI for server management |
| `octo-runner` | Multi-user process daemon, manages agent harnesses |
| `octo-sandbox` | Sandbox wrapper using bwrap/sandbox-exec |
| `pi-bridge` | HTTP/WebSocket bridge for Pi in containers |
| `octo-files` | File access server for workspaces |
| `hstry` | Chat history daemon (gRPC, SQLite-backed) |

### Process Sandboxing

Sandbox configuration in `~/.config/octo/sandbox.toml` (separate from main config for security):

```toml
enabled = true
profile = "development"  # or "minimal", "strict"
deny_read = ["~/.ssh", "~/.aws", "~/.gnupg"]
allow_write = ["~/.cargo", "~/.npm", "/tmp"]
isolate_network = false  # true in strict profile
isolate_pid = true
```

Per-workspace overrides in `.octo/sandbox.toml` can only ADD restrictions, never remove them.

---

## Event Flow

```
Agent Harness (e.g., Pi --mode rpc, stdin/stdout JSON)
  -> Runner: stdout_reader_task()
  -> Runner: translate(NativeEvent) -> CanonicalEvent
  -> Runner: broadcast::Sender<CanonicalEvent>
  -> Backend: Unix socket / TCP
  -> Backend: WebSocket handler
  -> Frontend: multiplexed WebSocket (agent, files, terminal channels)
```

The runner maintains a state machine per session (idle, working, error) and emits canonical events. The frontend derives UI state directly from events without harness-specific logic.

---

## Storage

### hstry (Chat History)

All chat history access goes through hstry's gRPC API - no raw SQLite access from `octo`.

- **WriteService**: Persist messages after agent turns complete (via `HstryClient` gRPC)
- **ReadService**: Query messages, sessions, search (via `HstryClient` gRPC)
- Stores canonical `Message` format directly (no translation at read time)
- **Runner exception**: `octo-runner` reads hstry SQLite directly for speed (runs as target user, same machine). This is intentional and secure.

### Session Files (Pi-Owned)

Pi writes its own JSONL session files -- **Octo must NEVER create or write JSONL session files**.

- **Pi**: `~/.pi/agent/sessions/--{safe_cwd}--/{timestamp}_{session_id}.jsonl`
- These are authoritative for harness-specific metadata (titles, fork points)
- hstry is authoritative for structured message content
- `pending-` prefixed IDs are internal runner/frontend placeholders for optimistic session matching; they must never leak into files or hstry

## Agent Tools

Two CLI tools are available for agent workflows:

| Tool | Purpose |
|------|---------|
| **byt** | Cross-repo governance and management (catalog, schemas, releases) |
| **agntz** | Day-to-day agent operations (memory, issues, mail, file reservations) |
| **sx** | External searches via SearXNG (`sx "<query>" -p`) |

### agntz - Agent Operations

```bash
agntz memory search "query"     # Search memories
agntz memory add "insight"      # Add a memory
agntz ready                     # Show unblocked issues
agntz issues                    # List all issues
agntz mail inbox                # Check messages
agntz reserve src/file.rs       # Reserve file for editing
agntz release src/file.rs       # Release reservation
```

### byt - Cross-Repo Management

```bash
byt catalog list                # List all repos
byt status                      # Show repo status
byt memory search "query" --all # Search across all stores
byt sync push                   # Sync memories to git
```

---

## Memory System (Critical)

**ALWAYS search memories before starting work on unfamiliar areas.** Memories contain architecture decisions, API patterns, and debugging insights that save time.

**Create memories when you discover reusable knowledge.** Memories are for patterns, interfaces, and insights - not atomic implementation details.

### When to Create Memories

Create a memory when you discover:

- **Reusable patterns** - "Voice mode uses eaRS for STT and kokorox for TTS via WebSocket"
- **Existing interfaces** - "Pi PATCH /api/chat-history/{id} renames sessions via session_info JSONL entry"
- **Architecture decisions** - "hstry is mandatory, writes go through gRPC WriteService not raw SQLite"
- **Debugging insights** - "Port cleanup requires waiting for process exit to prevent zombies"
- **Integration points** - "PiTranslator converts PiEvent to Vec<EventPayload> for canonical broadcast"

### When NOT to Create Memories

Don't create memories for:

- Specific bug fixes in specific files
- One-off implementation details
- Things already documented in code comments
- Temporary workarounds

### Memory Commands

```bash
# Search before implementing (find existing solutions)
agntz memory search "voice mode"
agntz memory search "session rename"

# Add after discovering something reusable
agntz memory add "Chat sessions from disk need PATCH /api/chat-history/{id} since no Pi running" -c api -i 7
agntz memory add "features.voice config gates dictation and voice mode buttons" -c frontend -i 6

# Categories: api, frontend, backend, architecture, patterns, debugging
# Importance: 1-10 (7+ for significant insights)
```

### Memory Examples (Good)

```bash
agntz memory add "Chat sessions from disk need PATCH /api/chat-history/{id} since no Pi running" -c api -i 7
agntz memory add "useDictation hook provides STT-only mode separate from full voice mode" -c frontend -i 6
agntz memory add "Pi session files stored at ~/.pi/agent/sessions/--{safe_cwd}--/{ts}_{id}.jsonl" -c architecture -i 8
```

### Memory Examples (Bad)

```bash
# Too specific - this is implementation detail, not reusable knowledge
agntz memory add "Fixed bug in line 451 of app-context.tsx"

# Too vague - not actionable
agntz memory add "Voice stuff is complicated"
```

---

## Build/Lint/Test Commands

| Component | Build | Lint | Test | Single Test |
|-----------|-------|------|------|-------------|
| **backend/** | `cargo build` | `cargo clippy && cargo fmt --check` | `cargo test` | `cargo test test_name` |
| **fileserver/** | `cargo build` | `cargo clippy && cargo fmt --check` | `cargo test` | `cargo test test_name` |
| **frontend/** | `bun run build` | `bun run lint` | `bun run test` | `bun run test -t "pattern"` |

## Code Style

**Rust**: Use `anyhow::Result` with `.context()` for errors. Group imports: std, external crates, internal modules. Run `cargo fmt` and `cargo check` after changes.

**TypeScript**: Use `@/` import alias for internal modules. Functional components with named exports. Vitest for tests.

**General**: No emojis in code/docs/commits. Use `bun` for JS/TS, `uv` for Python.

---

## Issue Tracking (trx)

```bash
trx ready              # Show unblocked issues
trx create "Title" -t task -p 2   # Create issue (types: bug/feature/task/epic/chore, priority: 0-4)
trx update <id> --status in_progress
trx close <id> -r "Done"
trx sync               # Commit .trx/ changes
```

Priorities: 0=critical, 1=high, 2=medium, 3=low, 4=backlog

---

## Displaying Files to the User

To display files (images, documents, etc.) to the user in the chat interface, reference them with the `@` prefix followed by the file path:

```
Here's the generated image: @output/screenshot.png

I've created these files:
@src/components/Button.tsx
@docs/architecture.md
```

The UI will automatically render:
- **Images** (png, jpg, gif, webp, svg) as inline previews with thumbnails
- **Other files** as clickable links that open in the file viewer

Use workspace-relative paths (e.g., `@src/file.ts`) or absolute paths (e.g., `@/home/user/file.png`).
