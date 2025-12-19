# Octo - AI Agent Workspace Platform

A secure, self-hosted platform providing containerized AI agent environments with persistent workspaces, web terminals, and file management.

## Overview

Octo provides isolated, container-based workspaces where users collaborate with AI agents. Each session runs in a Podman/Docker container with:

- **opencode** - AI coding assistant with full shell access
- **fileserver** - File browsing, upload/download, and management
- **ttyd** - Web-based terminal access

The platform supports different agent templates (Coding Copilot, Research Assistant, etc.) by injecting specific configurations at container startup.

## Architecture

```
Browser  <-->  Caddy  <-->  Next.js Frontend (Auth/UI)
                          <-->  Rust Backend (API/Orchestration)
                                    |
                                    +--> Container Runtime (Docker/Podman)
                                            |
                                            +--> Session Container
                                                   |-- opencode (port 41820)
                                                   |-- fileserver (port 41821)
                                                   +-- ttyd (port 41822)
```

## Components

| Directory        | Description                                                         |
| ---------------- | ------------------------------------------------------------------- |
| `backend/`       | Rust API server - session orchestration, container management, auth |
| `frontend/`      | Next.js UI - workspace picker, chat interface, file tree, terminal  |
| `fileserver/`    | Lightweight Rust file server for workspace access                   |
| `container/`     | Dockerfile and entrypoint for agent runtime containers              |
| `browser-tools/` | Browser automation scripts                                          |

## Quick Start

### Prerequisites

- Docker or Podman installed and running
- Rust toolchain (`rustup`)
- Bun (`bun.sh`)

### 1. Build the Container Image

```bash
docker build -t octo-dev:latest -f container/Dockerfile .
```

### 2. Start the Backend

```bash
cd backend
cargo run --bin octo -- serve
```

### 3. Start the Frontend

```bash
cd frontend
bun install
bun dev
```

The frontend runs at `http://localhost:3000`.

## Configuration

### Backend

Config file: `$XDG_CONFIG_HOME/octo/config.toml`

```toml
[server]
port = 8080

[container]
runtime = "docker"  # or "podman"
default_image = "octo-dev:latest"
base_port = 41820
```

See `backend/examples/config.toml` for all options.

### Frontend

Create `.env.local`:

```bash
NEXT_PUBLIC_OPENCODE_BASE_URL=http://localhost:4096
NEXT_PUBLIC_TERMINAL_WS_URL=ws://localhost:9090/ws
NEXT_PUBLIC_FILE_SERVER_URL=http://localhost:9000
```

## Development

| Component  | Build           | Lint                                | Test           |
| ---------- | --------------- | ----------------------------------- | -------------- |
| backend    | `cargo build`   | `cargo clippy && cargo fmt --check` | `cargo test`   |
| fileserver | `cargo build`   | `cargo clippy && cargo fmt --check` | `cargo test`   |
| frontend   | `bun run build` | `bun run lint`                      | `bun run test` |

## CLI Tools

### octo (Server)

```bash
octo serve                    # Start API server
octo init                     # Create config directories
octo config show              # Show effective configuration
octo invite-codes generate    # Generate invite codes
```

### octoctl (Control CLI)

```bash
octoctl status                # Check server status
octoctl session list          # List all sessions
octoctl container refresh     # Rebuild containers from latest image
octoctl image build           # Build new container image
```

## API Endpoints

- `POST /api/sessions` - Create a new session
- `GET /api/sessions` - List all sessions
- `GET /api/sessions/:id` - Get session details
- `DELETE /api/sessions/:id` - Stop and remove session
- `POST /api/sessions/:id/stop` - Stop session (preserve container)
- `POST /api/sessions/:id/resume` - Resume stopped session
- `GET /sessions/:id/opencode/*` - Proxy to opencode
- `GET /sessions/:id/files/*` - Proxy to fileserver
- `GET /sessions/:id/terminal` - WebSocket proxy to ttyd

## Agent Templates

Templates configure agent behavior by injecting `AGENTS.md` and `opencode.json`:

| Template           | Use Case          | Capabilities                        |
| ------------------ | ----------------- | ----------------------------------- |
| Coding Copilot     | Developers        | Full shell, Git, code editing       |
| Research Assistant | Knowledge workers | File reading, search, summarization |
| Meeting Synth      | Managers          | Transcript parsing, summarization   |

## Security

- Rootless containers (Podman) for production
- No direct browser-to-container communication
- JWT authentication with invite code registration
- Path traversal protection in fileserver

## License

Proprietary
