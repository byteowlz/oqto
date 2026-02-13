---
name: octo-setup
description: Guide users through Octo platform setup, troubleshoot installation issues, and verify deployment configuration. Use when the user is installing Octo, configuring the platform, or encountering setup-related problems.
---

# Octo Setup Guide for AI Agents

Octo is a self-hosted AI agent workspace platform with multiple deployment modes and configuration options.

## Quick Reference

| Component | Required For | Check Command |
|-----------|--------------|---------------|
| **Rust** | Building | `cargo --version` |
| **Bun** | Frontend | `bun --version` |
| **Git** | Clone repos | `git --version` |
| **Docker/Podman** | Container mode | `docker --version` or `podman --version` |
| **ttyd** | Web terminal | `ttyd --version` |
| **pi** | Main chat | `pi --version` |

## Required Repositories

### Core Platform

| Repository | Purpose | Installation |
|------------|---------|--------------|
| [octo](https://github.com/byteowlz/octo) | Main platform (this repo) | `git clone https://github.com/byteowlz/octo` |

### Backend Services

| Repository | Purpose | Installation |
|------------|---------|--------------|
| [hstry](https://github.com/byteowlz/hstry) | Chat history storage (gRPC + SQLite) | `cargo install --git https://github.com/byteowlz/hstry` |
| [mmry](https://github.com/byteowlz/mmry) | Memory system with semantic search | `cargo install --git https://github.com/byteowlz/mmry` |
| [trx](https://github.com/byteowlz/trx) | Issue and task tracking | `cargo install --git https://github.com/byteowlz/trx` |

### Agent Runtime

| Repository | Purpose | Installation |
|------------|---------|--------------|
| [pi](https://github.com/byteowlz/pi) | Main chat/LLM interface | `cargo install --git https://github.com/byteowlz/pi` |
| [agntz](https://github.com/byteowlz/agntz) | Agent toolkit (memory, issues, mail) | `cargo install --git https://github.com/byteowlz/agntz` |

### Voice Mode (Optional)

| Repository | Purpose | Installation |
|------------|---------|--------------|
| [eaRS](https://github.com/byteowlz/ears) | Speech-to-text (STT) service | `cargo install --git https://github.com/byteowlz/ears` |
| [kokorox](https://github.com/byteowlz/kokorox) | Text-to-speech (TTS) service | `cargo install --git https://github.com/byteowlz/kokorox` |

### Search Service (Optional)

| Repository | Purpose | Installation |
|------------|---------|--------------|
| [SearXNG](https://github.com/searxng/searxng) | Self-hosted meta-search engine | See [SearXNG Setup](#searxng-setup) below |

### Cross-Repo Tools

| Repository | Purpose | Installation |
|------------|---------|--------------|
| [sx](https://github.com/byteowlz/sx) | External search via SearXNG | `go install https://github.com/byteowlz/sx` |
| [scrpr](https://github.com/byteowlz/scrpr) | Get website content | `go install https://github.com/byteowlz/scrpr` |

### Quick Install All Tools

```bash
# Install agntz first, then use it to install all tools
cargo install --git https://github.com/byteowlz/agntz

# Install all byteowlz tools via agntz
agntz tools install all

# Or install specific tools
agntz tools install mmry trx mailz sx
```

## Setup Methods

### 1. Interactive Setup Script (Recommended)

```bash
./setup.sh
```

The script handles:

- OS detection and prerequisite installation
- User mode selection (single/multi)
- Backend mode selection (local/container)
- Dependency installation (Rust, Bun, shell tools)
- Building all components
- Configuration file generation
- Optional service installation

### 2. Non-Interactive Setup

```bash
# Development mode
OCTO_USER_MODE=single OCTO_BACKEND_MODE=local ./setup.sh --non-interactive

# Production mode with hardening
OCTO_DEV_MODE=false \
OCTO_HARDEN_SERVER=yes \
OCTO_SETUP_CADDY=yes \
OCTO_DOMAIN=octo.example.com \
./setup.sh --non-interactive
```

### 3. Manual Setup

See SETUP.md for step-by-step manual installation.

## Configuration Modes

### User Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| **Single-user** | Shared workspace, no auth | Personal laptops, single dev |
| **Multi-user** | Isolated workspaces, auth required | Teams, shared servers |

### Backend Modes

| Mode | Description | Requirements |
|------|-------------|--------------|
| **Local** | Native processes via octo-runner | opencode, fileserver, ttyd, pi binaries |
| **Container** | Docker/Podman isolation | Docker/Podman runtime, octo-dev image |

**Note**: macOS multi-user requires container mode.

### Deployment Modes

| Mode | Auth | TLS | Use Case |
|------|------|-----|----------|
| **Development** | Dev users, no JWT | HTTP only | Local development |
| **Production** | JWT + invite codes | HTTPS via Caddy | Servers, production |

## Common Setup Issues

### Prerequisites Missing

```bash
# Check what's missing
./setup.sh  # Run interactively to install

# Or install manually:
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Bun
curl -fsSL https://bun.sh/install | bash

# ttyd (Debian/Ubuntu)
sudo apt-get install -y ttyd

# ttyd (macOS)
brew install ttyd
```

### Container Image Not Found

```bash
# Build the image
docker build -t octo-dev:latest -f container/Dockerfile .
```

### Port Already in Use

```bash
# Find process
lsof -i :8080  # Octo backend port
lsof -i :3000  # Frontend port
lsof -i :8888  # SearXNG default port

# Kill or change ports in:
# ~/.config/octo/config.toml (Octo backend port)
# /etc/searxng/settings.yml (SearXNG port)
```

### Permission Denied (systemd)

- User services: Use `systemctl --user`
- System services: Use `sudo` or run as root

### Build Failures

```bash
# Clean and rebuild
cargo clean
cd backend && cargo build --release
cd fileserver && cargo build --release
cd frontend && bun install && bun run build
```

## Environment Variables Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `OCTO_USER_MODE` | single | single or multi |
| `OCTO_BACKEND_MODE` | local | local or container |
| `OCTO_CONTAINER_RUNTIME` | auto | docker, podman, or auto |
| `OCTO_INSTALL_DEPS` | yes | Install dependencies |
| `OCTO_INSTALL_SERVICE` | yes | Install systemd/launchd service |
| `OCTO_INSTALL_AGENT_TOOLS` | yes | Install agntz, mmry, trx, mailz, sx |
| `OCTO_DEV_MODE` | - | true for dev mode, false for production |
| `OCTO_LOG_LEVEL` | info | error, warn, info, debug, trace |
| `OCTO_SETUP_CADDY` | - | yes to install Caddy reverse proxy |
| `OCTO_DOMAIN` | - | Domain for HTTPS |
| `OCTO_HARDEN_SERVER` | - | yes to enable server hardening (Linux) |

## Post-Setup Verification

```bash
# Check binaries
which octo && octo --version
which fileserver && fileserver --version
which opencode && opencode --version
which ttyd && ttyd --version
which pi && pi --version

# Check config
cat ~/.config/octo/config.toml

# Test backend (in one terminal)
octo serve --local-mode

# Test frontend (in another terminal)
cd frontend && bun dev

# Access UI
open http://localhost:3000
```

## Configuration Files

| File | Purpose |
|------|---------|
| `~/.config/octo/config.toml` | Main server config |
| `~/.config/opencode/opencode.json` | OpenCode agent config |
| `frontend/.env.local` | Frontend environment |
| `/etc/caddy/Caddyfile` | Reverse proxy config (if using Caddy) |

## Service Management

### Linux (systemd)

```bash
# User service
systemctl --user enable --now octo
systemctl --user status octo
journalctl --user -u octo -f

# System service
sudo systemctl enable --now octo
sudo systemctl status octo
sudo journalctl -u octo -f
```

### macOS (launchd)

```bash
launchctl load ~/Library/LaunchAgents/ai.octo.server.plist
launchctl list | grep octo
```

## Production Deployment Checklist

- [ ] Generated secure JWT secret (64+ characters)
- [ ] Created admin user with strong password
- [ ] Configured Caddy with proper domain
- [ ] Enabled HTTPS (Caddy auto-SSL)
- [ ] Set up firewall (UFW/firewalld)
- [ ] Configured fail2ban
- [ ] Hardened SSH (key auth only)
- [ ] Enabled automatic security updates
- [ ] Set correct file permissions
- [ ] Tested backup/restore procedures

## When to Use Each Setup Method

| Scenario | Method |
|----------|--------|
| First-time setup, development | `./setup.sh` (interactive) |
| CI/CD automation | `./setup.sh --non-interactive` |
| Production server with hardening | `OCTO_DEV_MODE=false ./setup.sh` |
| Multi-server deployment | Ansible playbook |
| Container-only deployment | Manual Docker setup |

## Troubleshooting Commands

```bash
# Check logs
journalctl --user -u octo -f  # User service
sudo journalctl -u octo -f     # System service

# Check config validity
octo config show

# Test runner connectivity
octoctl status

# Debug WebSocket
# In browser console:
localStorage.setItem("debug:ws", "1")

# Check port bindings
ss -tlnp | grep -E ':(8080|3000|8888)'

# Verify binary locations
ls -la ~/.cargo/bin/octo*
```

## SearXNG Setup

For use with the `sx` search tool, you can install SearXNG natively (no Docker required).

### Option 1: Installation Script (Recommended)

```bash
git clone https://github.com/searxng/searxng.git
cd searxng
sudo -H ./utils/searxng.sh install all
```

### Option 2: Manual Step-by-Step

**Ubuntu/Debian:**

```bash
# Install dependencies
sudo apt-get install -y python3-dev python3-babel python3-venv \
    uwsgi uwsgi-plugin-python3 git build-essential libxslt-dev \
    zlib1g-dev libffi-dev libssl-dev redis-server

# Create searxng user
sudo useradd --shell /bin/bash --system \
    --home-dir "/usr/local/searxng" \
    --comment 'Privacy-respecting metasearch engine' \
    searxng

sudo mkdir "/usr/local/searxng"
sudo chown -R "searxng:searxng" "/usr/local/searxng"

# Clone and install
sudo -H -u searxng -i
git clone "https://github.com/searxng/searxng" \
    "/usr/local/searxng/searxng-src"
cd /usr/local/searxng/searxng-src

# Create virtualenv and install
python3 -m venv /usr/local/searxng/searx-pyenv
source /usr/local/searxng/searx-pyenv/bin/activate
pip install -e .
```

**Configure:** Edit `/etc/searxng/settings.yml`:

```yaml
server:
  port: 8888                    # Default: 8888 (change if conflicts with Octo)
  bind_address: "127.0.0.1"     # Localhost only for security
  secret_key: "your-secret-key" # Change this!

search:
  formats:
    - html
    - json                    # REQUIRED: Enable API access for sx tool
```

**Enable HTTP API:**
By default SearXNG only serves HTML. To use the API with the `sx` tool, you **must** add `json` to the `search.formats` list in `/etc/searxng/settings.yml`.

**Service:** SearXNG runs via uWSGI:

```bash
sudo systemctl start searxng
sudo systemctl enable searxng
```

**Test API:**

```bash
# Should return JSON results
curl 'http://localhost:8888/search?q=hello+world&format=json'
```

**Configure sx tool:**

```bash
# Set SearXNG URL for sx
export SEARXNG_URL=http://localhost:8888
# Or configure in sx config
sx config set searxng.url http://localhost:8888
```

### Why SearXNG in 2026?

SearXNG remains the best FOSS meta-search engine:

- Aggregates results from 246+ search engines
- No tracking or profiling
- Active development and community
- Better than alternatives (Whoogle, LibreY) due to multi-engine aggregation

## Getting Help

1. Check logs: `journalctl --user -u octo -f`
2. Verify config: `octo config show`
3. Check SETUP.md for detailed instructions
4. Check README.md for architecture overview
5. Review backend/examples/config.toml for all options
