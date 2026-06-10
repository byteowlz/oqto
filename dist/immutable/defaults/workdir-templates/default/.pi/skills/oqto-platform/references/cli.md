# Oqto CLI Reference

## Table of Contents

1. [oqto - Server Binary](#oqto---server-binary)
2. [oqtoctl - Control CLI](#oqtoctl---control-cli)
3. [oqto-runner - Agent Process Daemon](#oqto-runner---agent-process-daemon)
4. [oqto-files - File Server](#oqto-files---file-server)
5. [oqto-sandbox - Sandbox Wrapper](#oqto-sandbox---sandbox-wrapper)
6. [pi-bridge - Container Bridge](#pi-bridge---container-bridge)
7. [agntz - Agent Operations](#agntz---agent-operations)
8. [byt - Cross-Repo Management](#byt---cross-repo-management)
9. [trx - Issue Tracking](#trx---issue-tracking)

---

## oqto - Server Binary

Main backend server binary.

### serve
Start the API server.

```bash
oqto serve
oqto serve --port 8080
oqto serve --config /path/to/config.toml
```

### config show
Display current configuration.

```bash
oqto config show
```

### invite-codes generate
Generate invite codes for user registration.

```bash
oqto invite-codes generate
oqto invite-codes generate --count 5
oqto invite-codes generate --max-uses 10
```

### init
Create configuration directories.

```bash
oqto init
```

### completions
Generate shell completions.

```bash
oqto completions bash > /etc/bash_completion.d/oqto
oqto completions zsh > ~/.zsh/completions/_oqto
oqto completions fish > ~/.config/fish/completions/oqto.fish
```

---

## oqtoctl - Control CLI

Control CLI for managing the Oqto server. Communicates via HTTP API or Unix admin socket.

**Global flags:**
- `--server URL` / `-s` -- Oqto server URL (default: `http://localhost:8080/api`, env: `OQTO_SERVER_URL`)
- `--json` -- Machine-readable JSON output
- `--config PATH` / `-c` -- Config file path (env: `OQTO_CONFIG`)
- `--admin-socket PATH` -- Admin socket for local root access (env: `OQTO_ADMIN_SOCKET`)

### status
Check server health.

```bash
oqtoctl status
```

### ask
Ask an agent a question and get the response.

```bash
oqtoctl ask "@@main" "What files did you change?"
oqtoctl ask "@@pi:my-session" "Explain this code" --stream
oqtoctl ask "session:abc123" "Status?" --timeout 60
```

Target formats:
- `@@main`, `@@pi` -- Main chat (most recent session)
- `@@main:query` -- Main chat, search for session
- `@@<name>` -- Main chat by assistant name
- `@@session:id` -- Specific session by ID

### sessions
List or search main chat sessions.

```bash
oqtoctl sessions
oqtoctl sessions "my-project" --limit 10
```

### session
Manage sessions.

```bash
oqtoctl session list
oqtoctl session get <id>
oqtoctl session stop <id>
oqtoctl session resume <id>
oqtoctl session delete <id> [--force]
oqtoctl session upgrade <id>
```

### container
Manage containers.

```bash
oqtoctl container list
oqtoctl container refresh [--outdated-only]
oqtoctl container cleanup
oqtoctl container stop-all
```

### image
Manage container images.

```bash
oqtoctl image check
oqtoctl image pull [image-name]
oqtoctl image build [path] [--no-cache]
```

### local
Manage local mode processes.

```bash
oqtoctl local cleanup
```

### sandbox
Manage sandbox configuration.

```bash
oqtoctl sandbox show
oqtoctl sandbox edit
oqtoctl sandbox validate
oqtoctl sandbox reset [--yes]
```

### user
Manage users and runner provisioning.

```bash
oqtoctl user list [--runner-status]
oqtoctl user show <user>
oqtoctl user create <username> --email <email> [--role admin] [--password <pw>]
oqtoctl user setup-runner <user> [--force]
oqtoctl user runner-status <user>
oqtoctl user sync-configs [--user <id>]
oqtoctl user bootstrap --username <name> --email <email> [--password <pw>]
```

### a2ui
Send A2UI surfaces to users (for agents).

```bash
oqtoctl a2ui button -s <session> -b "Yes,No" -p "Confirm?"
oqtoctl a2ui input -s <session> "Enter your name"
oqtoctl a2ui choice -s <session> -c "opt1,opt2,opt3" [--multi]
oqtoctl a2ui checkbox -s <session> "Enable feature"
oqtoctl a2ui slider -s <session> --min 0 --max 100
oqtoctl a2ui datetime -s <session> --date --time
oqtoctl a2ui text -s <session> "Hello world" [--style h1]
oqtoctl a2ui image -s <session> "https://..." [--confirm]
oqtoctl a2ui video -s <session> "https://..." [--confirm]
oqtoctl a2ui audio -s <session> "https://..." [--description "..."]
oqtoctl a2ui tabs -s <session> '[{"title":"Tab1","content":"text"}]'
oqtoctl a2ui raw -s <session> '{"messages":[...]}' [--blocking]
```

### ui
Agent-driven UI control commands.

```bash
oqtoctl ui navigate "/path" [--replace]
oqtoctl ui session <session_id> [--mode main|pi]
oqtoctl ui view <view>   # chat, files, terminal, tasks, memories, settings, canvas, voice
oqtoctl ui palette [--open true|false]
oqtoctl ui palette-exec <command> [--args '{"key":"val"}']
oqtoctl ui spotlight <target> [--title "..." --description "..." --action "..."]
oqtoctl ui tour --steps '[...]' [--start-index 0] [--stop]
oqtoctl ui sidebar [--collapsed true|false]
oqtoctl ui panel [--view preview|canvas|terminal|memories] [--collapsed true|false]
oqtoctl ui theme <light|dark|system>
```

### hash-password
Hash a password using bcrypt.

```bash
oqtoctl hash-password --password "mypassword"
echo "mypassword" | oqtoctl hash-password
```

---

## oqto-runner - Agent Process Daemon

Per-user daemon that owns agent processes and translates native events to canonical format. Managed by the backend.

Runs as a systemd user service or spawned by the backend. Communicates with the backend over Unix or TCP sockets using the runner protocol defined in `oqto-protocol`.

---

## oqto-files - File Server

Workspace file access server. Provides REST API for file operations (read, write, list, search).

---

## oqto-sandbox - Sandbox Wrapper

Wraps agent processes with bwrap (Linux) or sandbox-exec (macOS) for process isolation.

---

## pi-bridge - Container Bridge

HTTP/WebSocket bridge for Pi running inside containers. Translates between container networking and the host.

---

## agntz - Agent Operations

CLI for day-to-day agent operations within sessions.

### memory
```bash
agntz memory search "query" [--limit 5]
agntz memory add "insight text" [-c category] [-i importance]
agntz memory list [--category api]
```

Categories: api, frontend, backend, architecture, patterns, debugging
Importance: 1-10 (7+ for significant insights)

### issues
```bash
agntz ready                    # Show unblocked issues
agntz issues [--status open]   # List all issues
```

### mail
```bash
agntz mail inbox               # Check messages
agntz mail send "recipient" "subject" "body"
```

### file reservations
```bash
agntz reserve src/file.rs      # Reserve file for editing
agntz release src/file.rs      # Release reservation
```

---

## byt - Cross-Repo Management

CLI for cross-repository governance and management.

```bash
byt catalog list               # List all repos
byt status                     # Show repo status
byt memory search "query" --all
byt sync push                  # Sync memories to git
byt sync pull                  # Pull memories from git
```

---

## trx - Issue Tracking

CLI for issue tracking within projects.

```bash
trx ready                              # Show unblocked issues
trx create "Title" -t bug -p 1         # Create issue
trx update <id> --status in_progress   # Update status
trx close <id> -r "Done"               # Close issue
trx list [--status open] [--type bug]  # List issues
trx sync                               # Commit .trx/ changes
```

Types: bug, feature, task, epic, chore
Priorities: 0=critical, 1=high, 2=medium, 3=low, 4=backlog

---

## Build Commands (justfile)

```bash
just                   # List all commands
just build             # Build all components
just build-backend     # Build backend only
just build-frontend    # Build frontend only
just dev               # Start frontend dev server (Vite on :3000)
just lint              # Run all linters
just fmt               # Format all Rust code
just check             # Check all Rust code compiles
just test              # Run all tests
just gen-types         # Generate TypeScript types from Rust structs
just reload-fast       # Rebuild backend, install, restart services
just install-all       # Install all dependencies and binaries
just install <crate>   # Install a specific crate
```

Admin recipes:
```bash
just admin help
just admin-status
just admin-eavs --all
just admin-sync-pi --all
just admin-skills --list
just admin-sync-all
```
