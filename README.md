![banner](banner.png)

# octo

Self-hosted platform for AI coding agents. Run OpenCode instances in isolated containers or native processes with web UI, terminal access, and file management.

## What it does

- Spawns and manages OpenCode AI agent sessions
- Provides web UI for chat, file browsing, and terminal
- Supports container mode (Docker/Podman) or local mode (native processes)
- Multi-user with JWT auth and invite codes
- Voice mode (STT/TTS) via eaRS and kokorox

## Architecture

```
                    ┌─────────────────────────────────────┐
                    │            Octo Backend             │
                    │              (Rust)                 │
                    ├─────────────────────────────────────┤
  Browser/App ───►  │  REST API · WebSocket · SSE Proxy  │
                    └──────────────┬──────────────────────┘
                                   │
           ┌───────────────────────┼───────────────────────┐
           ▼                       ▼                       ▼
   ┌──────────────┐       ┌──────────────┐       ┌──────────────┐
   │   Session    │       │   Session    │       │   Session    │
   │  Container   │       │  Container   │       │   (Local)    │
   │              │       │              │       │              │
   │  · opencode  │       │  · opencode  │       │  · opencode  │
   │  · fileserver│       │  · fileserver│       │  · fileserver│
   │  · ttyd      │       │  · ttyd      │       │  · ttyd      │
   └──────────────┘       └──────────────┘       └──────────────┘
```

## Quick Start

### Prerequisites

- Rust toolchain
- Bun
- Docker or Podman (for container mode)

### Run locally

```bash
# Backend
cd backend
cargo run --bin octo -- serve

# Frontend (separate terminal)
cd frontend
bun install && bun dev
```

Open `http://localhost:3000`. Default dev login: check backend logs for credentials.

### Container mode

```bash
# Build the agent container image
docker build -t octo-dev:latest -f container/Dockerfile .

# Configure backend to use containers
# Edit ~/.config/octo/config.toml:
#   [container]
#   runtime = "docker"
#   default_image = "octo-dev:latest"
```

## Project Structure

```
backend/        Rust API server, session orchestration, auth
frontend/       React UI (chat, files, terminal, settings)
fileserver/     Rust file server for workspace access
container/      Dockerfile for agent runtime containers
browser-tools/  Browser automation scripts
```

## Configuration

### Backend (`~/.config/octo/config.toml`)

```toml
[server]
port = 8080

[container]
runtime = "docker"  # or "podman" or "local"
default_image = "octo-dev:latest"

[auth]
jwt_secret = "change-me"
dev_mode = true  # enables dev login

[voice]
enabled = true
stt_url = "ws://localhost:8765"  # eaRS
tts_url = "ws://localhost:8766"  # kokorox
```

### Frontend (`.env.local`)

```bash
VITE_CONTROL_PLANE_URL=http://localhost:8080
```

## Development

| Component  | Build           | Lint                                | Test           |
| ---------- | --------------- | ----------------------------------- | -------------- |
| backend    | `cargo build`   | `cargo clippy && cargo fmt --check` | `cargo test`   |
| fileserver | `cargo build`   | `cargo clippy && cargo fmt --check` | `cargo test`   |
| frontend   | `bun run build` | `bun run lint`                      | `bun run test` |

## CLI

```bash
# Server
octo serve                    # Start API server
octo config show              # Show configuration
octo invite-codes generate    # Generate invite codes

# Control
octoctl status                # Check server health
octoctl session list          # List sessions
octoctl image build           # Build container image
```

## API

| Endpoint                   | Description           |
| -------------------------- | --------------------- |
| `POST /api/sessions`       | Create session        |
| `GET /api/sessions`        | List sessions         |
| `DELETE /api/sessions/:id` | Delete session        |
| `/session/:id/code/*`      | Proxy to OpenCode     |
| `/session/:id/files/*`     | Proxy to fileserver   |
| `/session/:id/term`        | WebSocket to terminal |
| `/session/:id/code/event`  | SSE event stream      |

## Roadmap

See [TAURI.md](./TAURI.md) for desktop/mobile app plans.

## License

Proprietary
