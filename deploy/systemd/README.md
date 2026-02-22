# Oqto Systemd Services

This directory contains systemd service files for running Oqto in production.

## Architecture

### Multi-User Mode (Recommended for Production)

```
                    ┌─────────────────────┐
                    │    oqto.service     │
                    │  (control plane)    │
                    │  runs as: oqto      │
                    └──────────┬──────────┘
                               │ Unix sockets
              ┌────────────────┼────────────────┐
              │                │                │
              ▼                ▼                ▼
    ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
    │ oqto-runner     │ │ oqto-runner     │ │ oqto-runner     │
    │ (user: alice)   │ │ (user: bob)     │ │ (user: ...)     │
    │                 │ │                 │ │                 │
    │ Depends on:     │ │ Depends on:     │ │                 │
    │ - hstry         │ │ - hstry         │ │                 │
    │ - eavs          │ │ - eavs          │ │                 │
    │ - mmry          │ │ - mmry          │ │                 │
    │                 │ │                 │ │                 │
    │ Spawns:         │ │ Spawns:         │ │                 │
    │ - pi            │ │ - pi            │ │                 │
    │ - fileserver    │ │ - fileserver    │ │                 │
    │ - ttyd          │ │ - ttyd          │ │                 │
    └─────────────────┘ └─────────────────┘ └─────────────────┘
```

The runner provides OS-level isolation: each user's processes run with their
Linux UID/GID, and the backend cannot directly access user files.

### Service Dependency Chain

All services are systemd **user services** and start automatically on boot
when lingering is enabled. The dependency chain ensures correct startup order:

```
eavs.service  ──┐
hstry.service ──┼──> oqto-runner.service ──> oqto.service
mmry.service  ──┘
```

- `eavs`, `hstry`, `mmry` start first (no interdependencies)
- `oqto-runner` starts after all three are ready (`After=` + `Wants=`)
- `oqto` starts after the runner is ready

## Service Files

### hstry.service (User Service)
Per-user chat history daemon. Provides gRPC API for reading/writing chat
history. Required by oqto-runner for session persistence.

Uses `hstry service run` (foreground mode, suitable for systemd).

### eavs.service (User Service)
Per-user LLM proxy. Provides multi-provider routing, virtual API keys, OAuth
credential management, and cost tracking. Pi connects to eavs for all LLM
API requests.

Uses `eavs serve` (foreground mode, suitable for systemd).

### mmry.service (User Service)
Per-user memory service for semantic search and memory storage. Already
included in previous deployments.

### oqto-runner.service (User Service)
Per-user runner daemon that manages processes for that user. Depends on
hstry, eavs, and mmry services.

The runner:
- Listens on `$XDG_RUNTIME_DIR/oqto-runner.sock`
- Spawns pi, fileserver, ttyd as the user
- Provides filesystem access to user's workspace
- Enforces sandbox restrictions if configured

### oqto.service
Main control plane server. Handles API requests and routes operations to
per-user runners via Unix sockets.

In single-user mode, this runs as a user service. In multi-user production
mode, this runs as a system service under the `oqto` user.

### mmry-embeddings.service (System Service, Optional)
Central embeddings/reranking service shared by all users. Per-user mmry
instances delegate heavy ML operations to this service.

### oqto-agent@.service (Deprecated)
Legacy template service for per-user agents. Replaced by oqto-runner.

### oqto-user.service (Deprecated)
Legacy user-level service. Replaced by oqto-runner.service.

## Installation

### System-wide Installation (Recommended)

Install all user services to `/usr/lib/systemd/user/` so every user gets
them automatically:

```bash
# Install user service units system-wide
sudo install -Dm644 hstry.service /usr/lib/systemd/user/hstry.service
sudo install -Dm644 eavs.service /usr/lib/systemd/user/eavs.service
sudo install -Dm644 oqto-runner.service /usr/lib/systemd/user/oqto-runner.service
sudo systemctl daemon-reload
```

Or use `just install-system` which handles this automatically.

### Per-User Activation

For each platform user that will use Oqto:

```bash
# As root: Enable lingering so user services start at boot
sudo loginctl enable-linger <username>

# As the user: Enable all services
systemctl --user enable --now hstry eavs mmry oqto-runner
```

### Automated User Provisioning

Use `oqtoctl` to create users with proper setup:

```bash
oqtoctl user create alice --email alice@example.com
```

This automatically:
1. Creates the Linux user if needed
2. Sets up home directory from skeleton
3. Enables systemd lingering
4. Installs and starts all required services

### Single-User Development Mode

For local development, all services run as the current user:

```bash
# Enable all services
systemctl --user enable --now hstry eavs mmry oqto-runner oqto

# Check status
systemctl --user status hstry eavs mmry oqto-runner oqto
```

The `oqto` backend auto-starts `hstry` via its own service manager as a
fallback, but the systemd unit is the preferred approach for reliability
across reboots.

## Socket Communication

The backend communicates with per-user runners via Unix sockets:

- Runner socket: `/run/user/{uid}/oqto-runner.sock`

The backend looks up the user's UID and connects to their runner socket.
Socket permissions (owned by the user, mode 0700 on parent directory)
ensure cross-user isolation.

## Configuration

Main config file: `/etc/oqto/config.toml` (multi-user) or
`~/.config/oqto/config.toml` (single-user).

Service-specific configuration:
- hstry: `~/.config/hstry/config.toml`
- eavs: `~/.config/eavs/config.toml`
- mmry: `~/.config/mmry/config.toml`

## Logs

```bash
# All user services (as the user)
journalctl --user -u hstry -f
journalctl --user -u eavs -f
journalctl --user -u mmry -f
journalctl --user -u oqto-runner -f
journalctl --user -u oqto -f

# Main server logs (system service)
journalctl -u oqto -f

# User runner logs (as root, multi-user)
journalctl --user-unit oqto-runner -M alice@
```

## Security Notes

### Isolation Model

1. **Backend** runs as `oqto` system user with no access to user home directories
2. **Runners** run as individual Linux users with access only to their own files
3. **Socket permissions** prevent users from connecting to other users' runners
4. **Sandbox** (optional) further restricts what processes can access

### Recommended Hardening

1. Ensure `/run/user/{uid}` directories have mode 0700
2. Use separate Linux users for each platform user
3. Enable sandbox restrictions in `/etc/oqto/sandbox.toml`
4. Consider network isolation for strict deployments
