![banner](banner.png)

# octo

Self-hosted platform for managing AI coding agents. Supports local mode (via octo-runner) and container mode (Docker/Podman) with web UI for chat, file browsing, terminal access, canvas, memory, and task tracking.

## Architecture

The backend never spawns agents directly. All agent processes go through `octo-runner`.

```
Frontend <--[WS: mux]--> Backend (octo) <--[Unix socket]--> octo-runner <--[stdin/stdout]--> Agent
```

The frontend speaks a canonical agent protocol over a multiplexed WebSocket (`/api/ws/mux`). It is agent-runtime agnostic -- the backend translates between the canonical protocol and agent-specific protocols.

---

### Agent runtimes

**pi** -- Lightweight AI coding assistant CLI used for Main Chat. Runs in RPC mode with JSON over stdin/stdout. Supports multiple providers (anthropic, openai, google), extensions, skills, and session compaction.

**Claude Code** -- Full-featured coding agent used for per-workspace coding sessions. Runs in HTTP server mode.

---

### Key binaries

| Binary | Purpose |
|--------|---------|
| `octo` | Main backend server |
| `octoctl` | CLI for server management |
| `octo-runner` | Multi-user process daemon (Linux only) |
| `octo-sandbox` | Sandbox wrapper using bwrap/sandbox-exec |
| `pi-bridge` | HTTP/WebSocket bridge for Pi in containers |
| `octo-files` | File access server for workspaces |

---

### Backend crates

| Crate | Purpose |
|-------|---------|
| `octo` | Main server (API, sessions, runner, auth) |
| `octo-protocol` | Canonical agent protocol types (events, commands, messages) |
| `octo-files` | File server for workspace access |
| `octo-browser` | Browser automation |
| `octo-scaffold` | Project scaffolding |

---

### Runtime modes

| Mode | Description | Use Case |
|------|-------------|----------|
| `local` | Via `octo-runner` daemon | Single-user and multi-user Linux |
| `container` | Inside Docker/Podman | Full container isolation |

Even in local mode, agents are spawned through octo-runner, never directly by the backend.

---

### Integrate with services

| Service | Purpose |
|---------|---------|
| `hstry` | Chat history storage (per-user SQLite) |
| `mmry` | Memory system (semantic search, embeddings) |
| `trx` | Issue/task tracking |
| `eavs` | LLM API proxy |

---

### Support multiple users

In multi-user mode each platform user maps to a Linux user (`octo_{username}`). Per-user octo-runner daemons manage agent processes and per-user mmry instances provide memory via HTTP API.

Auth uses JWT with invite codes. A progressive onboarding system with agent-driven UI unlock guides new users.

## Quick Start

### Use automated setup (recommended)

```bash
./setup.sh
```

The interactive script handles user mode selection, backend mode, dependency installation, building, configuration, and optional systemd services. See [SETUP.md](./SETUP.md) for full details.

### Portable install config

Generate `octo.install.toml` with the setup wizard or by hand, then hydrate per-app configs:

```bash
octo-setup hydrate --install-config octo.install.toml
```

---

### Set up manually

1. Install Rust and Bun
2. Build the backend:

   ```bash
   cargo install --path backend/crates/octo
   ```

3. Build the frontend:

   ```bash
   cd frontend && bun install && bun run build
   ```

4. Install agent tools (pi, agntz, byt)
5. Configure `~/.config/octo/config.toml`
6. Start services:

   ```bash
   octo serve
   cd frontend && bun dev
   ```

Open `http://localhost:3000`. Check backend logs for dev credentials.

---

### Use container mode

```bash
docker build -t octo-dev:latest -f container/Dockerfile .
```

Configure the backend to use containers in `~/.config/octo/config.toml`:

```toml
[container]
runtime = "docker"
default_image = "octo-dev:latest"
```

## Project Structure

```
backend/          Rust backend (API, sessions, auth, runner)
  crates/
    octo/         Main server crate
    octo-protocol/ Canonical agent protocol types
    octo-files/   File server
    octo-browser/ Browser automation
    octo-scaffold/ Project scaffolding
frontend/         React/TypeScript UI (chat, files, terminal, canvas, memory)
deploy/           Systemd service configs, deployment scripts
docs/             Architecture docs, design specs
scripts/          Build and utility scripts
tools/            CLI tools and utilities
```

## Configuration

See [backend/crates/octo/examples/config.toml](./backend/crates/octo/examples/config.toml) for the full reference.

### Edit the config file (`~/.config/octo/config.toml`)

```toml
[server]
port = 8080

[local]
enabled = true
single_user = false  # true for single-user, false for multi-user

[local.linux_users]
enabled = true       # multi-user Linux isolation

[runner]
# Pi session storage directory
pi_sessions_dir = "~/.local/share/pi/sessions"

[mmry]
enabled = true
local_service_url = "http://localhost:8081"

[hstry]
enabled = true

[auth]
dev_mode = true
```

### Set frontend environment (`.env.local`)

```bash
VITE_CONTROL_PLANE_URL=http://localhost:8080
```

Run `./setup.sh` to generate configuration files automatically.

## API

The primary interface is WebSocket-based via `/api/ws/mux` (multiplexed WebSocket). The frontend communicates over two channels:

- `agent` -- Canonical agent protocol (commands + events)
- `system` -- System notifications

---

### Use REST endpoints

| Endpoint | Description |
|----------|-------------|
| `POST /api/sessions` | Create session |
| `GET /api/sessions` | List sessions |
| `DELETE /api/sessions/:id` | Delete session |
| `GET /api/workspace/memories` | List memories |
| `POST /api/workspace/memories/search` | Search memories |
| `GET /api/onboarding` | Get onboarding state |
| `POST /api/admin/users` | Create user (admin) |

## Development

| Component | Build | Lint | Test |
|-----------|-------|------|------|
| backend | `cargo build` | `cargo clippy && cargo fmt --check` | `cargo test` |
| frontend | `bun run build` | `bun run lint` | `bun run test` |

## CLI

```bash
# Server
octo serve                    # Start API server
octo config show              # Show configuration

# Control
octoctl status                # Check server health
octoctl session list          # List sessions
```

---

### Use agent tools

| Tool | Purpose |
|------|---------|
| `agntz` | Memory, issues, mail, file reservations |
| `byt` | Cross-repo governance (catalog, schemas, releases) |
| `sx` | External searches via SearXNG |

## Documentation

- [SETUP.md](./SETUP.md) -- Installation guide
- [AGENTS.md](./AGENTS.md) -- Agent development guidelines
- [docs/design/canonical-protocol.md](./docs/design/canonical-protocol.md) -- Canonical agent protocol spec
- [deploy/systemd/README.md](./deploy/systemd/README.md) -- Systemd service setup
- [backend/crates/octo/examples/config.toml](./backend/crates/octo/examples/config.toml) -- Full configuration reference

## License

Proprietary
