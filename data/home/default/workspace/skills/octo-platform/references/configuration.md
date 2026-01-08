# Octo Configuration Reference

## Table of Contents

1. [Backend Configuration](#backend-configuration)
2. [Frontend Configuration](#frontend-configuration)
3. [Container Settings](#container-settings)
4. [Local Mode Settings](#local-mode-settings)

---

## Backend Configuration

Location: `~/.config/octo/config.toml`

### Full Example

```toml
[server]
port = 8080
host = "0.0.0.0"

[backend]
mode = "container"  # "container", "local", or "auto"

[container]
runtime = "docker"  # "docker" or "podman"
default_image = "octo-dev:latest"
base_port = 41820
network = "octo-network"

[local]
opencode_binary = "opencode"
fileserver_binary = "fileserver"
ttyd_binary = "ttyd"
workspace_dir = "$HOME/octo/{user_id}"
single_user = false

[auth]
jwt_secret = "your-secret-here"
dev_mode = false
session_duration = "7d"

[voice]
enabled = true
stt_url = "ws://localhost:8765"
tts_url = "ws://localhost:8766"

[mmry]
enabled = true
base_url = "http://localhost:41823"

[database]
path = "~/.local/share/octo/octo.db"
```

### Section Details

#### [server]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| port | int | 8080 | HTTP server port |
| host | string | "0.0.0.0" | Bind address |

#### [backend]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| mode | string | "auto" | Runtime mode: "container", "local", or "auto" |

Auto mode: Docker on macOS, Podman on Linux.

#### [auth]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| jwt_secret | string | required | Secret for JWT signing |
| dev_mode | bool | false | Enable dev login (insecure) |
| session_duration | string | "7d" | JWT token lifetime |

#### [voice]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| enabled | bool | true | Enable voice features |
| stt_url | string | ws://localhost:8765 | eaRS WebSocket URL |
| tts_url | string | ws://localhost:8766 | kokorox WebSocket URL |

#### [mmry]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| enabled | bool | true | Enable memory system |
| base_url | string | http://localhost:41823 | mmry service URL |

---

## Container Settings

#### [container]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| runtime | string | auto-detected | "docker" or "podman" |
| default_image | string | "octo-dev:latest" | Container image for sessions |
| base_port | int | 41820 | Starting port for session services |
| network | string | "octo-network" | Docker/Podman network name |

### Port Allocation

Each session uses 4 ports starting from base_port + (session_index * 4):
- Port 0: OpenCode (41820)
- Port 1: Fileserver (41821)
- Port 2: ttyd (41822)
- Port 3: mmry (41823)

### Building the Container Image

```bash
docker build -t octo-dev:latest -f container/Dockerfile .
```

The container includes:
- OpenCode runtime
- Fileserver
- ttyd (web terminal)
- mmry (memory service)
- Development tools (Git, Node.js, Python, Rust, Go, Bun)

---

## Local Mode Settings

#### [local]
| Key | Type | Default | Description |
|-----|------|---------|-------------|
| opencode_binary | string | "opencode" | Path to opencode binary |
| fileserver_binary | string | "fileserver" | Path to fileserver binary |
| ttyd_binary | string | "ttyd" | Path to ttyd binary |
| workspace_dir | string | "$HOME/octo/{user_id}" | Workspace root directory |
| single_user | bool | false | Skip user isolation |

### Workspace Directory Variables

The `workspace_dir` supports these variables:
- `{user_id}`: Current user's ID
- `$HOME`: User's home directory

---

## Frontend Configuration

Location: `.env.local` in frontend directory

```bash
# Backend API URL
VITE_CONTROL_PLANE_URL=http://localhost:8080

# Optional: Custom WebSocket URL (defaults to control plane)
VITE_WS_URL=ws://localhost:8080/api/ws
```

### Production Build

```bash
cd frontend
bun install
bun run build
```

Output in `frontend/dist/` can be served by any static file server.

---

## Environment Variables

The backend also supports configuration via environment variables:

| Variable | Description |
|----------|-------------|
| OCTO_PORT | Override server port |
| OCTO_HOST | Override bind address |
| OCTO_JWT_SECRET | Override JWT secret |
| OCTO_DEV_MODE | Enable dev mode (true/false) |
| OCTO_CONFIG | Path to config file |
| RUST_LOG | Log level (info, debug, trace) |

Environment variables take precedence over config file values.
