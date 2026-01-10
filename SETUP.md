# Octo Setup Guide

Comprehensive setup and installation guide for self-hosting Octo on your own infrastructure.

## Component Overview

| Component | Type | Required | Purpose |
|-----------|------|----------|---------|
| **octo** | Core | Yes | Control plane server and session orchestration |
| **fileserver** | Core | Yes | File server for workspace access |
| **opencode** | Agent Runtime | Yes (local mode) | AI agent CLI that runs in sessions |
| **ttyd** | Agent Runtime | Yes (local mode) | Web terminal for browser access |
| **pi** | Agent Runtime | Yes | Main chat/LLM interface |
| **docker/podman** | Runtime | Yes (container mode) | Container runtime for isolation |
| **agntz** | Tool | Recommended | Agent operations (memory, issues, mail, reservations) |
| **mmry** | Tool | Optional | Memory system (integrated with Octo API) |
| **trx** | Tool | Optional | Task tracking (integrated with Octo API) |
| **mailz** | Tool | Optional | Agent messaging and coordination |

## Overview

Octo is a self-hosted AI agent workspace platform. This guide covers all prerequisites and installation steps for both local and container modes.

## Quick Start

### Option 1: Interactive Setup Script (Development/Local)

For development or single-machine setup:

```bash
./setup.sh
```

The script will:
1. Detect your OS and system configuration
2. Prompt for user mode (single-user or multi-user)
3. Prompt for backend mode (local processes or containers)
4. Install all required dependencies
5. Build Octo components
6. Generate configuration files
7. Install system services (optional)

For non-interactive installation:

```bash
OCTO_USER_MODE=single OCTO_BACKEND_MODE=local ./setup.sh --non-interactive
```

### Option 2: Ansible Playbook (Production/Server)

For production server deployment with hardening:

```bash
cd deploy/ansible
cp inventory.yml.example inventory.yml
# Edit inventory.yml with your server details
ansible-playbook -i inventory.yml octo.yml
```

The Ansible playbook:
- Hardens SSH (key-only auth, strong ciphers)
- Configures fail2ban and UFW firewall
- Enables automatic security updates
- Installs all Octo dependencies including trash-cli
- Sets up systemd services
- Creates octo system user

See [deploy/ansible/README.md](./deploy/ansible/README.md) for details.

## Prerequisites

### Core System Requirements

| Tool | Purpose | Installation |
|------|---------|--------------|
| **git** | Version control | `apt install git` / `brew install git` |
| **curl** | HTTP client for downloads | `apt install curl` / `brew install curl` |
| **Rust** | Toolchain for building components | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh -s -- -y` |
| **Bun** | JavaScript runtime for frontend | `curl -fsSL https://bun.sh/install \| bash` |

### Container Runtime (Container Mode Only)

Required only if using container mode:

