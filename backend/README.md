# Octo Backend

Backend server for the AI Agent Workspace Platform. Orchestrates containerized development environments with opencode, fileserver, and web terminal access.

## Binaries

- **octo** - Main server binary
- **octoctl** - Control CLI for managing containers, sessions, and images

## Prerequisites

### Container Image (Required)

The backend requires the `octo-dev:latest` container image to create sessions. **The server will fail to start if this image is not found.**

Build the image from the repository root:

```bash
# From the repository root
docker build -t octo-dev:latest -f container/Dockerfile .

# Or using podman
podman build -t octo-dev:latest -f container/Dockerfile .

# Or use octoctl
octoctl image build
```

You can use a different image by setting `default_image` in your config or passing `--image` to the serve command.

### Container Runtime

Either Docker or Podman must be installed and running:
- **Docker**: Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) or Docker Engine
- **Podman**: Install via your package manager (`apt install podman`, `brew install podman`, etc.)

## Quick Start

```bash
# 1. Build the container image first (see Prerequisites)
docker build -t octo-dev:latest -f container/Dockerfile .

# 2. Install Rust
rustup default stable

# 3. Build and run
cargo run --bin octo -- serve

# Or with custom options
cargo run --bin octo -- serve --port 8080 --workspace-root ~/projects
```

## Features

- Session orchestration with Docker (macOS dev) or Podman (Linux prod)
- Per-session containerized environments with opencode, fileserver, and ttyd
- JWT-based authentication with invite code registration
- Automatic container runtime detection (Docker preferred on macOS)
- RESTful API for session management
- Proxy endpoints for opencode, files, and terminal access
- **Graceful shutdown**: Automatically stops all containers when server exits
- Optional CodexBar usage endpoint (requires `codexbar` on PATH)

## CodexBar Integration

If `codexbar` is installed, the backend exposes `/api/codexbar/usage`. The handler tries common CLI flags and expects JSON output. If your local `codexbar` does not support JSON output flags, update the CLI or disable the dashboard card.

## CLI Overview

### octo (Server)

```bash
cargo run --bin octo -- --help
```

Key subcommands:

- `serve` - Start the HTTP API server
- `init` - Create config directories and default files
- `config show|path|reset` - Inspect the effective configuration
- `invite-codes generate|list|revoke` - Manage user invite codes
- `completions <shell>` - Emit shell completions

### octoctl (Control CLI)

```bash
cargo run --bin octoctl -- --help
```

Key subcommands:

- `status` - Check server status
- `session list|get|stop|resume|delete|upgrade` - Manage sessions
- `container refresh|cleanup|list|stop-all` - Manage containers
- `image check|pull|build` - Manage container images

Example usage:

```bash
# Check server status
octoctl status

# List all sessions
octoctl session list

# Force refresh all containers (rebuild from latest image)
octoctl container refresh

# Only refresh containers with outdated images
octoctl container refresh --outdated-only

# Stop all running containers
octoctl container stop-all

# Build new container image
octoctl image build --no-cache
```

## Container Runtime

The backend automatically detects and uses the appropriate container runtime:

- **macOS (dev)**: Prefers Docker Desktop
- **Linux (prod)**: Prefers Podman

You can override this in config.toml:

```toml
[container]
runtime = "docker"  # or "podman"
# binary = "/usr/local/bin/docker"  # optional custom path
default_image = "octo-dev:latest"
base_port = 41820
```

Or via environment variables:
```bash
OCTO__CONTAINER__RUNTIME=docker
```

## Configuration

- Default config path: `$XDG_CONFIG_HOME/octo/config.toml` (or `%APPDATA%\octo\config.toml` on Windows). Override with `--config <path>`.
- Sample configuration with inline comments is available at `examples/config.toml`.
- Data and state directories default to `$XDG_DATA_HOME/octo` and `$XDG_STATE_HOME/octo` (falling back to `~/.local/share` and `~/.local/state` when unset). Override inside the config file.
- Values support `~` expansion and environment variables (e.g. `$HOME/logs/app.log`).

## Authentication

- API requests should send `Authorization: Bearer <jwt>` headers. The backend now accepts the Bearer scheme regardless of casing and ignores extra whitespace, making it compatible with diverse HTTP clients.
- Browser-based flows (SSE/EventSource or WebSocket fallbacks) can omit the header and rely on the `auth_token` HttpOnly cookie that is issued during login. Cookies inherit the standard SameSite=Lax policy so same-origin requests always include them.

## Graceful Shutdown

When the octo server receives SIGTERM or SIGINT (Ctrl+C), it:

1. Stops accepting new requests
2. Stops all running session containers gracefully
3. Waits for in-flight requests to complete
4. Exits cleanly

This ensures no orphan containers are left running when the server exits.

## Development Workflow

- Format the codebase:

  ```bash
  cargo fmt
  ```

- Run the test suite:

  ```bash
  cargo test
  ```

- Recommended lint pass during active development:

  ```bash
  cargo clippy --all-targets --all-features
  ```

- Generate completions for your shell:

  ```bash
  cargo run --bin octo -- completions bash > target/octo.bash
  ```

## Project Structure

```
backend/
  src/
    api/          # HTTP routes and handlers
    auth/         # JWT authentication
    container/    # Docker/Podman runtime abstraction
    ctl/          # octoctl CLI
    db/           # SQLite database
    invite/       # Invite code management
    session/      # Session orchestration
    user/         # User management
  examples/
    config.toml   # Example configuration
  migrations/     # SQL migrations
```

## API Endpoints

- `POST /api/sessions` - Create a new session
- `GET /api/sessions` - List all sessions
- `GET /api/sessions/:id` - Get session details
- `DELETE /api/sessions/:id` - Stop and remove a session
- `POST /api/sessions/:id/stop` - Stop a session (preserves container)
- `POST /api/sessions/:id/resume` - Resume a stopped session
- `POST /api/sessions/:id/upgrade` - Upgrade session to latest image
- `GET /sessions/:id/opencode/*` - Proxy to opencode
- `GET /sessions/:id/files/*` - Proxy to fileserver
- `GET /sessions/:id/terminal` - WebSocket proxy to ttyd
- `GET /session/:id/term` - WebSocket proxy to ttyd (alias)

The terminal proxy accepts connections during session startup and retries connecting to ttyd for a short period so clients can open the WebSocket immediately after creating a session.

During session startup, proxy requests that hit a not-yet-listening service return `503 Service Unavailable` (retryable) instead of `502 Bad Gateway`. If a container exits unexpectedly, the session is reconciled to `failed` when fetched via the API.
