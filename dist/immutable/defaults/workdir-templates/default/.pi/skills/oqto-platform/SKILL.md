---
name: oqto-platform
description: Guide for understanding and working with the Oqto platform - a self-hosted AI agent workspace. Use when explaining Oqto architecture, API usage, session management, CLI tools (oqto, oqtoctl, agntz, byt), configuration, or when helping users interact with the platform's features including chat, file browser, terminal, voice mode, and memory system.
---

# Oqto - AI Agent Workspace Platform

Oqto is a self-hosted platform for managing AI coding agents. It supports local mode (native processes, multi-user Linux isolation) and container mode (Docker/Podman).

## Architecture

```
Frontend                          Backend (oqto)                    Runner (per user)
   |                                 |                                    |
   |-- Single WebSocket ------------>|                                    |
   |   (multiplexed channels)        |                                    |
   |                                 |-- Unix/TCP socket ---------------->|
   |                                 |   (runner protocol)                |
   |                                 |                                    |
   |   {channel:"agent", ...}        |   Canonical Commands              |-- Agent Process A
   |   {channel:"files", ...}        |   Canonical Events                |-- Agent Process B
   |   {channel:"terminal", ...}     |                                   |-- hstry (gRPC)
   |   {channel:"hstry", ...}        |                                    |
```

### Core Components

| Component | Purpose |
|-----------|---------|
| **Frontend** | React/TypeScript app speaking the canonical protocol via multiplexed WebSocket |
| **Backend (oqto)** | Stateless relay: routes commands to runners, forwards events to frontend |
| **Runner (oqto-runner)** | Per-user daemon: owns agent processes, translates native events to canonical format |
| **hstry** | Chat history service (gRPC API, SQLite-backed). All reads/writes go through gRPC. |

### The Canonical Protocol

The frontend speaks a **harness-agnostic canonical protocol**. The message format and UI rendering is identical regardless of which agent harness is running.

- **Messages** are persistent (stored in hstry) with typed **Parts**: text, thinking, tool_call, tool_result, image, file_ref, etc.
- **Events** are ephemeral UI signals: stream.text_delta, agent.working, tool.start, agent.idle, etc.
- **Commands** flow from frontend to runner: prompt, abort, set_model, compact, fork, etc.

### Harnesses

A **harness** is an agent runtime that the runner can spawn. The runner translates the harness's native protocol into canonical format.

| Harness | Binary | Status |
|---------|--------|--------|
| **Pi** | `~/.bun/bin/pi` | Primary harness (active) |
| *(custom)* | Any RPC-compatible agent | Extensible |

### Runtime Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| `local` | Direct process spawn | Single-user, development |
| `runner` | Via `oqto-runner` daemon | Multi-user Linux isolation |
| `container` | Inside Docker/Podman | Full container isolation |

### Key Binaries

| Binary | Crate | Purpose |
|--------|-------|---------|
| `oqto` | oqto | Main backend server |
| `oqtoctl` | oqto | CLI for server management |
| `oqto-runner` | oqto | Per-user process daemon, manages agent harnesses |
| `oqto-sandbox` | oqto | Sandbox wrapper using bwrap/sandbox-exec |
| `oqto-guard` | oqto | Security guard binary |
| `oqto-ssh-proxy` | oqto | SSH proxy for remote access |
| `pi-bridge` | oqto | HTTP/WebSocket bridge for Pi in containers |
| `oqto-files` | oqto-files | File access server for workspaces |
| `hstry` | (external) | Chat history daemon (gRPC, SQLite-backed) |

### Additional Crates

| Crate | Purpose |
|-------|---------|
| `oqto-protocol` | Canonical protocol types (messages, events, commands, runner protocol) |
| `oqto-browser` / `oqto-browserd` | Agent browser integration (headless browser for agents) |
| `oqto-scaffold` | Project scaffolding / template system |
| `oqto-setup` | Server setup tooling |
| `oqto-usermgr` | User management utilities |

### Eavs Integration (LLM Proxy)

Eavs is the single source of truth for LLM model metadata and the routing layer between Pi and upstream providers.

**Architecture**: `Pi -> eavs (localhost:3033) -> upstream provider APIs`

