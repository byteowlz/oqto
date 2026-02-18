# Oqto - AI Agent Workspace Platform

Oqto is a self-hosted platform for managing AI coding agents. Supports local mode (native processes) and container mode (Docker/Podman).

**New to Oqto?** Start with the [SETUP.md](./SETUP.md) guide for installation and prerequisites.

---

## IMPORTANT

- Keep this document up to date. Whenever we change functionality or the architecture, we need to also update it in here so that subsequent sessions are always aware of the current status.
- Don't keep legacy alive. This project is still in it's infancy and there is 0 need for any backward compatibility. Remove any dead or legacy code you encounter without breaking the current system. If you stumble upon parts of the system that can be deprecated, suggest how we could best do this
- Document your work: Use trx cli for epics, features, bugs etc. Use agntz memory for documenting learnings along the way. Future sessions have access to both. Your todo list is ephemeral, it's like a status bar for the current session. Use it accordingly.
- Oqto is made up of many separate tools that we are building in parallel. If you encounter bugs or potential improvements, file trx in the respective repos (e.g. ../mmry etc)
- During development, use the agent-browser for end to end debugging. You can use wismut:dev to log in. The frontend is accessible under localhost:3000

## Debugging

Tmux is always available, use it to debug the logs of the running backend and frontend.

### agent-browser (headless browser testing)

Requires `DISPLAY=:0` prefix on all commands (X server runs on :0).

```bash
DISPLAY=:0 agent-browser open http://localhost:3000      # Open page
DISPLAY=:0 agent-browser snapshot -i                     # List interactive elements with refs
DISPLAY=:0 agent-browser fill @e1 "text"                 # Fill input by ref
DISPLAY=:0 agent-browser click @e2                       # Click by ref
DISPLAY=:0 agent-browser press Enter                     # Press key
DISPLAY=:0 agent-browser screenshot /tmp/shot.png        # Screenshot (view with Read tool)
DISPLAY=:0 agent-browser console                         # Browser console logs
DISPLAY=:0 agent-browser eval "JS expression"            # Run JS in page
DISPLAY=:0 agent-browser wait 3000                       # Wait ms
DISPLAY=:0 agent-browser scroll down 500                 # Scroll
DISPLAY=:0 agent-browser close                           # Close browser
```

Enable frontend debug logging: `DISPLAY=:0 agent-browser eval "localStorage.setItem('debug:pi-v2', '1')"`

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
| **Backend (oqto)** | Stateless relay: routes commands to runners, forwards events to frontend |
| **Runner (oqto-runner)** | Per-user daemon: owns agent processes, translates native events to canonical format |
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
| `runner` | Via `oqto-runner` daemon | Multi-user Linux isolation |
| `container` | Inside Docker/Podman | Full container isolation |

### Key Binaries

| Binary | Purpose |
|--------|---------|
| `oqto` | Main backend server |
| `oqtoctl` | CLI for server management |
| `oqto-runner` | Multi-user process daemon, manages agent harnesses |
| `oqto-sandbox` | Sandbox wrapper using bwrap/sandbox-exec |
| `pi-bridge` | HTTP/WebSocket bridge for Pi in containers |
| `oqto-files` | File access server for workspaces |
| `hstry` | Chat history daemon (gRPC, SQLite-backed) |

### Eavs Integration (LLM Proxy)

Eavs is the single source of truth for LLM model metadata and the routing layer between Pi and upstream providers.

**Architecture**: `Pi -> eavs (localhost:3033) -> upstream provider APIs`

Key integration points:
- **Model metadata**: `oqto` queries eavs `/providers/detail` to generate Pi's `models.json` (no hardcoded model lists in oqto)
- **Per-user keys**: Admin API creates eavs virtual keys per user, stored in `eavs.env` files
- **OAuth routing**: Virtual keys can be bound to OAuth users + account labels for multi-account provider access
- **Policy enforcement**: Eavs rewrites request fields (e.g., `store: true` for Codex models) before forwarding upstream
- **Network ACL**: Domain allow/deny lists prevent agents from reaching unauthorized endpoints
- **Quota tracking**: Upstream rate limit headers are parsed and available via `GET /admin/quotas`

Key files:
- `backend/crates/oqto/src/eavs/` -- `EavsClient` (create/revoke keys), `generate_pi_models_json()`
- `backend/crates/oqto/src/api/handlers/admin.rs` -- `provision_eavs_for_user`, `sync_eavs_models_json`
- `backend/crates/oqto/src/session/service.rs` -- Injects `EAVS_API_KEY` env var into agent sessions

### Process Sandboxing

Sandbox configuration in `~/.config/oqto/sandbox.toml` (separate from main config for security):

```toml
enabled = true
profile = "development"  # or "minimal", "strict"
deny_read = ["~/.ssh", "~/.aws", "~/.gnupg"]
allow_write = ["~/.cargo", "~/.npm", "/tmp"]
isolate_network = false  # true in strict profile
isolate_pid = true
```

Per-workspace overrides in `.oqto/sandbox.toml` can only ADD restrictions, never remove them.

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

All chat history access goes through hstry's gRPC API - no raw SQLite access from `oqto`.

