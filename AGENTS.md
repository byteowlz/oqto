# Octo - AI Agent Workspace Platform

Octo is a self-hosted platform for managing AI coding agents. Supports local mode (native processes) and container mode (Docker/Podman).

**New to Octo?** Start with the [SETUP.md](./SETUP.md) guide for installation and prerequisites.

---
## Debugging

Tmux is always available, use it to debug the logs of the running backend and frontend.

Use agent-browser + tmux for end-to-end testing

## Architecture Overview

Octo manages two types of AI agent sessions:

| Session Type | Agent Runtime | Purpose |
|--------------|---------------|---------|
| **Main Chat** | `pi` | Primary chat interface, streaming responses, compaction |
| **OpenCode Sessions** | `opencode` | Per-workspace coding agent sessions |

### Agent Runtimes

**pi** - Lightweight AI coding assistant CLI (`~/.bun/bin/pi`)
- Source code: `../external-repos/pi-mono`
- Used for Main Chat in the Octo UI
- Supports multiple providers (anthropic, openai, google)
- RPC mode for streaming via stdin/stdout JSON protocol
- Tools: read, bash, edit, write, grep, find, ls
- Extensions and skills system
- Session management with compaction

**opencode** - Full-featured coding agent
- Source code: `../external-repos/opencode`
- Used for workspace-specific sessions
- HTTP server mode (`opencode serve`)
- `x-opencode-directory` header switches working directory per request
- Currently: one opencode server per user serves all workspaces

### Runtime Modes

Both `pi` and `opencode` can run in different isolation modes:

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
| `octo-runner` | Multi-user process daemon (Linux only) |
| `octo-sandbox` | Sandbox wrapper using bwrap/sandbox-exec |
| `pi-bridge` | HTTP/WebSocket bridge for Pi in containers |
| `octo-files` | File access server for workspaces |

### External Dependencies

fresh clones of dependencies like opencode or pi-mono can be found in ../external-repos

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

## Recent Architecture Decisions

- Hstry is the canonical chat history store. Single-user main chat reads via hstry ReadService; multi-user reads via octo-runner against per-user hstry.db.
- Pi sessions rehydrate by rebuilding JSONL from hstry when the session file is missing.
- Main chat Pi WS connections bind to a specific session_id; runner writer errors trigger a single restart + retry.
- Provider storage: Pi JSONL stores provider in model_change entries and assistant message payloads; OpenCode stores provider per message as providerID in session message JSON.

## Memory System (Critical)

**ALWAYS search memories before starting work on unfamiliar areas.** Memories contain architecture decisions, API patterns, and debugging insights that save time.

```bash
# Search BEFORE implementing
agntz memory search "sandbox"
agntz memory search "pi runtime"
agntz memory search "opencode session"
```

**Create memories when you discover reusable knowledge:**

```bash
agntz memory add "Pi uses RPC mode with JSON over stdin/stdout for Main Chat streaming" -c architecture -i 8
agntz memory add "x-opencode-directory header switches workspace without restarting server" -c api -i 7
```

---

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
- **Existing interfaces** - "OpenCode has PATCH /session/{id} for updating session title"
- **Architecture decisions** - "Chat history sessions use JSON files, live sessions use opencode API"
- **Debugging insights** - "Port cleanup requires waiting for process exit to prevent zombies"
- **Integration points** - "x-opencode-directory header switches working directory for any request"

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
agntz memory add "OpenCode PATCH /session/{id} accepts {title} to rename sessions" -c api -i 7
agntz memory add "features.voice config gates dictation and voice mode buttons" -c frontend -i 6

# Categories: api, frontend, backend, architecture, patterns, debugging
# Importance: 1-10 (7+ for significant insights)
```

### Memory Examples (Good)

```bash
agntz memory add "Chat sessions from disk need PATCH /api/chat-history/{id} since no opencode running" -c api -i 7
agntz memory add "useDictation hook provides STT-only mode separate from full voice mode" -c frontend -i 6
agntz memory add "SessionInfo stored in ~/.local/share/opencode/storage/session/{projectID}/ses_*.json" -c architecture -i 8
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