Key integration points:
- **Model metadata**: `oqto` queries eavs `/providers/detail` to generate Pi's `models.json`
- **Per-user keys**: Admin API creates eavs virtual keys per user, stored in `eavs.env` files
- **OAuth routing**: Virtual keys can be bound to OAuth users + account labels for multi-account provider access
- **Policy enforcement**: Eavs rewrites request fields before forwarding upstream
- **Quota tracking**: Upstream rate limit headers are parsed and available via admin API

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

## Event Flow

```
Agent Harness (Pi --mode rpc, stdin/stdout JSON)
  -> Runner: stdout_reader_task()
  -> Runner: translate(NativeEvent) -> CanonicalEvent
  -> Runner: broadcast::Sender<CanonicalEvent>
  -> Backend: Unix socket / TCP
  -> Backend: WebSocket handler
  -> Frontend: multiplexed WebSocket (agent, files, terminal, hstry channels)
```

The runner maintains a state machine per session (idle, working, error) and emits canonical events. The frontend derives UI state directly from events without harness-specific logic.

## Core Features

### Session Management
Sessions are isolated AI agent workspaces. The backend creates/manages them via REST API.

### Multiplexed WebSocket
All real-time communication flows through a single multiplexed WebSocket at `/ws/mux` with channels:
- **agent**: Pi agent events and commands
- **files**: File operations
- **terminal**: Web terminal (ttyd)
- **hstry**: Chat history events
- **trx**: Issue tracking channel

### Voice Mode
Real-time voice interaction:
- **STT**: eaRS WebSocket service
- **TTS**: kokorox WebSocket service
- Visualizers: Orb and K.I.T.T. styles
- VAD auto-send after silence

### Agent Browser
Headless browser integration allowing agents to browse web pages, take screenshots, and interact with web content. Managed via `oqto-browserd`.

### A2UI (Agent-to-UI)
Protocol for agents to generate rich, interactive UI surfaces directly in the chat interface. Implements Google's A2UI spec.

### Delegation
Localhost-only API for Pi extensions to start sub-sessions, send prompts, and check status. Used for multi-agent orchestration.

### Onboarding
Progressive onboarding system with stages, component unlocking, and bootstrap templates.

### Scheduler (sldr)
Job scheduling integration proxied through the backend.

### Settings
Schema-driven settings system with hot-reload support. Settings are scoped by user role and managed via API.

### Workspace Management
- Project locations (multiple workspace roots)
- Workspace metadata and sandbox config
- Pi resource management per workspace
- Project templates via scaffold system

## Storage

### hstry (Chat History)
All chat history access goes through hstry's gRPC API - no raw SQLite access from `oqto`.

- **WriteService**: Persist messages after agent turns complete (via `HstryClient` gRPC)
- **ReadService**: Query messages, sessions, search (via `HstryClient` gRPC)
- Stores canonical `Message` format directly (no translation at read time)
- **Runner exception**: `oqto-runner` reads hstry SQLite directly for speed (runs as target user, same machine)

### Session Files (Pi-Owned)
Pi writes its own JSONL session files -- **Oqto must NEVER create or write JSONL session files**.

- **Pi**: `~/.pi/agent/sessions/--{safe_cwd}--/{timestamp}_{session_id}.jsonl`
- These are authoritative for harness-specific metadata (titles, fork points)
- hstry is authoritative for structured message content

## Agent Tools

CLI tools available for agent workflows:

| Tool | Purpose |
|------|---------|
| **byt** | Cross-repo governance and management (catalog, schemas, releases) |
| **agntz** | Day-to-day agent operations (memory, issues, mail, file reservations) |
| **sx** | External searches via SearXNG (`sx "<query>" -p`) |
| **trx** | Issue and task tracking |

## Memory System

Create memories for reusable knowledge (patterns, interfaces, architecture decisions, debugging insights).

```bash
# Good: Reusable insight
agntz memory add "PATCH /session/{id} accepts {title} to rename sessions" -c api -i 7

# Bad: Too specific
agntz memory add "Fixed bug in line 451 of app-context.tsx"
```

Categories: `api`, `frontend`, `backend`, `architecture`, `patterns`, `debugging`
Importance: 1-10 (7+ for significant insights)

## Resources

For detailed information, see:
- **API Reference**: See [references/api.md](references/api.md) for complete endpoint documentation
- **CLI Reference**: See [references/cli.md](references/cli.md) for all CLI commands and options
- **Configuration**: See [references/configuration.md](references/configuration.md) for all config options
