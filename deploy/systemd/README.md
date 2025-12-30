# Octo Systemd Services

This directory contains systemd service files for running Octo in production.

## Architecture

```
                    ┌─────────────────────┐
                    │    octo.service     │
                    │  (control plane)    │
                    │  runs as: octo      │
                    └──────────┬──────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
              ▼                ▼                ▼
    ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
    │octo-agent@alice │ │octo-agent@bob   │ │octo-agent@...   │
    │ runs as: alice  │ │ runs as: bob    │ │                 │
    └─────────────────┘ └─────────────────┘ └─────────────────┘
```

## Service Files

### octo.service
Main control plane server. Handles API requests and coordinates user agents.

### octo-agent@.service
Template service for per-user agents. Each user gets their own instance running
as their Linux user, providing proper file ownership and isolation.

### octo-user.service
Alternative user-level service for systems using `systemd --user`. Users install
this in their own `~/.config/systemd/user/` directory.

## Installation

### System-wide (recommended for servers)

```bash
# As root
cp octo.service octo-agent@.service /etc/systemd/system/
systemctl daemon-reload

# Create octo system user
useradd -r -s /usr/sbin/nologin octo

# Create directories
mkdir -p /var/lib/octo /etc/octo /run/octo
chown octo:octo /var/lib/octo /run/octo

# Start main service
systemctl enable --now octo

# Start agent for a user
systemctl enable --now octo-agent@alice
```

### Per-user (for development or single-user)

```bash
# As the user
mkdir -p ~/.config/systemd/user
cp octo-user.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now octo-user
```

## Socket Communication

The main octo server communicates with per-user agents via Unix sockets:

- System agents: `/run/octo/agent-{username}.sock`
- User agents: `$XDG_RUNTIME_DIR/octo-agent.sock` (typically `/run/user/{uid}/octo-agent.sock`)

## Configuration

Main config file: `/etc/octo/config.toml`

```toml
[server]
listen = "0.0.0.0:3000"

[backend]
mode = "local"  # or "container" or "auto"

[local]
# Socket directory for agent communication
socket_dir = "/run/octo"
```

## Logs

```bash
# Main server logs
journalctl -u octo -f

# User agent logs
journalctl -u octo-agent@alice -f
```
