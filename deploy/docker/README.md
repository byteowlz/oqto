# Oqto Docker (All-in-One)

Single container running the complete Oqto platform: backend, frontend, LLM proxy, chat history, agent runtime, and all tools.

## Quick Start

```bash
cd deploy/docker

# Configure
cp .env.example .env
# Edit .env -- add at least one LLM provider API key

# Run (pulls from ghcr.io or builds locally)
docker compose up -d

# Check logs
docker compose logs -f

# Open
open http://localhost:8080
```

On first boot, admin credentials are printed in the logs. Save them.

## Build Locally

```bash
# From repo root
docker build -f deploy/docker/Dockerfile -t oqto:latest .

# Or via compose
cd deploy/docker
docker compose build
```

## Environment Variables

### Required (at least one provider)

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `OPENAI_API_KEY` | OpenAI API key |
| `GEMINI_API_KEY` | Google Gemini API key |
| `OPENROUTER_API_KEY` | OpenRouter API key |

### Authentication

| Variable | Default | Description |
|----------|---------|-------------|
| `JWT_SECRET` | auto-generated | JWT signing key (min 32 chars). Persisted in `/data` volume across restarts. |
| `ADMIN_USER` | `admin` | Bootstrap admin username (first run only) |
| `ADMIN_PASSWORD` | auto-generated | Admin password. Printed in logs if auto-generated. |
| `ADMIN_EMAIL` | `admin@oqto.local` | Admin email |
| `OQTO_SINGLE_USER` | `false` | Set `true` to disable auth entirely (dev/personal use) |

### Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `OQTO_PORT` | `8080` | Port exposed to host |
| `OQTO_LOG_LEVEL` | `info` | Log level: `error`, `warn`, `info`, `debug`, `trace` |
| `OQTO_DATA_DIR` | `/data` | Data directory inside container (map to volume) |

### Additional Provider Keys

| Variable | Provider |
|----------|----------|
| `AZURE_API_KEY` | Azure OpenAI |
| `DEEPSEEK_API_KEY` | DeepSeek |
| `MISTRAL_API_KEY` | Mistral AI |

## Architecture

Everything runs inside one container, managed by `entrypoint.sh`:

```
                  :8080 (exposed)
                    |
                  caddy (reverse proxy)
                  /    \
    frontend     API + WebSocket
   (static)        |
              oqto backend (:8081)
              /          \
        hstry           eavs (:3033)
      (gRPC)          (LLM proxy)
        |                |
    SQLite          upstream LLM APIs
   (chat history)   (Anthropic, OpenAI, ...)
```

### Internal Services

| Service | Port | Protocol | Purpose |
|---------|------|----------|---------|
| caddy | 8080 | HTTP | Reverse proxy, serves frontend static files |
| oqto | 8081 | HTTP+WS | Backend API, WebSocket multiplexer |
| eavs | 3033 | HTTP | LLM proxy with virtual keys |
| hstry | auto | gRPC | Chat history (Unix socket / TCP) |

### Process Lifecycle

1. **hstry** starts first (chat history must be available)
2. **eavs** starts next (LLM proxy for model metadata)
3. **oqto** backend starts (depends on both)
4. **caddy** starts last (reverse proxy + static frontend)

If any process exits, the entrypoint triggers graceful shutdown of all services.

## Data Persistence

All state lives in `/data` (mount as a Docker volume):

```
/data/
  oqto/
    oqto.db          # User accounts, sessions (SQLite)
    .jwt_secret       # Persisted JWT secret
    .bootstrapped     # First-run marker
  hstry/
    hstry.db          # Chat message history (SQLite)
  eavs/
    eavs.env          # Generated eavs environment
    .admin_key        # Eavs admin API key
  users/              # Per-user data
  workspaces/         # Agent workspaces
```

## What's Inside

### Oqto Binaries
`oqto`, `oqtoctl`, `oqto-runner`, `oqto-files`, `oqto-sandbox`, `oqto-scaffold`, `oqto-usermgr`, `pi-bridge`

### Agent Tools
`hstry`, `hstry-tui`, `eavs`, `agntz`, `mmry`, `mmry-service`, `tmpltr`, `sldr`, `ignr`, `trx`, `scrpr`, `sx`

### Runtime
`pi` (AI agent), `ttyd` (web terminal), `caddy` (reverse proxy)

### Shell Tools
`tmux`, `rg` (ripgrep), `fd`, `fzf`, `zsh`, `neovim`, `yazi`, `zoxide`, `starship`, `jq`, `git`, `curl`

### Languages
`python3` + `uv`, `bun` (JS/TS runtime)

### Media/Docs
`typst` (PDF), `imagemagick`, `ffmpeg`, `poppler-utils`

### Fonts
Liberation, Noto, Noto Emoji, DejaVu, Roboto, Inter

## Future: Split Architecture

This monolithic image is designed to be split later when the runner gains TCP/IP support:

- `ghcr.io/byteowlz/oqto:latest` -- platform only (oqto, caddy, frontend)
- `ghcr.io/byteowlz/oqto-session:latest` -- per-session container (pi, tools, shell)

The runner currently uses Unix sockets, so everything must live in one container for now.

## CI/CD

The Docker image is built and pushed automatically by `.github/workflows/docker.yml` when a release is published. It triggers after the release workflow creates GitHub releases with pre-built binaries.

```bash
# Image is published to:
ghcr.io/byteowlz/oqto:latest
ghcr.io/byteowlz/oqto:<version>
```
