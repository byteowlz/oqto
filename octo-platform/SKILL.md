---
name: octo-platform
description: Guide for understanding and working with the Octo platform - a self-hosted AI agent workspace. Use when explaining Octo architecture, API usage, session management, CLI tools (octo, octoctl, agntz, byt), configuration, or when helping users interact with the platform's features including chat, file browser, terminal, voice mode, and memory system.
---

# Octo Platform

Octo is a self-hosted platform for managing AI coding agents (OpenCode instances). It supports container mode (Docker/Podman) and local mode (native processes) with web UI access to chat, files, and terminal.

## Architecture

```
                    +-----------------------------------------+
                    |            Octo Backend                 |
                    |              (Rust)                     |
                    +-----------------------------------------+
  Browser/App --->  |  REST API - WebSocket - SSE Proxy      |
                    +-------------------+---------------------+
                                        |
           +----------------------------+----------------------------+
           v                            v                            v
   +---------------+           +---------------+           +---------------+
   |   Session     |           |   Session     |           |   Session     |
   |  Container    |           |  Container    |           |   (Local)     |
   |               |           |               |           |               |
   |  - opencode   |           |  - opencode   |           |  - opencode   |
   |  - fileserver |           |  - fileserver |           |  - fileserver |
   |  - ttyd       |           |  - ttyd       |           |  - ttyd       |
   +---------------+           +---------------+           +---------------+
```

Each session runs:
- **opencode**: AI agent runtime (port 41820)
- **fileserver**: Workspace file access (port 41821)
- **ttyd**: Web terminal (port 41822)

## Runtime Modes

### Container Mode (Default)
Isolated environments per session using Docker/Podman. Sessions can be stopped, resumed, and upgraded.

### Local Mode
Native process spawning without containers. Faster startup, shared host environment.

Configure mode in `~/.config/octo/config.toml`:
```toml
[backend]
mode = "container"  # or "local" or "auto"
```

## Core Features

### Session Management
Sessions are isolated AI agent workspaces. The backend creates/manages them via REST API.

| Action | API Endpoint | CLI |
|--------|--------------|-----|
| Create | `POST /api/sessions` | - |
| List | `GET /api/sessions` | `octoctl session list` |
| Stop | `POST /api/sessions/:id/stop` | `octoctl session stop <id>` |
| Resume | `POST /api/sessions/:id/resume` | `octoctl session resume <id>` |
| Delete | `DELETE /api/sessions/:id` | - |

### Proxy Routes
Backend proxies requests to session services:

| Route | Target |
|-------|--------|
| `/session/:id/code/*` | OpenCode API |
| `/session/:id/files/*` | Fileserver |
| `/session/:id/term` | Terminal WebSocket |
| `/session/:id/code/event` | SSE event stream |

### Voice Mode
Real-time voice interaction:
- **STT**: eaRS WebSocket service
- **TTS**: kokorox WebSocket service
- Visualizers: Orb and K.I.T.T. styles
- VAD auto-send after silence

### Main Chat
Persistent cross-project AI assistant with memory via mmry. Uses Pi agent runtime with block streaming and automatic compaction.

## Agent Tools

Two CLI tools for agents running inside sessions:

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

## CLI Tools

### octo (Server)
```bash
octo serve                    # Start API server
octo config show              # Show configuration
octo invite-codes generate    # Generate invite codes
```

### octoctl (Control)
```bash
octoctl status                # Check server health
octoctl session list          # List sessions
octoctl session stop <id>     # Stop a session
octoctl session resume <id>   # Resume a session
octoctl image build           # Build container image
```

## Resources

For detailed information, see:
- **API Reference**: See [references/api.md](references/api.md) for complete endpoint documentation
- **CLI Reference**: See [references/cli.md](references/cli.md) for all CLI commands and options
- **Configuration**: See [references/configuration.md](references/configuration.md) for all config options
