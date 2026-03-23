# Oqto Backend

Backend server for the AI Agent Workspace Platform. Orchestrates agent sessions with Pi, hstry (chat history), and eavs (LLM proxy).

## Binaries

- **oqto** - Main server binary (API + WebSocket)
- **oqtoctl** - Control CLI for managing users, sessions, and configuration
- **oqto-runner** - Per-user agent process manager
- **oqto-files** - File server for workspace access
- **pi-bridge** - HTTP/WebSocket bridge for Pi in containers
- **oqto-sandbox** - Sandbox wrapper (bwrap/sandbox-exec)

## Quick Start

```bash
# 1. Install Rust
rustup default stable

# 2. Build and run
cargo run --bin oqto -- serve

# Or with custom options
cargo run --bin oqto -- serve --port 8080
```

## Docker

For the all-in-one Docker deployment, see `deploy/docker/`:

```bash
cd deploy/docker
cp .env.example .env  # Add your API keys
docker compose up -d
```

Or build the image directly:

```bash
# From repo root
docker build -f deploy/docker/Dockerfile -t oqto:latest .
```

## Features

- Session orchestration in local mode (direct processes) or container mode (Docker/Podman)
- Per-user runner daemon managing agent harnesses
- JWT-based authentication with invite code registration
- Multiplexed WebSocket (agent, files, terminal channels)
- RESTful API for session and user management
- Eavs integration for LLM proxy and model metadata
- hstry integration for persistent chat history
