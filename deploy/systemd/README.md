# Octo Systemd Services

This directory contains systemd service files for running Octo in production.

## Architecture

### Multi-User Mode (Recommended for Production)

```
                    ┌─────────────────────┐
                    │    octo.service     │
                    │  (control plane)    │
                    │  runs as: octo      │
                    └──────────┬──────────┘
                               │ Unix sockets
              ┌────────────────┼────────────────┐
              │                │                │
              ▼                ▼                ▼
    ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
    │ octo-runner     │ │ octo-runner     │ │ octo-runner     │
    │ (user: alice)   │ │ (user: bob)     │ │ (user: ...)     │
    │                 │ │                 │ │                 │
    │ Spawns:         │ │ Spawns:         │ │                 │
    │ - opencode      │ │ - opencode      │ │                 │
    │ - fileserver    │ │ - fileserver    │ │                 │
    │ - ttyd          │ │ - ttyd          │ │                 │
    └─────────────────┘ └─────────────────┘ └─────────────────┘
```

The runner provides OS-level isolation: each user's processes run with their
Linux UID/GID, and the backend cannot directly access user files.

## Service Files

### octo.service
Main control plane server. Handles API requests and routes operations to
per-user runners via Unix sockets.

### octo-runner.service (User Service)
Per-user runner daemon that manages processes for that user. Runs as a systemd
user service (`systemctl --user`). Each user installs this in their own
`~/.config/systemd/user/` directory.

The runner:
- Listens on `$XDG_RUNTIME_DIR/octo-runner.sock`
- Spawns opencode, fileserver, ttyd as the user
- Provides filesystem access to user's workspace
- Enforces sandbox restrictions if configured

### octo-agent@.service (Deprecated)
Legacy template service for per-user agents. Replaced by octo-runner for better
isolation. Kept for backwards compatibility.

### octo-user.service (Deprecated)
Legacy user-level service. Replaced by octo-runner.service.

## Installation

### 1. Backend Service (System-wide)

```bash
# As root
cp octo.service /etc/systemd/system/
systemctl daemon-reload

# Create octo system user
useradd -r -s /usr/sbin/nologin octo

# Create directories
mkdir -p /var/lib/octo /etc/octo
chown octo:octo /var/lib/octo

# Start main service
systemctl enable --now octo
```

### 2. Per-User Runner Setup

For each platform user that will use Octo:

```bash
# As root: Enable lingering so user services start at boot
loginctl enable-linger <username>

# As the user: Install runner service
mkdir -p ~/.config/systemd/user
cp octo-runner.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now octo-runner
```

### Automated User Provisioning

Use `octoctl` to create users with proper setup:

```bash
octoctl user create alice --email alice@example.com
```

This automatically:
1. Creates the Linux user if needed
2. Sets up home directory from skeleton
3. Enables systemd lingering
4. Installs and starts the runner service

### Single-User Development Mode

For local development, the runner isn't required. The backend runs processes directly:

```bash
# Just start the backend
octo serve --mode local --single-user
```

## Socket Communication

The backend communicates with per-user runners via Unix sockets:

- Runner socket: `/run/user/{uid}/octo-runner.sock`

The backend looks up the user's UID and connects to their runner socket.
Socket permissions (owned by the user, mode 0700 on parent directory)
ensure cross-user isolation.

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

# User runner logs (as root)
journalctl --user-unit octo-runner -M alice@

# User runner logs (as the user)
journalctl --user -u octo-runner -f
```

## Security Notes

### Isolation Model

1. **Backend** runs as `octo` system user with no access to user home directories
2. **Runners** run as individual Linux users with access only to their own files
3. **Socket permissions** prevent users from connecting to other users' runners
4. **Sandbox** (optional) further restricts what processes can access

### Recommended Hardening

1. Ensure `/run/user/{uid}` directories have mode 0700
2. Use separate Linux users for each platform user
3. Enable sandbox restrictions in `/etc/octo/sandbox.toml`
4. Consider network isolation for strict deployments
