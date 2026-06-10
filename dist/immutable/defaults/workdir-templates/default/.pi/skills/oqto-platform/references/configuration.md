# Oqto Configuration Reference

## Table of Contents

1. [Backend Configuration](#backend-configuration)
2. [Sandbox Configuration](#sandbox-configuration)
3. [Frontend Configuration](#frontend-configuration)
4. [Environment Variables](#environment-variables)

---

## Backend Configuration

Location: `$XDG_CONFIG_HOME/oqto/config.toml` (typically `~/.config/oqto/config.toml`)

Schema: `https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/oqto/oqto.backend.config.schema.json`

### Full Example

```toml
"$schema" = "https://raw.githubusercontent.com/byteowlz/schemas/refs/heads/main/oqto/oqto.backend.config.schema.json"

profile = "default"

[logging]
level = "info"                           # error, warn, info, debug, trace
# file = "~/Library/Logs/oqto.log"       # Optional log file
audit_enabled = true                      # JSONL audit logging
# audit_file = "$XDG_STATE_HOME/oqto/audit.log.jsonl"

[server]
max_upload_size_mb = 100                  # Maximum file upload size
admin_socket_path = "/run/oqto/oqtoctl.sock"  # Unix socket for oqtoctl

[runtime]
# parallelism = 8                        # Worker pool size (default: CPU count)
timeout = 60                              # Operation timeout in seconds
fail_fast = true

[paths]
# data_dir = "$XDG_DATA_HOME/oqto"       # Persistent data
# state_dir = "$XDG_STATE_HOME/oqto"     # Machine-specific state

[container]
# runtime = "docker"                     # "docker" or "podman" (auto-detected)
# binary = "/usr/local/bin/docker"       # Custom runtime binary path
default_image = "oqto-dev:latest"         # Container image for sessions
base_port = 41820                         # Starting port for session services
# skel_path = "./container/skel"         # Skeleton dir for new user homes

[local]
enabled = false                           # Enable local mode (no containers)
fileserver_binary = "fileserver"           # Path to oqto-files binary
ttyd_binary = "ttyd"                      # Path to ttyd binary
workspace_dir = "$HOME/oqto/{user_id}"    # Workspace root ({user_id} placeholder)
single_user = false                       # Single-user mode
runner_socket_pattern = "/run/oqto/runner-sockets/{user}/oqto-runner.sock"
cleanup_on_startup = false                # Kill sessions on backend start
stop_sessions_on_shutdown = false          # Kill sessions on backend stop

[local.linux_users]
enabled = false                           # Linux user isolation (requires root)
prefix = "oqto_"                          # Username prefix (e.g., "oqto_alice")
uid_start = 2000                          # Starting UID
group = "oqto"                            # Shared group
shell = "/bin/bash"
use_sudo = true
create_home = true

[runner]
# runner_id = "workstation-1"            # Human-readable runner ID
# pi_sessions_dir = "~/.local/share/pi/sessions"
# memories_dir = "~/.local/share/mmry"

[agent_browser]
enabled = false                           # Per-session agent-browser daemon
binary = "oqto-browserd"                  # Browser daemon binary
headed = false                            # Headed vs headless
stream_port_base = 30000                  # Screencast WebSocket base port
stream_port_range = 10000
# executable_path = "/usr/bin/chromium"   # Custom Chromium path
# extensions = ["/path/to/extension"]

[eavs]
enabled = true                            # EAVS (LLM proxy) integration
base_url = "http://localhost:41800"
container_url = "http://host.containers.internal:41800"
# master_key = "your-master-key"          # EAVS admin key
default_session_budget_usd = 10.0
default_session_rpm = 60

[auth]
dev_mode = true                           # Dev mode (relaxed security)
# jwt_secret = "your-secret-32-chars+"    # Required when dev_mode = false
# jwt_secret = "env:AUTH_JWT_SECRET"      # Read from environment variable
# oidc_issuer = "https://auth.example.com"
# oidc_audience = "your-app-id"
# allowed_origins = ["https://your-domain.com"]

# [[auth.dev_users]]                     # Dev mode users
# id = "dev"
# name = "Developer"
# email = "dev@localhost"
# password_hash = "$2b$12$..."
# role = "admin"

[sessions]
auto_attach = "on"                        # "off", "attach", "resume", "on"
auto_attach_scan = true
max_concurrent_sessions = 6
idle_timeout_minutes = 30
idle_check_interval_seconds = 300

[templates]
type = "remote"                           # "remote" (git) or "local"
# repo_path = "/path/to/oqto-templates"
sync_on_list = true
sync_interval_seconds = 120

[onboarding_templates]
sync_enabled = true
sync_interval_seconds = 300
use_embedded_fallback = true
branch = "main"
subdirectory = "agents"

[feedback]
keep_public = true
sync_interval_seconds = 60

[scaffold]
binary = "byt"
subcommand = "new"
template_arg = "--template"
output_arg = "--output"
github_arg = "--github"
private_arg = "--private"
description_arg = "--description"
```

### Section Details

#### [logging]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| level | string | "info" | Log level: error, warn, info, debug, trace |
| file | string | (none) | Optional log file path (supports ~ and env vars) |
| audit_enabled | bool | true | Enable JSONL audit logging |
| audit_file | string | (auto) | Audit log path (default: `$XDG_STATE_HOME/oqto/audit.log.jsonl`) |

#### [server]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| max_upload_size_mb | int | 100 | Maximum file upload size in MB |
| admin_socket_path | string | `/run/oqto/oqtoctl.sock` | Unix socket for oqtoctl |

#### [runtime]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| parallelism | int | (CPU count) | Worker pool size |
| timeout | int | 60 | Operation timeout in seconds |
| fail_fast | bool | true | Fail fast on errors |

#### [paths]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| data_dir | string | `$XDG_DATA_HOME/oqto` | Persistent data directory |
| state_dir | string | `$XDG_STATE_HOME/oqto` | Machine-specific state directory |

#### [container]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| runtime | string | (auto) | "docker" or "podman" |
| binary | string | (auto) | Custom container runtime binary |
| default_image | string | "oqto-dev:latest" | Container image for sessions |
| base_port | int | 41820 | Starting port for session services |
| skel_path | string | (none) | Skeleton directory for new user homes |

#### [local]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| enabled | bool | false | Enable local mode |
| fileserver_binary | string | "fileserver" | Path to oqto-files |
| ttyd_binary | string | "ttyd" | Path to ttyd |
| workspace_dir | string | `$HOME/oqto/{user_id}` | Workspace root (supports `{user_id}`) |
| single_user | bool | false | Single-user mode |
| runner_socket_pattern | string | (none) | Runner socket path (supports `{user}`, `{uid}`) |
| cleanup_on_startup | bool | false | Kill sessions on backend start |
| stop_sessions_on_shutdown | bool | false | Kill sessions on backend stop |

#### [local.linux_users]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| enabled | bool | false | Enable Linux user isolation |
| prefix | string | "oqto_" | Username prefix |
| uid_start | int | 2000 | Starting UID |
| group | string | "oqto" | Shared group |
| shell | string | "/bin/bash" | User shell |
| use_sudo | bool | true | Use sudo for user creation |
| create_home | bool | true | Create home directories |

#### [runner]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| runner_id | string | (hostname) | Human-readable runner ID |
| pi_sessions_dir | string | `~/.local/share/pi/sessions` | Pi session files directory |
| memories_dir | string | `~/.local/share/mmry` | Memories database directory |

#### [agent_browser]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| enabled | bool | false | Enable agent-browser integration |
| binary | string | "oqto-browserd" | Browser daemon binary |
| headed | bool | false | Headed browser windows |
| stream_port_base | int | 30000 | Screencast WebSocket base port |
| stream_port_range | int | 10000 | Port range for per-session streams |
| executable_path | string | (auto) | Custom Chromium path |
| extensions | string[] | [] | Browser extensions to load |

#### [eavs]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| enabled | bool | true | Enable EAVS integration |
| base_url | string | `http://localhost:41800` | EAVS server URL |
| container_url | string | (none) | EAVS URL for containers |
| master_key | string | (none) | EAVS admin master key |
| default_session_budget_usd | float | 10.0 | Default session budget |
| default_session_rpm | int | 60 | Default rate limit |

#### [auth]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| dev_mode | bool | true | Enable dev mode |
| jwt_secret | string | (required in prod) | JWT signing secret (32+ chars) |
| oidc_issuer | string | (none) | OIDC provider URL |
| oidc_audience | string | (none) | OIDC audience/app ID |
| allowed_origins | string[] | (auto in dev) | CORS allowed origins |

#### [sessions]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| auto_attach | string | "on" | Auto-attach behavior: off, attach, resume, on |
| auto_attach_scan | bool | true | Scan running sessions before attaching |
| max_concurrent_sessions | int | 6 | Max running sessions per user |
| idle_timeout_minutes | int | 30 | Idle timeout before stopping |
| idle_check_interval_seconds | int | 300 | Idle check interval |

#### [templates]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| type | string | "remote" | Template source: "remote" or "local" |
| repo_path | string | (none) | Local path to templates repo |
| sync_on_list | bool | true | Sync before listing |
| sync_interval_seconds | int | 120 | Min seconds between syncs |

#### [scaffold]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| binary | string | "byt" | Scaffolding tool binary |
| subcommand | string | "new" | Subcommand to invoke |
| template_arg | string | "--template" | Template name argument |
| output_arg | string | "--output" | Output directory argument |
| github_arg | string | "--github" | GitHub repo creation argument |
| private_arg | string | "--private" | Private repo argument |
| description_arg | string | "--description" | Description argument |

---

## Sandbox Configuration

Location: `~/.config/oqto/sandbox.toml` (separate from main config for security)

Agents can modify `config.toml` but cannot weaken sandbox restrictions.

```toml
enabled = true
profile = "development"  # "minimal", "development", "strict"
deny_read = ["~/.ssh", "~/.aws", "~/.gnupg"]
allow_write = ["~/.cargo", "~/.npm", "/tmp"]
isolate_network = false  # true in strict profile
isolate_pid = true
```

Per-workspace overrides in `.oqto/sandbox.toml` can only ADD restrictions, never remove them.

---

## Frontend Configuration

Location: `.env.local` in frontend directory

```bash
# Backend API URL
VITE_CONTROL_PLANE_URL=http://localhost:8080

# Debug flags
VITE_DEBUG_WS=1       # WebSocket debug logging
VITE_DEBUG_PI=1        # Pi debug logging
```

### Production Build

```bash
cd frontend
bun install
bun run build
```

Output in `frontend/dist/`.

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| OQTO_CONFIG | Path to config file |
| OQTO_SERVER_URL | Server URL for oqtoctl |
| OQTO_ADMIN_SOCKET | Admin socket path for oqtoctl |
| OQTO_SESSION_ID | Current session ID (set in agent env) |
| OQTO_RUNNER_ID | Runner identifier |
| OQTO_DATABASE_PATH | Database file path |
| EAVS_API_KEY | EAVS virtual key (injected per-session) |
| EAVS_MASTER_KEY | EAVS admin master key |
| AUTH_JWT_SECRET | JWT secret (via `env:` prefix in config) |
| RUST_LOG | Log level override (info, debug, trace) |
