# Octo CLI Reference

## Table of Contents

1. [octo - Server CLI](#octo---server-cli)
2. [octoctl - Control CLI](#octoctl---control-cli)
3. [agntz - Agent Operations](#agntz---agent-operations)
4. [byt - Cross-Repo Management](#byt---cross-repo-management)
5. [trx - Issue Tracking](#trx---issue-tracking)

---

## octo - Server CLI

Main server binary for running the Octo backend.

### serve
Start the API server.

```bash
octo serve
octo serve --port 8080
octo serve --config /path/to/config.toml
```

### config show
Display current configuration.

```bash
octo config show
```

### invite-codes generate
Generate invite codes for user registration.

```bash
octo invite-codes generate
octo invite-codes generate --count 5
octo invite-codes generate --max-uses 10
```

### init
Create configuration directories.

```bash
octo init
```

### completions
Generate shell completions.

```bash
octo completions bash > /etc/bash_completion.d/octo
octo completions zsh > ~/.zsh/completions/_octo
octo completions fish > ~/.config/fish/completions/octo.fish
```

---

## octoctl - Control CLI

Control CLI for managing containers, sessions, and images.

### status
Check server health.

```bash
octoctl status
```

### session list
List all sessions.

```bash
octoctl session list
octoctl session list --format json
```

### session stop
Stop a running session.

```bash
octoctl session stop <session_id>
```

### session resume
Resume a stopped session.

```bash
octoctl session resume <session_id>
```

### session delete
Delete a session.

```bash
octoctl session delete <session_id>
```

### container refresh
Rebuild all containers.

```bash
octoctl container refresh
```

### image build
Build the container image.

```bash
octoctl image build
octoctl image build --tag octo-dev:latest
```

---

## agntz - Agent Operations

CLI for day-to-day agent operations within sessions.

### memory search
Search memories.

```bash
agntz memory search "query"
agntz memory search "voice mode" --limit 5
```

### memory add
Add a new memory.

```bash
agntz memory add "insight text"
agntz memory add "insight text" -c api -i 7
```

**Flags:**
- `-c, --category`: Category (api, frontend, backend, architecture, patterns, debugging)
- `-i, --importance`: Importance level 1-10 (7+ for significant insights)

### memory list
List all memories.

```bash
agntz memory list
agntz memory list --category api
```

### ready
Show unblocked issues ready for work.

```bash
agntz ready
```

### issues
List all issues.

```bash
agntz issues
agntz issues --status open
```

### mail inbox
Check messages.

```bash
agntz mail inbox
```

### mail send
Send a message.

```bash
agntz mail send "recipient" "subject" "body"
```

### reserve
Reserve a file for editing (prevents conflicts).

```bash
agntz reserve src/file.rs
```

### release
Release a file reservation.

```bash
agntz release src/file.rs
```

---

## byt - Cross-Repo Management

CLI for cross-repository governance and management.

### catalog list
List all repositories in the catalog.

```bash
byt catalog list
```

### status
Show current repository status.

```bash
byt status
```

### memory search
Search memories across all stores.

```bash
byt memory search "query" --all
byt memory search "pattern" --repo specific-repo
```

### sync push
Sync memories to git.

```bash
byt sync push
```

### sync pull
Pull memories from git.

```bash
byt sync pull
```

---

## trx - Issue Tracking

CLI for issue tracking within projects.

### ready
Show unblocked issues ready for work.

```bash
trx ready
```

### create
Create a new issue.

```bash
trx create "Issue title"
trx create "Bug: Something broken" -t bug -p 1
trx create "Add feature" -t feature -p 2 --description "Details here"
```

**Flags:**
- `-t, --type`: Issue type (bug, feature, task, epic, chore)
- `-p, --priority`: Priority 0-4 (0=critical, 1=high, 2=medium, 3=low, 4=backlog)
- `--description`: Issue description

### update
Update an issue.

```bash
trx update <id> --status in_progress
trx update <id> --priority 1
trx update <id> --assignee username
```

### close
Close an issue.

```bash
trx close <id>
trx close <id> -r "Completed implementation"
```

**Flags:**
- `-r, --reason`: Closing reason/comment

### list
List issues.

```bash
trx list
trx list --status open
trx list --type bug
```

### sync
Commit .trx/ changes to git.

```bash
trx sync
```
