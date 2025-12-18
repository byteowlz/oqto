# Workspace Backend

Backend server for the AI Agent Workspace Platform. Orchestrates containerized development environments with opencode, fileserver, and web terminal access.

## Prerequisites

### Container Image (Required)

The backend requires the `opencode-dev:latest` container image to create sessions. **The server will fail to start if this image is not found.**

Build the image from the repository root:

```bash
# From the repository root
docker build -t opencode-dev:latest -f container/Dockerfile .

# Or using podman
podman build -t opencode-dev:latest -f container/Dockerfile .
```

You can use a different image by setting `default_image` in your config or passing `--image` to the serve command.

### Container Runtime

Either Docker or Podman must be installed and running:
- **Docker**: Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) or Docker Engine
- **Podman**: Install via your package manager (`apt install podman`, `brew install podman`, etc.)

## Quick Start

```bash
# 1. Build the container image first (see Prerequisites)
docker build -t opencode-dev:latest -f container/Dockerfile .

# 2. Install Rust
rustup default stable

# 3. Build and run
cargo run -- serve

# Or with custom options
cargo run -- serve --port 8080 --workspace-root ~/projects
```

## Features

- Session orchestration with Docker (macOS dev) or Podman (Linux prod)
- Per-session containerized environments with opencode, fileserver, and ttyd
- JWT-based authentication with invite code registration
- Automatic container runtime detection (Docker preferred on macOS)
- RESTful API for session management
- Proxy endpoints for opencode, files, and terminal access

## CLI Overview

```bash
cargo run -- --help
```

Key subcommands:

- `serve` - Start the HTTP API server
- `init` - Create config directories and default files
- `config show|path|reset` - Inspect the effective configuration
- `invite-codes generate|list|revoke` - Manage user invite codes
- `completions <shell>` - Emit shell completions

## Container Runtime

The backend automatically detects and uses the appropriate container runtime:

- **macOS (dev)**: Prefers Docker Desktop
- **Linux (prod)**: Prefers Podman

You can override this in config.toml:

```toml
[container]
runtime = "docker"  # or "podman"
# binary = "/usr/local/bin/docker"  # optional custom path
default_image = "opencode-dev:latest"
base_port = 41820
```

Or via environment variables:
```bash
WORKSPACE_BACKEND__CONTAINER__RUNTIME=docker
```

## Configuration

- Default config path: `$XDG_CONFIG_HOME/workspace-backend/config.toml` (or `%APPDATA%\workspace-backend\config.toml` on Windows). Override with `--config <path>`.
- Sample configuration with inline comments is available at `examples/config.toml`.
- Data and state directories default to `$XDG_DATA_HOME/workspace-backend` and `$XDG_STATE_HOME/workspace-backend` (falling back to `~/.local/share` and `~/.local/state` when unset). Override inside the config file.
- Values support `~` expansion and environment variables (e.g. `$HOME/logs/app.log`).

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
  cargo run -- completions bash > target/workspace-backend.bash
  ```

## Project Structure

```
backend/
  src/
    api/          # HTTP routes and handlers
    auth/         # JWT authentication
    container/    # Docker/Podman runtime abstraction
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
- `GET /sessions/:id/opencode/*` - Proxy to opencode
- `GET /sessions/:id/files/*` - Proxy to fileserver
- `GET /sessions/:id/terminal` - WebSocket proxy to ttyd
- `GET /session/:id/term` - WebSocket proxy to ttyd (alias)

The terminal proxy accepts connections during session startup and retries connecting to ttyd for a short period so clients can open the WebSocket immediately after creating a session.

During session startup, proxy requests that hit a not-yet-listening service return `503 Service Unavailable` (retryable) instead of `502 Bad Gateway`. If a container exits unexpectedly, the session is reconciled to `failed` when fetched via the API.
