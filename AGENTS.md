# Octo - AI Agent Workspace Platform

Octo is a self-hosted platform for managing AI coding agents (opencode instances). Supports local mode (native processes) and container mode (Docker/Podman).

## Agent Tools

Two CLI tools are available for agent workflows:

| Tool | Purpose |
|------|---------|
| **byt** | Cross-repo governance and management (catalog, schemas, releases) |
| **agntz** | Day-to-day agent operations (memory, issues, mail, file reservations) |

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

**Create memories whenever you learn something reusable.** Memories are for patterns, interfaces, and insights - not atomic implementation details.

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

