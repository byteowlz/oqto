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

If you have a portable install config, hydrate configs directly:

```bash
octo-setup hydrate --install-config octo.install.toml
```

For non-interactive installation:

```bash
OCTO_USER_MODE=single OCTO_BACKEND_MODE=local ./setup.sh --non-interactive
```

### Option 2: Setup Script with Server Hardening (Production)

For production deployment with built-in server hardening (Linux only):

```bash
# Interactive production setup with hardening
OCTO_DEV_MODE=false ./setup.sh

# Or fully automated with all hardening enabled
OCTO_DEV_MODE=false \
OCTO_HARDEN_SERVER=yes \
OCTO_SETUP_CADDY=yes \
OCTO_DOMAIN=octo.example.com \
./setup.sh --non-interactive
```

The setup script with hardening enabled will:
- Configure UFW/firewalld firewall (only allow SSH, HTTP/S)
- Install and configure fail2ban for SSH protection
- Harden SSH (disable password auth, use strong ciphers)
- Enable automatic security updates
- Apply kernel security parameters (sysctl)
- Enable audit logging (auditd)
- Set up Caddy reverse proxy with automatic HTTPS

**Hardening Environment Variables**:

| Variable | Default | Description |
|----------|---------|-------------|
| `OCTO_HARDEN_SERVER` | prompt | Enable server hardening (yes/no) |
| `OCTO_SSH_PORT` | 22 | SSH port number |
| `OCTO_SETUP_FIREWALL` | yes | Configure UFW/firewalld |
| `OCTO_SETUP_FAIL2BAN` | yes | Install and configure fail2ban |
| `OCTO_HARDEN_SSH` | yes | Apply SSH hardening (disables passwords!) |
| `OCTO_SETUP_AUTO_UPDATES` | yes | Enable automatic security updates |
| `OCTO_HARDEN_KERNEL` | yes | Apply kernel security parameters |

> ⚠️ **Warning**: SSH hardening disables password authentication. Ensure you have SSH key access before enabling!

### Option 3: Ansible Playbook (Production/Server)

For more complex deployments or when you need full control:

```bash
cd deploy/ansible
cp inventory.yml.example inventory.yml
# Edit inventory.yml with your server details
ansible-playbook -i inventory.yml octo.yml
```

The Ansible playbook provides the same hardening as `setup.sh --harden-server` plus:
- Creates dedicated octo system user
- Installs all Octo dependencies including trash-cli
- Sets up systemd services
- More granular control via Ansible variables

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

# Octo will attempt to enable lingering and start `octo-runner` automatically
# when creating Linux users (when `local.linux_users.enabled=true`).
# If needed, you can enable it manually for a specific Linux user:
sudo loginctl enable-linger <username>
sudo -u <username> systemctl --user enable --now octo-runner

For non-root Octo backends in local multi-user mode, we recommend using shared
runner sockets:

- Runner socket base: `/run/octo/runner-sockets/<user>/octo-runner.sock`
- Ensure `/run/octo/runner-sockets` exists via tmpfiles (see `systemd/octo-runner.tmpfiles.conf`)
- Ensure the Octo backend user is in the shared `octo` group
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

## Production Deployment

For production deployments, run the setup script and select "Production" when prompted for deployment mode:

```bash
./setup.sh
```

The setup script will:
1. Generate a secure 64-character JWT secret
2. Create an admin user with a secure password
3. Optionally configure Caddy as a reverse proxy with automatic HTTPS

### Caddy Reverse Proxy

Caddy provides automatic HTTPS via Let's Encrypt and serves as a reverse proxy for Octo.

**Installation via setup.sh**:
The setup script will install and configure Caddy automatically when you select "Production" mode and agree to set up Caddy.

**Manual Installation**:
```bash
# Debian/Ubuntu
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https curl
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update && sudo apt install -y caddy

# Arch Linux
sudo pacman -S caddy

# macOS
brew install caddy
```

**Caddyfile Example** (`/etc/caddy/Caddyfile`):
```caddyfile
octo.example.com {
    # Backend API
    handle /api/* {
        reverse_proxy localhost:8080
    }
    
    # WebSocket connections
    handle /ws/* {
        reverse_proxy localhost:8080
    }
    
    # WebSocket upgrade handling
    @websockets {
        header Connection *Upgrade*
        header Upgrade websocket
    }
    handle @websockets {
        reverse_proxy localhost:8080
    }
    
    # Frontend
    handle {
        reverse_proxy localhost:3000
    }
    
    # Security headers
    header {
        X-Content-Type-Options nosniff
        X-Frame-Options DENY
        Referrer-Policy strict-origin-when-cross-origin
        -Server
    }
    
    encode gzip zstd
}
```

**Starting Caddy**:
```bash
# Linux (systemd)
sudo systemctl enable --now caddy

# Check status
sudo systemctl status caddy

# View logs
sudo journalctl -u caddy -f

# macOS
sudo caddy start --config /etc/caddy/Caddyfile
```

### Authentication Setup

**Development Mode** (dev_mode = true):
- Uses pre-configured dev users from config file
- No JWT secret required
- Useful for local development

**Production Mode** (dev_mode = false):
- Requires JWT secret (minimum 32 characters)
- Users stored in SQLite database
- New users require invite codes

**JWT Secret Generation**:
```bash
# Generate a secure 64-character secret
openssl rand -base64 48

# Or use the setup script which generates one automatically
```

**Creating the Admin User**:

For a fresh install, use the bootstrap command to create the first admin user:

```bash
# Bootstrap admin user with Linux user + runner (multi-user mode)
# This creates: database user + Linux user + systemd runner
octoctl user bootstrap -u admin -e admin@example.com -p "your-secure-password"

# Database-only (single-user mode or existing Linux user)
octoctl user bootstrap -u admin -e admin@example.com -p "password" --no-linux-user

# Custom Linux username (different from Octo username)
octoctl user bootstrap -u admin -e admin@example.com --linux-user octo_admin

# With a custom database path
octoctl user bootstrap -u admin -e admin@example.com --database /path/to/octo.db

# Non-interactive JSON output (for scripting)
octoctl --json user bootstrap -u admin -e admin@example.com -p "password"
```

The setup script (`./setup.sh --production`) will prompt for admin credentials and show the bootstrap command.

To create additional users after the server is running:

```bash
# Using the CLI (server must be running)
octoctl user create admin2 --email admin2@example.com --role admin

# Generate password hash for config file
htpasswd -nbBC 12 admin yourpassword | cut -d: -f2
```

**Invite Codes for New Users**:
In production mode, new users need invite codes to register:

```bash
# Create a single-use invite code
octo invites create --uses 1

# Create a multi-use invite code (e.g., for a team)
octo invites create --uses 10

# List active invite codes
octo invites list
```

### CORS Configuration

For production deployments with a custom domain, configure allowed origins:

```toml
[auth]
dev_mode = false
jwt_secret = "your-secure-secret-at-least-32-characters"
allowed_origins = ["https://octo.example.com"]
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
