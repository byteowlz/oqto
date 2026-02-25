# Oqto Deployment

Deploy Oqto to all configured hosts with a single command.

## Quick Start

```bash
just deploy              # Build + deploy to all hosts
just deploy-dry-run      # See what would happen
just deploy-host archvm  # Deploy to one host only
just deploy-quick        # Skip build, deploy existing artifacts
```

## Configuration

Hosts are defined in `deploy/hosts.toml`. Each `[[host]]` block describes a
deployment target:

```toml
[[host]]
name = "archvm"          # Label for filtering and logs
ssh = "localhost"        # SSH alias or user@host
local = true             # true = local machine, skip SSH
mode = "local"           # "local" or "multi-user"
user = "wismut"          # OS user that owns the deployment
frontend = true          # Deploy frontend assets
web_root = "/var/www/oqto"  # Where frontend dist is served
binaries = ["oqto", "oqto-runner", "oqto-files"]  # Binaries to install
services = ["oqto", "oqto-runner"]  # Systemd user services to restart
```

### Host Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Human-readable label, used with `--host` filter |
| `ssh` | yes | SSH alias from `~/.ssh/config` or `user@hostname` |
| `local` | no | Set `true` for the local machine (default: `false`) |
| `mode` | yes | `"local"` (single-user) or `"multi-user"` |
| `user` | yes | OS user that owns deployed files |
| `frontend` | no | Whether to deploy frontend (default: `false`) |
| `web_root` | if frontend | Absolute path for frontend files |
| `binaries` | no | List of backend binaries to install to `/usr/local/bin` |
| `services` | no | Systemd user services to restart after deploy |

### Adding a New Host

1. Ensure SSH access works: `ssh <alias> "echo ok"`
2. Add a `[[host]]` block to `deploy/hosts.toml`
3. Test with `just deploy-dry-run --host <name>`
4. Deploy with `just deploy --host <name>`

## What It Does

### Build Phase

1. Builds backend binaries via `remote-build` (offloaded to build server)
2. Builds frontend via `bun run build`

### Per-Host Deploy Phase

For each host (in order):

1. **Connectivity check** - SSH test (skipped for local)
2. **Backend binaries** - Copied to `/usr/local/bin/` via `sudo install`
3. **Frontend assets** - Synced to `web_root` (old assets removed first)
4. **Service restart** - `systemctl --user restart` for listed services
5. **Multi-user extras** - Kills per-user runners so they respawn with new binary

### Deploy Modes

**Local mode** (`mode = "local"`):
- Single-user setup (dev machine)
- Binaries installed via `sudo install` directly
- Frontend copied locally
- Services restarted via `systemctl --user`

**Multi-user mode** (`mode = "multi-user"`):
- Production multi-user setup
- Binaries uploaded via scp, installed via `sudo install`
- Frontend synced via rsync
- Admin's oqto service restarted
- All per-user oqto-runner processes killed (they auto-respawn)

## Options

```
--host NAME       Deploy only to this host (repeatable)
--skip-build      Don't build, deploy existing artifacts
--skip-frontend   Skip frontend deployment
--skip-backend    Skip binary deployment
--skip-services   Skip service restarts
--dry-run         Show what would happen
--config FILE     Alternate hosts.toml path
```

## Just Recipes

| Recipe | Description |
|--------|-------------|
| `just deploy` | Full build + deploy to all hosts |
| `just deploy-host <name>` | Deploy to one host only |
| `just deploy-quick` | Deploy without rebuilding |
| `just deploy-backend` | Backend binaries only |
| `just deploy-frontend` | Frontend only |
| `just deploy-dry-run` | Preview without executing |

All recipes pass extra arguments through, e.g.:
```bash
just deploy --skip-services
just deploy-host octo-azure --skip-frontend
```

## Prerequisites

- SSH keys configured for all remote hosts
- `sudo` access on remote hosts (for `/usr/local/bin` installs)
- `remote-build` available for compilation (build server)
- `bun` installed for frontend builds
- `rsync` available for frontend deployment to remote hosts