- **WriteService**: Persist messages after agent turns complete (via `HstryClient` gRPC)
- **ReadService**: Query messages, sessions, search (via `HstryClient` gRPC)
- Stores canonical `Message` format directly (no translation at read time)
- **Runner exception**: `oqto-runner` reads hstry SQLite directly for speed (runs as target user, same machine). This is intentional and secure.

### Session Files (Pi-Owned)

Pi writes its own JSONL session files -- **Oqto must NEVER create or write JSONL session files**.

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

Use `just` (justfile) for all build/dev tasks:

```bash
just dev              # Start frontend dev server (Vite on :3000)
just build            # Build all components
just build-backend    # Build backend only
just build-frontend   # Build frontend only
just lint             # Run all linters
just fmt              # Format all Rust code
just check            # Check all Rust code compiles
just gen-types        # Generate TypeScript types from Rust structs
```

| Component | Build | Lint | Test | Single Test |
|-----------|-------|------|------|-------------|
| **backend/** | `cargo build` | `cargo clippy && cargo fmt --check` | `cargo test` | `cargo test test_name` |
| **fileserver/** | `cargo build` | `cargo clippy && cargo fmt --check` | `cargo test` | `cargo test test_name` |
| **frontend/** | `bun run build` | `bun run lint` | `bun run test` | `bun run test -t "pattern"` |

### Dev Workflow

- **Frontend dev server**: Always use `just dev` (runs `cd frontend && bun dev`). The Vite dev server runs on port 3000.
- **Restart frontend**: If Vite HMR gets stuck after major type changes, kill the dev server (Ctrl+C in its tmux pane) and run `just dev` again. May also need `rm -rf frontend/node_modules/.vite` to clear Vite cache.
- **Backend is at** `archlinux:8080` (tmux pane `0:1.1`).
- **Frontend dev server** runs in tmux pane `%5` (check with `tmux list-panes -a`).
- **Rebuild backend**: `cd backend && cargo build --release` then restart the process in its tmux pane.

## Code Style

**Rust**: Use `anyhow::Result` with `.context()` for errors. Group imports: std, external crates, internal modules. Run `cargo fmt` and `cargo check` after changes.

**TypeScript**: Use `@/` import alias for internal modules. Functional components with named exports. Vitest for tests.

**General**: No emojis in code/docs/commits. Use `bun` for JS/TS, `uv` for Python.

---

## External Dependencies

Oqto depends on several external tools and services. Version tracking is maintained in `dependencies.toml`.

### Updating Dependencies

```bash
just update-deps      # Update dependencies.toml from local repos and git tags
just check-updates    # Check for available updates to external dependencies
```

The `dependencies.toml` manifest tracks:

- **byteowlz tools**: hstry, mmry, trx, agntz, mailz, sx, sldr, eaRS, kokorox, eavs
- **External tools**: pi (from crates.io), opencode (from opencode.ai)

Key dependencies:

| Tool | Purpose | Install Command |
|------|---------|-----------------|
| **eavs** | LLM proxy: multi-provider routing, OAuth, virtual keys, quotas | `cargo install --git https://github.com/byteowlz/eavs` |
| **hstry** | Chat history storage (gRPC + SQLite) | `cargo install --git https://github.com/byteowlz/hstry` |
| **mmry** | Memory system with semantic search | `cargo install --git https://github.com/byteowlz/mmry` |
| **trx** | Issue and task tracking | `cargo install --git https://github.com/byteowlz/trx` |
| **agntz** | Agent toolkit (memory, issues, mail) | `cargo install --git https://github.com/byteowlz/agntz` |
| **pi** | Main chat/LLM interface | `bun install -g @mariozechner/pi-coding-agent` |
| **eaRS** | Speech-to-text (STT) service | `cargo install --git https://github.com/byteowlz/ears` |
| **kokorox** | Text-to-speech (TTS) service | `cargo install --git https://github.com/byteowlz/kokorox` |

For a complete list, see `dependencies.toml`.

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

### Session Protocol

**Before ending a session, run this checklist:**

```bash
git status              # Check what changed
git add <files>         # Stage code changes
git commit -m "..."     # Commit everything together
```

### Best Practices

- Check `trx ready` at session start to find available work
- Update status as you work (in_progress -> closed)
- Create new issues with `trx create` when you discover tasks
- Use descriptive titles and set appropriate priority/type

---

Note regarding file edits you did not do:

DO NOT STOP AND ASK ABOUT THESE CHANGES. Those are changes created by other agents working on the project at the same time as you. This is a common occurrence. DO NOT, stash, revert, overwrite, or otherwise disturb in ANY way the work of other agents. Just treat those changes identically to changes that you yourself made. Just assume that YOU made the changes and simply don't recall it for some reason. Should I ask you to commit, create sensible commit messages for these changes as well.

---

### Continuous Improvements for our own tools

Many of the tools we are building like trx, agntz etc. live in ../
If you run into any issues with these tools or find that the ergonomics could be improved, just file trx isuues in that repo so we can constantly improve our own tools. You can run ```byt catalog list``` to get an overview of all of our tools.
