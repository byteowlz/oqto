![banner](banner.png)

# octo

Self-hosted platform for AI coding agents. Run OpenCode instances in isolated containers or native processes with web UI, terminal access, and file management.

## What it does

- Spawns and manages OpenCode AI agent sessions
- Provides web UI for chat, file browsing, and terminal
- Supports container mode (Docker/Podman) or local mode (native processes)
- Multi-user with JWT auth and invite codes
- Voice mode (STT/TTS) via eaRS and kokorox
- Agent tools for memory and task tracking (agntz, mmry, trx)

## Architecture

```
                    ┌─────────────────────────────────────┐
                    │            Octo Backend             │
                    │  (octo, octoctl, octo-runner, pi)  │
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
   │  · pi        │       │  · pi        │       │  · pi        │
   └──────────────┘       └──────────────┘       └──────────────┘

Agent Tools (CLI):
  agntz   - Memory, issues, mail, reservations
  mmry    - Memory system (optional, integrated)
  trx     - Task tracking (optional, integrated)
  mailz   - Agent messaging (optional)
```

## Quick Start

### Automated Setup (Recommended)

For a complete setup experience with all prerequisites handled automatically:

```bash
./setup.sh
```

The interactive script will guide you through:
- User mode selection (single-user or multi-user)
- Backend mode selection (local processes or containers)
- Installing all dependencies (Rust, Bun, agent tools, shell tools)
- Building Octo components
- Generating configuration files
- Installing system services (optional)

For detailed manual setup instructions and prerequisite documentation, see [SETUP.md](./SETUP.md).

### Manual Setup

If you prefer to install components manually:

1. Install core dependencies (Rust, Bun, Docker/Podman)
2. Build the backend and fileserver:
   ```bash
   cargo install --path backend
   cargo install --path fileserver
   ```
3. Build the frontend:
   ```bash
   cd frontend && bun install && bun run build
   ```
4. Install agent tools (opencode, ttyd, agntz, byt)
5. Configure `~/.config/octo/config.toml`
6. Start services:
   ```bash
   # Backend
   octo serve

   # Frontend (development)
   cd frontend && bun dev
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

For complete configuration options, see [backend/examples/config.toml](./backend/examples/config.toml).

### Backend (`~/.config/octo/config.toml`)

```toml[server]
port = 8080

[container]
runtime = "docker"  # or "podman"
default_image = "octo-dev:latest"

[local]
enabled = false  # Set to true for local mode
opencode_binary = "opencode"
fileserver_binary = "fileserver"
ttyd_binary = "ttyd"

[auth]
dev_mode = true  # enables dev users
# jwt_secret = "change-me"  # Required when dev_mode = false

[pi]
enabled = true
executable = "pi"
default_provider = "anthropic"
```

### Frontend (`.env.local`)

```bash
VITE_CONTROL_PLANE_URL=http://localhost:8080
```

Run `./setup.sh` to generate configuration files automatically.

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

## Documentation

- [SETUP.md](./SETUP.md) - Comprehensive setup and installation guide
- [AGENTS.md](./AGENTS.md) - Agent development guidelines
- [backend/README.md](./backend/README.md) - Backend documentation
- [frontend/README.md](./frontend/README.md) - Frontend documentation
- [deploy/systemd/README.md](./deploy/systemd/README.md) - Systemd service setup
- [backend/examples/config.toml](./backend/examples/config.toml) - Full configuration reference

## Roadmap

See [TAURI.md](./TAURI.md) for desktop/mobile app plans.

## License

Proprietary