| Tool | Purpose | Installation |
|------|---------|--------------|
| **docker** | Container runtime (macOS/Linux) | [Docker Desktop](https://www.docker.com/products/docker-desktop/) |
| **podman** | Container runtime (Linux production) | `apt install podman` / `dnf install podman` |

**Note**: macOS multi-user mode requires Docker Desktop.

## Core Components

### Octo Backend (octo)

The control plane server that orchestrates agent sessions.

**Installation**:
```bash
# From repository root
cd backend
cargo install --path .
```

**Binaries built**:
- `octo` - Main server binary
- `octo-runner` - Per-user process runner (multi-user Linux mode)
- `pi-bridge` - Bridge for container Pi mode (container mode)

**Configuration**: `~/.config/octo/config.toml`

### Fileserver (fileserver)

File server for workspace access. Provides HTTP endpoints for browsing and retrieving files.

**Installation**:
```bash
cd fileserver
cargo install --path .
```

**Usage**: Automatically started by octo for each session.

### Frontend

React-based web UI for managing agents, chat, and terminals.

**Installation**:
```bash
cd frontend
bun install
bun run build
```

**Development**:
```bash
cd frontend
bun dev
```

**Environment variables** (`.env.local`):
```bash
VITE_CONTROL_PLANE_URL=http://localhost:8080
```

## Agent Runtime Components

### OpenCode (opencode)

OpenCode CLI - the AI agent that runs within sessions. Required for local mode.

**Installation**:
```bash
curl -fsSL https://opencode.ai/install | bash
```

**Configuration**: `~/.config/opencode/opencode.json`

### ttyd

Web terminal that provides browser-based terminal access to sessions.

**Installation**:
```bash
# macOS
brew install ttyd

# Debian/Ubuntu
apt-get install -y ttyd

# Arch Linux
pacman -S ttyd

# Fedora
dnf install -y ttyd
```

## Agent Tools

### agntz

Agent operations CLI for day-to-day workflows.

**Purpose**: Memory management, issue tracking, mail, and file reservations.

**Installation**:
```bash
# Via cargo (if published to crates.io)
cargo install agntz

# Or from git
cargo install --git https://github.com/byteowlz/agntz.git
```

**Key commands**:
```bash
agntz memory search "query"     # Search memories
agntz memory add "insight"      # Add a memory
agntz ready                     # Show unblocked issues
agntz issues                    # List all issues
agntz mail inbox                # Check messages
agntz reserve src/file.rs       # Reserve file for editing
agntz release src/file.rs       # Release reservation
```

## Optional Tools

### mmry (Memory System)

Persistent memory storage and retrieval for AI agents. Integrated with Octo API.

**Installation**:
```bash
cargo install mmry
```

### trx (Task Tracking)

Task tracking and issue management. Integrated with Octo API for displaying issues in the UI.

**Installation**:
```bash
cargo install trx
```

**Usage**:
```bash
trx ready              # Show unblocked issues
trx create "Title" -t task -p 2   # Create issue (types: bug/feature/task/epic/chore, priority: 0-4)
trx update <id> --status in_progress
trx close <id> -r "Done"
trx sync               # Commit .trx/ changes
```

### mailz (Agent Messaging)

Cross-agent communication and coordination system.

**Installation**:
```bash
cargo install mailz
```

## Shell Tools (Recommended)

Recommended tools for agents to use within sessions. These improve agent productivity.

| Tool | Purpose | Installation |
|------|---------|--------------|
| **tmux** | Terminal multiplexer | `apt install tmux` / `brew install tmux` |
| **fd** | Fast file finder | `apt install fd-find` / `brew install fd` |
| **ripgrep (rg)** | Fast search | `apt install ripgrep` / `brew install ripgrep` |
| **yazi** | Terminal file manager | `cargo install yazi-fm yazi-cli` |
| **zsh** | Z shell | `apt install zsh` / `brew install zsh` |
| **zoxide** | Smart directory navigation | `cargo install zoxide` |

## Pi (Main Chat)

The main chat/LLM interface used by Octo for AI conversations.

**Installation**:
```bash
cargo install pi
```

**Configuration**:
Pi is configured via the `[pi]` section in `~/.config/octo/config.toml`:

```toml
[pi]
enabled = true
executable = "pi"
default_provider = "anthropic"
default_model = "claude-sonnet-4-20250514"
runtime_mode = "local"  # or "container" or "runner"
```

**Runtime modes**:
- `local` - Pi runs directly on the host (single-user local mode)
- `container` - Pi runs inside containers (container mode)
- `runner` - Pi runs via octo-runner for multi-user Linux mode

## Deployment Modes

### Local Mode

Runs all components as native processes on the host.

**Prerequisites**:
- opencode binary
- fileserver binary
- ttyd binary
- pi binary

**Best for**:
- Development
- Single-user setups
- Trusted environments
- Bare-metal or LXC containers without container runtime

**Configuration**:
```toml
[backend]
mode = "local"

[local]
enabled = true
opencode_binary = "opencode"
fileserver_binary = "fileserver"
ttyd_binary = "ttyd"
workspace_dir = "$HOME/octo/{user_id}"
single_user = false
```

### Container Mode

Runs each session in isolated Docker/Podman containers.

**Prerequisites**:
- Docker or Podman runtime
- Container image: `octo-dev:latest`

**Best for**:
- Multi-user deployments
- Production environments
- Untrusted code execution
- Complete isolation between sessions

**Building the container image**:
```bash
docker build -t octo-dev:latest -f container/Dockerfile .
```

**Configuration**:
```toml
[backend]
mode = "container"

[container]
runtime = "docker"  # or "podman"
default_image = "octo-dev:latest"
base_port = 41820
```

## User Modes

### Single-User Mode

All sessions use the same workspace. Simpler setup, no user management.

**Best for**: Personal laptops, single-developer servers.

**Configuration**:
```toml
[local]
single_user = true
workspace_dir = "$HOME/workspace"
```

### Multi-User Mode

Each user gets isolated workspace with user authentication and management.

**Best for**: Teams, shared servers.

**Linux User Isolation** (multi-user local mode only):
Creates dedicated Linux users for process isolation.

```toml
[local.linux_users]
enabled = true
prefix = "octo_"
uid_start = 2000
group = "octo"
shell = "/bin/bash"
use_sudo = true
create_home = true
```

## Service Installation

### Linux (systemd)

**Single-user (user-level service)**:
```bash
# Install
~/.config/systemd/user/octo.service

# Enable and start
systemctl --user enable --now octo

# Check status
systemctl --user status octo
journalctl --user -u octo -f
```

**Multi-user (system-level service)**:
```bash
# Install system service
cp octo.service /etc/systemd/system/

# Install per-user runner template
cp octo-runner.service /etc/systemd/user/

# Create octo system user
useradd -r -s /usr/sbin/nologin -d /var/lib/octo octo

# Create directories
mkdir -p /var/lib/octo /etc/octo /run/octo
chown octo:octo /var/lib/octo /run/octo

# Enable and start
systemctl enable --now octo

# Users enable their own runner
systemctl --user enable --now octo-runner
```

### macOS (launchd)

**Installation**:
```bash
# Install plist
~/Library/LaunchAgents/ai.octo.server.plist

# Load service
launchctl load ~/Library/LaunchAgents/ai.octo.server.plist

# Check status
launchctl list | grep octo
```

## Configuration Reference

Full configuration reference available at `backend/examples/config.toml`.

Key configuration sections:

- `[logging]` - Log level and output
- `[runtime]` - Worker pool and timeout settings
- `[container]` - Container runtime and image settings
- `[local]` - Local mode configuration
- `[eavs]` - LLM proxy integration
- `[auth]` - Authentication and authorization
- `[sessions]` - Session behavior settings
- `[scaffold]` - Agent scaffolding configuration
- `[pi]` - Main chat configuration

## Verification

After installation, verify all components are working:

```bash
# Check binaries
which octo
which fileserver
which opencode
which ttyd
which pi

# Check agent tools
which agntz

# Check optional tools (if installed)
which mmry
which trx
which mailz

# Start server (local mode)
octo serve --local-mode

# Start server (container mode)
octo serve

# Start frontend (development)
cd frontend && bun dev

# Access web UI
open http://localhost:3000
```

## Troubleshooting

### Container image not found

Error: `Failed to find container image`

Solution: Build the container image:
```bash
docker build -t octo-dev:latest -f container/Dockerfile .
```

### Port already in use

Error: `Address already in use`

Solution: Change port in config or kill the process using the port:
```bash
# Find process using port 8080
lsof -i :8080
kill -9 <PID>
```

### opencode not found

Error: `opencode: command not found`

Solution: Install opencode:
```bash
curl -fsSL https://opencode.ai/install | bash
```

### Permission prompts or errors not showing (UI)

Symptoms: OpenCode requests permissions or hits errors, but the web UI shows nothing.

Solution:
- Update Octo to a recent build (the UI normalizes both `tool`/`input` and `permission_type`/`pattern` payload shapes).
- Enable WebSocket debug logging: `localStorage.setItem("debug:ws", "1")`

### Permission denied (systemd)

Error: `Failed to start service: Permission denied`

Solution: For user-level services, use `systemctl --user`. For system services, use sudo or run as root.

### Container runtime not found

Error: `No container runtime found`

Solution: Install Docker or Podman:
```bash
# Docker
curl -fsSL https://get.docker.com -o get-docker.sh
sh get-docker.sh

# Podman (Linux)
apt install podman
```

## Additional Resources

- [README.md](./README.md) - Project overview and architecture
- [backend/README.md](./backend/README.md) - Backend documentation
- [frontend/README.md](./frontend/README.md) - Frontend documentation
- [deploy/systemd/README.md](./deploy/systemd/README.md) - Systemd service setup
- [deploy/ansible/README.md](./deploy/ansible/README.md) - Ansible deployment playbook
- [AGENTS.md](./AGENTS.md) - Agent development guidelines
- [backend/examples/config.toml](./backend/examples/config.toml) - Full config reference
