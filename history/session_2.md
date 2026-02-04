# octo - 2026-02-02 - # AGENTS.md instructions for /home/wismut/byteowlz/octo <INS...

- Created: 2026-02-02T09:38:49.000Z
- Updated: 2026-02-02T12:15:30.000Z
- Workspace: /home/wismut/byteowlz/octo

## assistant
_at 2026-02-02T09:38:49.000Z_

<permissions instructions>
Filesystem sandboxing defines which files can be read or written. `sandbox_mode` is `workspace-write`: The sandbox permits reading files, and editing files in `cwd` and `writable_roots`. Editing files in other directories requires approval. Network access is restricted.
# Escalation Requests

Commands are run outside the sandbox if they are approved by the user, or match an existing rule that allows it to run unrestricted. The command string is split into independent command segments at shell control operators, including but not limited to:

- Pipes: |
- Logical operators: &&, ||
- Command separators: ;
- Subshell boundaries: (...), $(...)

Each resulting segment is evaluated independently for sandbox restrictions and approval requirements.

Example:

git pull | tee output.txt

This is treated as two command segments:

["git", "pull"]

["tee", "output.txt"]

## How to request escalation

IMPORTANT: To request approval to execute a command that will require escalated privileges:

- Provide the `sandbox_permissions` parameter with the value `"require_escalated"`
- Include a short question asking the user if they want to allow the action in `justification` parameter. e.g. "Do you want to download and install dependencies for this project?"
- Suggest a `prefix_rule` - this will be shown to the user with an option to persist the rule approval for future sessions.

If you run a command that is important to solving the user's query, but it fails because of sandboxing, rerun the command with "require_escalated". ALWAYS proceed to use the `justification` and `prefix_rule` parameters - do not message the user before requesting approval for the command.

## When to request escalation

While commands are running inside the sandbox, here are some scenarios that will require escalation outside the sandbox:

- You need to run a command that writes to a directory that requires it (e.g. running tests that write to /var)
- You need to run a GUI app (e.g., open/xdg-open/osascript) to open browsers or files.
- If you run a command that is important to solving the user's query, but it fails because of sandboxing, rerun the command with `require_escalated`. ALWAYS proceed to use the `sandbox_permissions` and `justification` parameters. do not message the user before requesting approval for the command.
- You are about to take a potentially destructive action such as an `rm` or `git reset` that the user did not explicitly ask for.

Only run commands that require approval if it is absolutely necessary to solve the user's query, don't try and circumvent approvals by using other tools.

## prefix_rule guidance

When choosing a `prefix_rule`, request one that will allow you to fulfill similar requests from the user in the future without re-requesting escalation. It should be categorical and reasonably scoped to similar capabilities. You MUST NOT pass the entire command into `prefix_rule`.

<good_example reason="frequently run command">
["npm", "run", "dev"]
</good_example>
<good_example reason="generic and reusable">
["gh", "pr", "checks"]
</good_example>
<good_example reason="helpful for development cycle">
["pytest"]
</good_example>
<bad_example reason="too specific">
["cargo", "test", "-p", "codex-app-server"]
<correction_to_good_example>
["cargo", "test"]
</correction_to_good_example>
</bad_example>

Approved command prefixes:
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nfrom pathlib import Path\npath = Path.home() / \".config\" / \"octo\" / \"config.toml\"\ntext = path.read_text()\nlines = text.splitlines()\nupdated = False\nout = []\nfor line in lines:\n    if line.strip().startswith(\"auto_attach_scan\") and \"=\" in line:\n        out.append(\"auto_attach_scan = true\")\n        updated = True\n    else:\n        out.append(line)\nif not updated:\n    # insert under [sessions]\n    inserted = False\n    for i, line in enumerate(out):\n        if line.strip() == \"[sessions]\":\n            out.insert(i + 1, \"auto_attach_scan = true\")\n            inserted = True\n            break\n    if not inserted:\n        out.append(\"\")\n        out.append(\"[sessions]\")\n        out.append(\"auto_attach_scan = true\")\nnew_text = \"\\n\".join(out) + \"\\n\"\npath.write_text(new_text)\nprint(f\"Updated {path}\")\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport requests\ntext = requests.get('https://raw.githubusercontent.com/kyutai-labs/moshi/main/rust/moshi-server/src/batched_asr.rs').text\nfor i,line in enumerate(text.splitlines(),1):\n    if 'pre_process' in line or 'Channels' in line or 'allocate' in line or 'Channel' in line and 'struct' in line:\n        if 'pre_process' in line or 'Channels' in line:\n            print(i, line)\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport requests\ntext = requests.get('https://raw.githubusercontent.com/kyutai-labs/moshi/main/rust/moshi-server/src/batched_asr.rs').text\nlines = text.splitlines()\nstart=260; end=360\nprint('\\n'.join(f\"{i+1:04d}: {lines[i]}\" for i in range(start-1,end)))\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport requests\ntext = requests.get('https://raw.githubusercontent.com/kyutai-labs/moshi/main/rust/moshi-server/src/batched_asr.rs').text\nlines = text.splitlines()\nstart=360; end=520\nprint('\\n'.join(f\"{i+1:04d}: {lines[i]}\" for i in range(start-1,end)))\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport requests\nurl = 'https://api.github.com/repos/kyutai-labs/moshi/contents/rust'\nresp = requests.get(url, headers={'Accept':'application/vnd.github+json'})\nprint(resp.status_code)\nfor item in resp.json():\n    print(item['name'], item['type'])\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport requests\nurl = 'https://api.github.com/repos/kyutai-labs/moshi/contents/rust/moshi-server/src'\nresp = requests.get(url, headers={'Accept':'application/vnd.github+json'})\nprint(resp.status_code)\nfor item in resp.json():\n    print(item['name'], item['type'])\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport requests, os, textwrap\nurl = 'https://api.github.com/search/repositories?q=kyutai+parallel+streaming'\nprint(requests.get(url, headers={'Accept':'application/vnd.github+json'}).status_code)\nprint(requests.get(url, headers={'Accept':'application/vnd.github+json'}).json().get('items', [])[:3])\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sqlite3\nfrom pathlib import Path\npath = Path.home() / \".local\" / \"share\" / \"octo\" / \"octo.db\"\nprint(path)\nconn = sqlite3.connect(path)\ncur = conn.cursor()\ncur.execute(\"SELECT name FROM sqlite_master WHERE type='table' ORDER BY name\")\nprint(cur.fetchall())\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sqlite3\nfrom pathlib import Path\npath = Path.home() / \".local\" / \"share\" / \"octo\" / \"sessions.db\"\nconn = sqlite3.connect(path)\ncur = conn.cursor()\ncur.execute(\n    \"SELECT readable_id, id, status, started_at, stopped_at, last_activity_at FROM sessions WHERE workspace_path = ? ORDER BY created_at DESC LIMIT 5\",\n    (\"/home/wismut/byteowlz/octo\",),\n)\nfor row in cur.fetchall():\n    print(row)\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sqlite3\nfrom pathlib import Path\npath = Path.home() / \".local\" / \"share\" / \"octo\" / \"sessions.db\"\nconn = sqlite3.connect(path)\ncur = conn.cursor()\ncur.execute(\"PRAGMA table_info(agents)\")\nprint(cur.fetchall())\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sqlite3\nfrom pathlib import Path\npath = Path.home() / \".local\" / \"share\" / \"octo\" / \"sessions.db\"\nconn = sqlite3.connect(path)\ncur = conn.cursor()\ncur.execute(\"PRAGMA table_info(sessions)\")\nprint(cur.fetchall())\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sqlite3\nfrom pathlib import Path\npath = Path.home() / \".local\" / \"share\" / \"octo\" / \"sessions.db\"\nconn = sqlite3.connect(path)\ncur = conn.cursor()\ncur.execute(\"SELECT id, readable_id, status, workspace_path, last_activity_at FROM sessions WHERE readable_id LIKE ?\", (\"%jinx%\",))\nprint(cur.fetchall())\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sqlite3\nfrom pathlib import Path\npath = Path.home() / \".local\" / \"share\" / \"octo\" / \"sessions.db\"\nconn = sqlite3.connect(path)\ncur = conn.cursor()\ncur.execute(\"SELECT id, readable_id, status, workspace_path, opencode_port, fileserver_port, ttyd_port, last_activity_at FROM sessions WHERE readable_id = ?\", (\"dark-jinx\",))\nprint(cur.fetchall())\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sqlite3\nfrom pathlib import Path\npath = Path.home() / \".local\" / \"share\" / \"octo\" / \"sessions.db\"\nconn = sqlite3.connect(path)\ncur = conn.cursor()\ncur.execute(\"SELECT readable_id, id, status, workspace_path FROM sessions ORDER BY created_at DESC LIMIT 20\")\nfor row in cur.fetchall():\n    print(row)\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sqlite3\nfrom pathlib import Path\npath = Path.home() / \".local\" / \"share\" / \"octo\" / \"sessions.db\"\nconn = sqlite3.connect(path)\ncur = conn.cursor()\ncur.execute(\"SELECT readable_id, id, status, workspace_path FROM sessions WHERE readable_id LIKE ?\", (\"%dark%\",))\nprint(cur.fetchall())\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sqlite3\nfrom pathlib import Path\npath = Path.home() / \".local\" / \"share\" / \"octo\" / \"sessions.db\"\nprint(path)\nconn = sqlite3.connect(path)\ncur = conn.cursor()\ncur.execute(\"SELECT name FROM sqlite_master WHERE type='table' ORDER BY name\")\nprint(cur.fetchall())\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport sys,urllib.request,ssl\nurl='https://raw.githubusercontent.com/alexzhang13/rlm/main/README.md'\nprint(urllib.request.urlopen(url, context=ssl.create_default_context()).read().decode('utf-8'))\nPY"]
- ["/usr/bin/zsh", "-lc", "python - <<'PY'\nimport textwrap,sys,subprocess,os, json\nPY"]
- ["/usr/bin/zsh", "-lc", "rg -n \"The Gov'nr\" /home/wismut/.local/share/opencode/storage/session -g\"*.json\""]
- ["/usr/bin/zsh", "-lc", "rg -n \"dark-jinx\" ~/.local/share/opencode/storage/session -g\"*.json\""]
- ["agntz", "mail", "inbox"]
- ["bd", "close", "ingestr-dap", "--reason", "Completed", "--json"]
- ["bd", "create", "Add ingestr convert file command", "-t", "feature", "-p", "1", "--description", "Add `ingestr convert <FILE>` for quick ad-hoc conversion to Markdown (with YAML frontmatter). Default outputs to stdout; `--output PATH` writes to file; respects `--json/--yaml` and `--dry-run`. Includes unit test for frontmatter+content rendering.", "--json"]
- ["bd", "create", "Extend byt new scaffolding for git repos", "-t", "task", "-p", "2", "--description", "Add optional git-repo scaffolding support to byt new (clone repo or repo+subdir) so control plane can delegate scaffolding without duplicating logic.", "--json"]
- ["bd", "update", "ingestr-dap", "--status", "in_progress", "--json"]
- ["bun", "install"]
- ["bun", "run", "build"]
- ["bun", "run", "lint"]
- ["bunx", "biome", "format", "--write", "features/sessions/SessionScreen.tsx"]
- ["bunx", "biome", "format", "--write", "features/sessions/SessionScreen.tsx", "hooks/use-session-events.ts", "hooks/use-ws-session.ts"]
- ["cargo", "test"]
- ["cargo", "test", "-p", "rdgg-analysis", "-p", "rdgg-library", "-p", "rdgg-server"]
- ["curl", "-L", "--max-time", "20", "https://huggingface.co/docs/datasets/en/about_dataset"]
- ["curl", "-L", "--max-time", "20", "https://huggingface.co/docs/datasets/main/en/about_dataset"]
- ["curl", "-L", "--max-time", "20", "https://jsonlines.org/"]
- ["curl", "-L", "--max-time", "20", "https://mlflow.org/docs/latest/tracking.html"]
- ["curl", "-L", "https://opencode.ai/docs"]
- ["curl", "-L", "https://opencode.ai/docs/plugins"]
- ["curl", "-L", "https://opencode.ai/docs/sdk", "-o", "/tmp/opencode-sdk.html"]
- ["curl", "-sL", "https://agentskills.io/specification"]
- ["curl", "-sL", "https://api.github.com/repos/agentskills/agentskills/contents"]
- ["curl", "-sL", "https://api.github.com/repos/agentskills/agentskills/contents/docs"]
- ["curl", "-sL", "https://api.github.com/repos/anthropics/skills/contents/spec"]
- ["curl", "-sL", "https://raw.githubusercontent.com/agentskills/agentskills/main/docs/specification.mdx"]
- ["curl", "-sL", "https://raw.githubusercontent.com/anthropics/skills/main/README.md"]
- ["curl", "-sL", "https://raw.githubusercontent.com/anthropics/skills/main/spec/README.md"]
- ["curl", "-sL", "https://raw.githubusercontent.com/anthropics/skills/main/spec/agent-skills-spec.md"]
- ["curl", "-sS", "https://status.anthropic.com"]
- ["curl", "-sSL", "https://raw.githubusercontent.com/kyutai-labs/delayed-streams-modeling/main/README.md"]
- ["curl", "-sSL", "https://raw.githubusercontent.com/kyutai-labs/moshi/main/README.md"]
- ["curl", "-sSL", "https://raw.githubusercontent.com/kyutai-labs/moshi/main/rust/moshi-server/src/batched_asr.rs"]
- ["curl", "-sSL", "https://raw.githubusercontent.com/kyutai-labs/moshi/main/rust/moshi-server/src/server.rs"]
- ["curl", "-sSL", "https://raw.githubusercontent.com/kyutai-labs/parallel-streaming/main/README.md"]
- ["git", "clone", "https://github.com/Dimillian/CodexMonitor", "external-repos/CodexMonitor"]
- ["git", "clone", "https://github.com/sst/opencode", "/tmp/tmp.GbknLB1xvL/opencode"]
- ["git", "reset", "--soft", "HEAD~1"]
- ["just", "bump", "minor"]
- ["just", "bump", "patch"]
- ["just", "fix", "-p", "mmry-core"]
- ["just", "fix", "-p", "mmry-core", "-p", "mmry-cli", "-p", "mmry-mcp", "-p", "mmry-service", "-p", "mmry-tui"]
- ["just", "fix", "-p", "mmry-core", "-p", "mmry-cli", "-p", "mmry-service"]
- ["just", "fix", "-p", "mmry-service"]
- ["mailz-cli", "agent", "register", "--project", "govnr-3df7a466", "--name", "codex-mmry", "--model", "gpt-5", "--task", "mmry breaking change notice", "--json"]
- ["mailz-cli", "send", "opencode-govnr", "Breaking change: /console/data now requires auth when API key configured", "--body", "Heads-up: mmry external API now enforces auth via middleware. If external_api.api_key is set or require_api_key=true, /console/data now requires Authorization: Bearer <key>. The console UI has an API key field to send the header. /console (HTML) remains local-only.", "--json"]
- ["mkdir", "-p", ".tmp"]
- ["npm", "run", "build"]
- ["rm", "-f", "/home/wismut/Code/dgrmr/src/adapters/markdownAdapter.ts"]
- ["rm", "-f", "/home/wismut/Code/dgrmr/src/parser/types.ts"]
- ["rm", "-f", "/tmp/*.so"]
- ["rm", "-rf", "/home/wismut/Code/dgrmr/src/utils"]
- ["rm", "-rf", "/tmp/tmp.GbknLB1xvL", "/tmp/clawdis", "/tmp/node-compile-cache", "/tmp/clawdbot-analysis", "/tmp/oh-my-opencode", "/tmp/beads-analysis"]
- ["rm", "-rf", "src/components/MermaidCanvas.tsx", "src/components/ReactFlowCanvas.tsx", "src/components/MermaidEditor.tsx", "src/components/Toolbar.tsx", "src/components/NodePalette.tsx", "src/components/Minimap.tsx", "src/components/nodes", "src/components/edges", "src/hooks/useAutoSave.ts", "src/hooks/useCanvasStore.ts", "src/hooks/useDiagram.ts", "src/hooks/useFileWatch.ts", "src/store", "src/parser/mermaidParser.ts", "src/parser/mermaidGenerator.ts", "src/parser/flowchartParser.ts", "src/layout", "tests/parser", "tests/layout", "tests/integration/roundtrip.test.ts", "test-mermaid-temp.ts"]
- ["rm", "-rf", "uploads"]
- ["ss", "-ltnp"]
- ["sudo", "chown", "-R", "wismut:wismut", "/home/wismut/.local/share/octo"]
- ["sudo", "chown", "-R", "wismut:wismut", "/home/wismut/byteowlz"]
- ["tmux", "capture-pane", "-pt", "0:5", "-S", "-1000"]
- ["tmux", "capture-pane", "-pt", "0:5", "-S", "-200"]
- ["tmux", "capture-pane", "-t", "octo", "-p", "-S", "-200"]
- ["tmux", "list-panes", "-a", "-F", "#S:#I.#P #{pane_current_command} #{pane_title}"]
- ["tmux", "list-windows", "-a"]
- ["tmux", "ls"]
- ["trx", "close", "trx-3f4w.1", "-r", "Done"]
- ["trx", "close", "trx-3f4w.2", "-r", "Done"]
- ["trx", "close", "trx-3f4w.5", "-r", "Done"]
- ["trx", "close", "trx-3f4w.6", "-r", "Done"]
- ["trx", "create", "Add --clear for update fields", "-t", "feature", "-p", "3", "-d", "Allow trx update to clear optional fields (description, parent, labels) explicitly, e.g., --clear description."]
- ["trx", "create", "Add list filters (status/type/priority/search)", "-t", "feature", "-p", "2", "-d", "Improve trx list ergonomics with flags like --status open,closed --type bug,task --priority 0..4 and --search text. Support combined filters."]
- ["trx", "create", "Add model switcher + fix opencode permission/error events", "-t", "task", "-p", "2"]
- ["trx", "create", "Add one-off schedule support (run-at timestamp)", "-t", "feature", "-p", "2", "--description", "Support one-off scheduled runs via explicit timestamp (e.g., --at 2026-02-13 08:00), stored as single-run and auto-disable/remove after execution. Cron is 5-field only; need first-class once."]
- ["trx", "create", "Add trx sync options for message and dry-run", "-t", "feature", "-p", "3", "-d", "Support --message to set commit message and --dry-run or --no-commit to inspect staged changes without committing."]
- ["trx", "create", "Expose blockers in trx ready output", "-t", "feature", "-p", "3", "-d", "When an issue is blocked, show which dependencies are open to explain why it isn't ready."]
- ["trx", "create", "Implement dependency tree rendering", "-t", "feature", "-p", "2", "-d", "trx dep tree currently prints '(not yet implemented)'. Add traversal to display blockers/children with type labels and detect cycles."]
- ["trx", "create", "Show reverse dependencies in trx show", "-t", "feature", "-p", "2", "-d", "Add optional output (e.g., --reverse or default section) that lists issues blocked by this one and parent/child relationships, so epics can show their children."]
- ["trx", "create", "Support description input from file or stdin", "-t", "feature", "-p", "3", "-d", "For long issue text, allow trx create/update to read description from --description-file or --description - (stdin)."]
 The writable roots are `/home/wismut/byteowlz/octo`, `/tmp`.
</permissions instructions>

## user
_at 2026-02-02T09:38:49.000Z_

# AGENTS.md instructions for /home/wismut/byteowlz/octo

<INSTRUCTIONS>
# Important

- Never use emojis in README files or commit messages.
- Keep content for README files concise and to the point
- always document your changes

## Rust

always run cargo check after implementing changes. Fix any errors that occur.

## Python

always use uv for package management:

```bash
uv run <script_name.py> # Execute script

```

```bash
uv add <library_name> # install dependency

```

## JavaScript/TypeScript

always use bun for package management:

```bash
bun install # install packages

```

```bash
bun dev  # run server in dev mode
```

--- project-doc ---

# Octo - AI Agent Workspace Platform

Octo is a self-hosted platform for managing AI coding agents. Supports local mode (native processes) and container mode (Docker/Podman).

**New to Octo?** Start with the [SETUP.md](./SETUP.md) guide for installation and prerequisites.

---
## Debugging

Tmux is always available, use it to debug the logs of the running backend and frontend.

Use agent-browser + tmux for end-to-end testing

## Architecture Overview

Octo manages two types of AI agent sessions:

| Session Type | Agent Runtime | Purpose |
|--------------|---------------|---------|
| **Main Chat** | `pi` | Primary chat interface, streaming responses, compaction |
| **OpenCode Sessions** | `opencode` | Per-workspace coding agent sessions |

### Agent Runtimes

**pi** - Lightweight AI coding assistant CLI (`~/.bun/bin/pi`)
- Source code: `../external-repos/pi-mono`
- Used for Main Chat in the Octo UI
- Supports multiple providers (anthropic, openai, google)
- RPC mode for streaming via stdin/stdout JSON protocol
- Tools: read, bash, edit, write, grep, find, ls
- Extensions and skills system
- Session management with compaction

**opencode** - Full-featured coding agent
- Source code: `../external-repos/opencode`
- Used for workspace-specific sessions
- HTTP server mode (`opencode serve`)
- `x-opencode-directory` header switches working directory per request
- Currently: one opencode server per user serves all workspaces

### Runtime Modes

Both `pi` and `opencode` can run in different isolation modes:

| Mode | Description | Use Case |
|------|-------------|----------|
| `local` | Direct process spawn | Single-user, development |
| `runner` | Via `octo-runner` daemon | Multi-user Linux isolation |
| `container` | Inside Docker/Podman | Full container isolation |

### Key Binaries

| Binary | Purpose |
|--------|---------|
| `octo` | Main backend server |
| `octoctl` | CLI for server management |
| `octo-runner` | Multi-user process daemon (Linux only) |
| `octo-sandbox` | Sandbox wrapper using bwrap/sandbox-exec |
| `pi-bridge` | HTTP/WebSocket bridge for Pi in containers |
| `octo-files` | File access server for workspaces |

### External Dependencies

fresh clones of dependencies like opencode or pi-mono can be found in ../external-repos

### Process Sandboxing

Sandbox configuration in `~/.config/octo/sandbox.toml` (separate from main config for security):

```toml
enabled = true
profile = "development"  # or "minimal", "strict"
deny_read = ["~/.ssh", "~/.aws", "~/.gnupg"]
allow_write = ["~/.cargo", "~/.npm", "/tmp"]
isolate_network = false  # true in strict profile
isolate_pid = true
```

Per-workspace overrides in `.octo/sandbox.toml` can only ADD restrictions, never remove them.

---

## Recent Architecture Decisions

- Hstry is the canonical chat history store. Single-user main chat reads via hstry ReadService; multi-user reads via octo-runner against per-user hstry.db.
- Pi sessions rehydrate by rebuilding JSONL from hstry when the session file is missing.
- Main chat Pi WS connections bind to a specific session_id; runner writer errors trigger a single restart + retry.
- Provider storage: Pi JSONL stores provider in model_change entries and assistant message payloads; OpenCode stores provider per message as providerID in session message JSON.

## Memory System (Critical)

**ALWAYS search memories before starting work on unfamiliar areas.** Memories contain architecture decisions, API patterns, and debugging insights that save time.

```bash
# Search BEFORE implementing
agntz memory search "sandbox"
agntz memory search "pi runtime"
agntz memory search "opencode session"
```

**Create memories when you discover reusable knowledge:**

```bash
agntz memory add "Pi uses RPC mode with JSON over stdin/stdout for Main Chat streaming" -c architecture -i 8
agntz memory add "x-opencode-directory header switches workspace without restarting server" -c api -i 7
```

---

## Agent Tools

Two CLI tools are available for agent workflows:

| Tool | Purpose |
|------|---------|
| **byt** | Cross-repo governance and management (catalog, schemas, releases) |
| **agntz** | Day-to-day agent operations (memory, issues, mail, file reservations) |
| **sx** | External searches via SearXNG (`sx "<query>" -p`) |

### agntz - Agent Operations

```bash
agntz memory search "query"     # Search memories
agntz memory add "insight"      # Add a memory
agntz ready                     # Show unblocked issues
agntz issues                    # List all issues
agntz mail inbox                # Check messages
agntz reserve src/file.rs       # Reserve file for editing
agntz release src/file.rs       # Release reservation
```

### byt - Cross-Repo Management

```bash
byt catalog list                # List all repos
byt status                      # Show repo status
byt memory search "query" --all # Search across all stores
byt sync push                   # Sync memories to git
```

---

## Memory System (Critical)

**ALWAYS search memories before starting work on unfamiliar areas.** Memories contain architecture decisions, API patterns, and debugging insights that save time.

**Create memories when you discover reusable knowledge.** Memories are for patterns, interfaces, and insights - not atomic implementation details.

### When to Create Memories

Create a memory when you discover:

- **Reusable patterns** - "Voice mode uses eaRS for STT and kokorox for TTS via WebSocket"
- **Existing interfaces** - "OpenCode has PATCH /session/{id} for updating session title"
- **Architecture decisions** - "Chat history sessions use JSON files, live sessions use opencode API"
- **Debugging insights** - "Port cleanup requires waiting for process exit to prevent zombies"
- **Integration points** - "x-opencode-directory header switches working directory for any request"

### When NOT to Create Memories

Don't create memories for:

- Specific bug fixes in specific files
- One-off implementation details
- Things already documented in code comments
- Temporary workarounds

### Memory Commands

```bash
# Search before implementing (find existing solutions)
agntz memory search "voice mode"
agntz memory search "session rename"

# Add after discovering something reusable
agntz memory add "OpenCode PATCH /session/{id} accepts {title} to rename sessions" -c api -i 7
agntz memory add "features.voice config gates dictation and voice mode buttons" -c frontend -i 6

# Categories: api, frontend, backend, architecture, patterns, debugging
# Importance: 1-10 (7+ for significant insights)
```

### Memory Examples (Good)

```bash
agntz memory add "Chat sessions from disk need PATCH /api/chat-history/{id} since no opencode running" -c api -i 7
agntz memory add "useDictation hook provides STT-only mode separate from full voice mode" -c frontend -i 6
agntz memory add "SessionInfo stored in ~/.local/share/opencode/storage/session/{projectID}/ses_*.json" -c architecture -i 8
```

### Memory Examples (Bad)

```bash
# Too specific - this is implementation detail, not reusable knowledge
agntz memory add "Fixed bug in line 451 of app-context.tsx"

# Too vague - not actionable
agntz memory add "Voice stuff is complicated"
```

---

## Build/Lint/Test Commands

| Component | Build | Lint | Test | Single Test |
|-----------|-------|------|------|-------------|
| **backend/** | `cargo build` | `cargo clippy && cargo fmt --check` | `cargo test` | `cargo test test_name` |
| **fileserver/** | `cargo build` | `cargo clippy && cargo fmt --check` | `cargo test` | `cargo test test_name` |
| **frontend/** | `bun run build` | `bun run lint` | `bun run test` | `bun run test -t "pattern"` |

## Code Style

**Rust**: Use `anyhow::Result` with `.context()` for errors. Group imports: std, external crates, internal modules. Run `cargo fmt` and `cargo check` after changes.

**TypeScript**: Use `@/` import alias for internal modules. Functional components with named exports. Vitest for tests.

**General**: No emojis in code/docs/commits. Use `bun` for JS/TS, `uv` for Python.

---

## Issue Tracking (trx)

```bash
trx ready              # Show unblocked issues
trx create "Title" -t task -p 2   # Create issue (types: bug/feature/task/epic/chore, priority: 0-4)
trx update <id> --status in_progress
trx close <id> -r "Done"
trx sync               # Commit .trx/ changes
```

Priorities: 0=critical, 1=high, 2=medium, 3=low, 4=backlog

---

## Displaying Files to the User

To display files (images, documents, etc.) to the user in the chat interface, reference them with the `@` prefix followed by the file path:

```
Here's the generated image: @output/screenshot.png

I've created these files:
@src/components/Button.tsx
@docs/architecture.md
```

The UI will automatically render:
- **Images** (png, jpg, gif, webp, svg) as inline previews with thumbnails
- **Other files** as clickable links that open in the file viewer

Use workspace-relative paths (e.g., `@src/file.ts`) or absolute paths (e.g., `@/home/user/file.png`).


## Skills
A skill is a set of local instructions to follow that is stored in a `SKILL.md` file. Below is the list of skills that can be used. Each entry includes a name, description, and file path so you can open the source for full instructions when using a specific skill.
### Available skills
- rust-magic-linter: Add strict Clippy lint configurations to Rust projects for AI-assisted development. Use when the user wants to add Clippy lints, AI guardrails, or rust-magic-linter to a Rust project. Supports three presets (minimal, standard, maximum) that enforce best practices and prevent common AI coding mistakes like using unwrap(), silencing warnings with (file: /home/wismut/.codex/skills/rust-magic-linter/SKILL.md)
- skill-creator: Guide for creating effective skills. This skill should be used when users want to create a new skill (or update an existing skill) that extends Codex's capabilities with specialized knowledge, workflows, or tool integrations. (file: /home/wismut/.codex/skills/.system/skill-creator/SKILL.md)
- skill-installer: Install Codex skills into $CODEX_HOME/skills from a curated list or a GitHub repo path. Use when a user asks to list installable skills, install a curated skill, or install a skill from another repo (including private repos). (file: /home/wismut/.codex/skills/.system/skill-installer/SKILL.md)
### How to use skills
- Discovery: The list above is the skills available in this session (name + description + file path). Skill bodies live on disk at the listed paths.
- Trigger rules: If the user names a skill (with `$SkillName` or plain text) OR the task clearly matches a skill's description shown above, you must use that skill for that turn. Multiple mentions mean use them all. Do not carry skills across turns unless re-mentioned.
- Missing/blocked: If a named skill isn't in the list or the path can't be read, say so briefly and continue with the best fallback.
- How to use a skill (progressive disclosure):
  1) After deciding to use a skill, open its `SKILL.md`. Read only enough to follow the workflow.
  2) If `SKILL.md` points to extra folders such as `references/`, load only the specific files needed for the request; don't bulk-load everything.
  3) If `scripts/` exist, prefer running or patching them instead of retyping large code blocks.
  4) If `assets/` or templates exist, reuse them instead of recreating from scratch.
- Coordination and sequencing:
  - If multiple skills apply, choose the minimal set that covers the request and state the order you'll use them.
  - Announce which skill(s) you're using and why (one short line). If you skip an obvious skill, say why.
- Context hygiene:
  - Keep context small: summarize long sections instead of pasting them; only load extra files when needed.
  - Avoid deep reference-chasing: prefer opening only files directly linked from `SKILL.md` unless you're blocked.
  - When variants exist (frameworks, providers, domains), pick only the relevant reference file(s) and note that choice.
- Safety and fallback: If a skill can't be applied cleanly (missing files, unclear instructions), state the issue, pick the next-best approach, and continue.
</INSTRUCTIONS>

## user
_at 2026-02-02T09:38:49.000Z_

<environment_context>
  <cwd>/home/wismut/byteowlz/octo</cwd>
  <shell>zsh</shell>
</environment_context>

## user
_at 2026-02-02T09:39:44.000Z_

read the end of the latest session document and continue the fixes. We have migrated to a single multiplex ws per user that can drive multiple pi sessions + ttyd + hstry + mmry + octo-files.

## user
_at 2026-02-02T09:40:13.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## user
_at 2026-02-02T09:40:24.000Z_

there is a session*.md in this repo

## assistant
_at 2026-02-02T09:41:31.000Z_

Updated the Main Chat primary send action to use `prompt` and documented the change.

Details
- Changed the send button handler to `handleSend("prompt")` in `frontend/features/main-chat/components/MainChatPiView.tsx:1607`.
- Logged the change in `docs/changes.md`.

Notes
- `agntz memory search "ws multiplex"` failed: mmry service attempted to start but the database is read-only. If you want, I can retry after mmry is writable.
- `cargo check` must run after changes; it failed in repo root (no Cargo.toml) and succeeded in `backend/` with existing warnings (ts-rs serde attribute parsing, unused imports, unsafe blocks, dead_code). Let me know if you want me to chase any of these now.

Suggested next steps
1. `cd frontend && bun run build`
2. Verify the multiplex WS flow in the browser console and backend logs.

Want me to run the frontend build and watch the tmux logs for the multiplex WS events?

## user
_at 2026-02-02T09:46:20.000Z_

Error with Permissions-Policy header: Unrecognized feature: 'browsing-topics'.
Error with Permissions-Policy header: Unrecognized feature: 'run-ad-auction'.
Error with Permissions-Policy header: Origin trial controlled feature not enabled: 'join-ad-interest-group'.
client:495 [vite] connecting...
client:618 [vite] connected.
use-auth.ts:22 [auth] fetchCurrentUser http://archlinux:8080/api/me
use-auth.ts:36 [auth] /me response: 200
use-auth.ts:55 [auth] /me user id: wismut
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
react-dom_client.js?v=eeedd95f:13259 [Violation] 'setTimeout' handler took 72ms
installHook.js:1 Encountered two children with the same key, `36e4ec42-ba8c-4db6-b3e5-25dcd9f82a1b`. Keys should be unique so that components maintain their identity across updates. Non-unique keys may cause children to be duplicated and/or omitted — the behavior is unsupported and could change in a future version.
overrideMethod @ installHook.js:1
(anonymous) @ react-dom_client.js?v=eeedd95f:4657
runWithFiberInDEV @ react-dom_client.js?v=eeedd95f:729
warnOnInvalidKey @ react-dom_client.js?v=eeedd95f:4656
reconcileChildrenArray @ react-dom_client.js?v=eeedd95f:4697
reconcileChildFibersImpl @ react-dom_client.js?v=eeedd95f:4903
(anonymous) @ react-dom_client.js?v=eeedd95f:4969
reconcileChildren @ react-dom_client.js?v=eeedd95f:6914
beginWork @ react-dom_client.js?v=eeedd95f:8433
performUnitOfWork @ react-dom_client.js?v=eeedd95f:12293
workLoopSync @ react-dom_client.js?v=eeedd95f:12156
renderRootSync @ react-dom_client.js?v=eeedd95f:12140
performWorkOnRoot @ react-dom_client.js?v=eeedd95f:11498
performWorkOnRootViaSchedulerTask @ react-dom_client.js?v=eeedd95f:13237
performWorkUntilDeadline @ chunk-AAYXMYGC.js?v=eeedd95f:30
<ContextMenu>
exports.jsxDEV @ react_jsx-dev-runtime.js?v=eeedd95f:247
(anonymous) @ MainChatEntry.tsx:680
MainChatEntry @ MainChatEntry.tsx:653
react_stack_bottom_frame @ react-dom_client.js?v=eeedd95f:18241
renderWithHooksAgain @ react-dom_client.js?v=eeedd95f:5461
renderWithHooks @ react-dom_client.js?v=eeedd95f:5397
updateFunctionComponent @ react-dom_client.js?v=eeedd95f:7207
beginWork @ react-dom_client.js?v=eeedd95f:8257
reconcileChildrenArray @ react-dom_client.js?v=eeedd95f:4681
workLoopConcurrentByScheduler @ react-dom_client.js?v=eeedd95f:12289
renderRootConcurrent @ react-dom_client.js?v=eeedd95f:12271
chunk-AAYXMYGC.js?v=eeedd95f:10 [Violation] 'message' handler took 226ms
usePiChatV2.ts:386 Uncaught (in promise) Error: No active session
    at usePiChatV2.ts:386:11
    at MainChatPiView.tsx:1013:10
    at MainChatPiView.tsx:1051:5
    at executeDispatch (react-dom_client.js?v=eeedd95f:13354:11)
    at runWithFiberInDEV (react-dom_client.js?v=eeedd95f:729:72)
    at processDispatchQueue (react-dom_client.js?v=eeedd95f:13390:37)
    at react-dom_client.js?v=eeedd95f:13803:11
    at batchedUpdates$1 (react-dom_client.js?v=eeedd95f:2358:42)
    at dispatchEventForPluginEventSystem (react-dom_client.js?v=eeedd95f:13495:9)
    at dispatchEvent (react-dom_client.js?v=eeedd95f:16516:13)
(anonymous) @ usePiChatV2.ts:386
(anonymous) @ MainChatPiView.tsx:1013
(anonymous) @ MainChatPiView.tsx:1051
executeDispatch @ react-dom_client.js?v=eeedd95f:13354
processDispatchQueue @ react-dom_client.js?v=eeedd95f:13390
(anonymous) @ react-dom_client.js?v=eeedd95f:13803
batchedUpdates$1 @ react-dom_client.js?v=eeedd95f:2358
dispatchEventForPluginEventSystem @ react-dom_client.js?v=eeedd95f:13495
dispatchEvent @ react-dom_client.js?v=eeedd95f:16516
dispatchDiscreteEvent @ react-dom_client.js?v=eeedd95f:16497
performSyncWorkOnRoot @ react-dom_client.js?v=eeedd95f:13249
flushSyncWorkAcrossRoots_impl @ react-dom_client.js?v=eeedd95f:13146
processRootScheduleInMicrotask @ react-dom_client.js?v=eeedd95f:13169
(anonymous) @ react-dom_client.js?v=eeedd95f:13263
ws-manager.ts:246 [ws-mux] subscribePiSession: 8194b969-08ba-4d90-b5ba-f05a78d8fde3 config: {cwd: '/home/wismut/.local/share/octo/users/wismut/main'} isConnected: false
ws-manager.ts:276 [ws-mux] Not connected, queueing subscription for: 8194b969-08ba-4d90-b5ba-f05a78d8fde3
ws-manager.ts:119 [ws-mux] Connecting to WebSocket...
ws-manager.ts:442 [ws-mux] Connecting to ws://archlinux:8080/api/ws/mux?token=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ3aXNtdXQiLCJpc3MiOiJ3b3Jrc3BhY2UtYmFja2VuZCIsImF1ZCI6bnVsbCwiZXhwIjoxNzcwMTEwMDA4LCJpYXQiOjE3NzAwMjM2MDgsIm5iZiI6bnVsbCwianRpIjpudWxsLCJlbWFpbCI6ImRldkBsb2NhbGhvc3QiLCJuYW1lIjoiRGV2ZWxvcGVyIiwicHJlZmVycmVkX3VzZXJuYW1lIjoid2lzbXV0Iiwicm9sZXMiOlsiYWRtaW4iXSwicm9sZSI6ImFkbWluIn0.VfH9tvC5M14di4OqIl6p_TuNh9faI3o9IfIr2BjaHB4
ws-manager.ts:115 [ws-mux] Connection already in progress
ws-manager.ts:285 [ws-mux] Already subscribed to session: 8194b969-08ba-4d90-b5ba-f05a78d8fde3
react-dom_client.js?v=eeedd95f:13259 [Violation] 'click' handler took 300ms
ws-manager.ts:447 [ws-mux] Connected!
ws-manager.ts:452 [ws-mux] Processing pending subscriptions: 1
ws-manager.ts:453 [ws-mux] Processing subscribed sessions: 1
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"8194b969-08ba-4d90-b5ba-f05a78d8fde3","config":{"cwd":"/home/wismut/.local/share/octo/users/wismut/main"}}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"8194b969-08ba-4d90-b5ba-f05a78d8fde3"}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"prompt","session_id":"8194b969-08ba-4d90-b5ba-f05a78d8fde3","message":"hello"}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"unsubscribe","session_id":"8194b969-08ba-4d90-b5ba-f05a78d8fde3"}
ws-manager.ts:246 [ws-mux] subscribePiSession: 49281280-ee99-496b-a168-e9054a44001e config: {cwd: '/home/wismut/.local/share/octo/users/wismut/main'} isConnected: true
ws-manager.ts:261 [ws-mux] Sending create_session + subscribe for: 49281280-ee99-496b-a168-e9054a44001e
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"49281280-ee99-496b-a168-e9054a44001e","config":{"cwd":"/home/wismut/.local/share/octo/users/wismut/main"}}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"49281280-ee99-496b-a168-e9054a44001e"}
ws-manager.ts:110 [ws-mux] Already connected
ws-manager.ts:285 [ws-mux] Already subscribed to session: 49281280-ee99-496b-a168-e9054a44001e
react-dom_client.js?v=eeedd95f:13259 [Violation] 'click' handler took 305ms
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"unsubscribe","session_id":"49281280-ee99-496b-a168-e9054a44001e"}
ws-manager.ts:246 [ws-mux] subscribePiSession: 85fb93a2-7889-4186-a64d-7c3af0d20fe1 config: {cwd: '/home/wismut/.local/share/octo/users/wismut/main'} isConnected: true
ws-manager.ts:261 [ws-mux] Sending create_session + subscribe for: 85fb93a2-7889-4186-a64d-7c3af0d20fe1
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"85fb93a2-7889-4186-a64d-7c3af0d20fe1","config":{"cwd":"/home/wismut/.local/share/octo/users/wismut/main"}}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"85fb93a2-7889-4186-a64d-7c3af0d20fe1"}
ws-manager.ts:285 [ws-mux] Already subscribed to session: 85fb93a2-7889-4186-a64d-7c3af0d20fe1
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"prompt","session_id":"85fb93a2-7889-4186-a64d-7c3af0d20fe1","message":"hello"}
Error with Permissions-Policy header: Unrecognized feature: 'browsing-topics'.
Error with Permissions-Policy header: Unrecognized feature: 'run-ad-auction'.
Error with Permissions-Policy header: Origin trial controlled feature not enabled: 'join-ad-interest-group'.
client:495 [vite] connecting...
client:618 [vite] connected.
use-auth.ts:22 [auth] fetchCurrentUser http://archlinux:8080/api/me
use-auth.ts:36 [auth] /me response: 200
use-auth.ts:55 [auth] /me user id: wismut
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
react-dom_client.js?v=eeedd95f:13259 [Violation] 'setTimeout' handler took 72ms
installHook.js:1 Encountered two children with the same key, `36e4ec42-ba8c-4db6-b3e5-25dcd9f82a1b`. Keys should be unique so that components maintain their identity across updates. Non-unique keys may cause children to be duplicated and/or omitted — the behavior is unsupported and could change in a future version.
overrideMethod @ installHook.js:1
(anonymous) @ react-dom_client.js?v=eeedd95f:4657
runWithFiberInDEV @ react-dom_client.js?v=eeedd95f:729
warnOnInvalidKey @ react-dom_client.js?v=eeedd95f:4656
reconcileChildrenArray @ react-dom_client.js?v=eeedd95f:4697
reconcileChildFibersImpl @ react-dom_client.js?v=eeedd95f:4903
(anonymous) @ react-dom_client.js?v=eeedd95f:4969
reconcileChildren @ react-dom_client.js?v=eeedd95f:6914
beginWork @ react-dom_client.js?v=eeedd95f:8433
performUnitOfWork @ react-dom_client.js?v=eeedd95f:12293
workLoopSync @ react-dom_client.js?v=eeedd95f:12156
renderRootSync @ react-dom_client.js?v=eeedd95f:12140
performWorkOnRoot @ react-dom_client.js?v=eeedd95f:11498
performWorkOnRootViaSchedulerTask @ react-dom_client.js?v=eeedd95f:13237
performWorkUntilDeadline @ chunk-AAYXMYGC.js?v=eeedd95f:30
<ContextMenu>
exports.jsxDEV @ react_jsx-dev-runtime.js?v=eeedd95f:247
(anonymous) @ MainChatEntry.tsx:680
MainChatEntry @ MainChatEntry.tsx:653
react_stack_bottom_frame @ react-dom_client.js?v=eeedd95f:18241
renderWithHooksAgain @ react-dom_client.js?v=eeedd95f:5461
renderWithHooks @ react-dom_client.js?v=eeedd95f:5397
updateFunctionComponent @ react-dom_client.js?v=eeedd95f:7207
beginWork @ react-dom_client.js?v=eeedd95f:8257
reconcileChildrenArray @ react-dom_client.js?v=eeedd95f:4681
workLoopConcurrentByScheduler @ react-dom_client.js?v=eeedd95f:12289
renderRootConcurrent @ react-dom_client.js?v=eeedd95f:12271
[Violation] 'message' handler took <N>ms
usePiChatV2.ts:386 Uncaught (in promise) Error: No active session
    at usePiChatV2.ts:386:11
    at MainChatPiView.tsx:1013:10
    at MainChatPiView.tsx:1051:5
    at executeDispatch (react-dom_client.js?v=eeedd95f:13354:11)
    at runWithFiberInDEV (react-dom_client.js?v=eeedd95f:729:72)
    at processDispatchQueue (react-dom_client.js?v=eeedd95f:13390:37)
    at react-dom_client.js?v=eeedd95f:13803:11
    at batchedUpdates$1 (react-dom_client.js?v=eeedd95f:2358:42)
    at dispatchEventForPluginEventSystem (react-dom_client.js?v=eeedd95f:13495:9)
    at dispatchEvent (react-dom_client.js?v=eeedd95f:16516:13)
(anonymous) @ usePiChatV2.ts:386
(anonymous) @ MainChatPiView.tsx:1013
(anonymous) @ MainChatPiView.tsx:1051
executeDispatch @ react-dom_client.js?v=eeedd95f:13354
processDispatchQueue @ react-dom_client.js?v=eeedd95f:13390
(anonymous) @ react-dom_client.js?v=eeedd95f:13803
batchedUpdates$1 @ react-dom_client.js?v=eeedd95f:2358
dispatchEventForPluginEventSystem @ react-dom_client.js?v=eeedd95f:13495
dispatchEvent @ react-dom_client.js?v=eeedd95f:16516
dispatchDiscreteEvent @ react-dom_client.js?v=eeedd95f:16497
performSyncWorkOnRoot @ react-dom_client.js?v=eeedd95f:13249
flushSyncWorkAcrossRoots_impl @ react-dom_client.js?v=eeedd95f:13146
processRootScheduleInMicrotask @ react-dom_client.js?v=eeedd95f:13169
(anonymous) @ react-dom_client.js?v=eeedd95f:13263
ws-manager.ts:246 [ws-mux] subscribePiSession: 8194b969-08ba-4d90-b5ba-f05a78d8fde3 config: {cwd: '/home/wismut/.local/share/octo/users/wismut/main'} isConnected: false
ws-manager.ts:276 [ws-mux] Not connected, queueing subscription for: 8194b969-08ba-4d90-b5ba-f05a78d8fde3
ws-manager.ts:119 [ws-mux] Connecting to WebSocket...
ws-manager.ts:442 [ws-mux] Connecting to ws://archlinux:8080/api/ws/mux?token=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ3aXNtdXQiLCJpc3MiOiJ3b3Jrc3BhY2UtYmFja2VuZCIsImF1ZCI6bnVsbCwiZXhwIjoxNzcwMTEwMDA4LCJpYXQiOjE3NzAwMjM2MDgsIm5iZiI6bnVsbCwianRpIjpudWxsLCJlbWFpbCI6ImRldkBsb2NhbGhvc3QiLCJuYW1lIjoiRGV2ZWxvcGVyIiwicHJlZmVycmVkX3VzZXJuYW1lIjoid2lzbXV0Iiwicm9sZXMiOlsiYWRtaW4iXSwicm9sZSI6ImFkbWluIn0.VfH9tvC5M14di4OqIl6p_TuNh9faI3o9IfIr2BjaHB4
ws-manager.ts:115 [ws-mux] Connection already in progress
ws-manager.ts:285 [ws-mux] Already subscribed to session: 8194b969-08ba-4d90-b5ba-f05a78d8fde3
react-dom_client.js?v=eeedd95f:13259 [Violation] 'click' handler took 300ms
ws-manager.ts:447 [ws-mux] Connected!
ws-manager.ts:452 [ws-mux] Processing pending subscriptions: 1
ws-manager.ts:453 [ws-mux] Processing subscribed sessions: 1
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"8194b969-08ba-4d90-b5ba-f05a78d8fde3","config":{"cwd":"/home/wismut/.local/share/octo/users/wismut/main"}}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"8194b969-08ba-4d90-b5ba-f05a78d8fde3"}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"prompt","session_id":"8194b969-08ba-4d90-b5ba-f05a78d8fde3","message":"hello"}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"unsubscribe","session_id":"8194b969-08ba-4d90-b5ba-f05a78d8fde3"}
ws-manager.ts:246 [ws-mux] subscribePiSession: 49281280-ee99-496b-a168-e9054a44001e config: {cwd: '/home/wismut/.local/share/octo/users/wismut/main'} isConnected: true
ws-manager.ts:261 [ws-mux] Sending create_session + subscribe for: 49281280-ee99-496b-a168-e9054a44001e
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"49281280-ee99-496b-a168-e9054a44001e","config":{"cwd":"/home/wismut/.local/share/octo/users/wismut/main"}}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"49281280-ee99-496b-a168-e9054a44001e"}
ws-manager.ts:110 [ws-mux] Already connected
ws-manager.ts:285 [ws-mux] Already subscribed to session: 49281280-ee99-496b-a168-e9054a44001e
react-dom_client.js?v=eeedd95f:13259 [Violation] 'click' handler took 305ms
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"unsubscribe","session_id":"49281280-ee99-496b-a168-e9054a44001e"}
ws-manager.ts:246 [ws-mux] subscribePiSession: 85fb93a2-7889-4186-a64d-7c3af0d20fe1 config: {cwd: '/home/wismut/.local/share/octo/users/wismut/main'} isConnected: true
ws-manager.ts:261 [ws-mux] Sending create_session + subscribe for: 85fb93a2-7889-4186-a64d-7c3af0d20fe1
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"85fb93a2-7889-4186-a64d-7c3af0d20fe1","config":{"cwd":"/home/wismut/.local/share/octo/users/wismut/main"}}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"85fb93a2-7889-4186-a64d-7c3af0d20fe1"}
ws-manager.ts:285 [ws-mux] Already subscribed to session: 85fb93a2-7889-4186-a64d-7c3af0d20fe1
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"prompt","session_id":"85fb93a2-7889-4186-a64d-7c3af0d20fe1","message":"hello"}
react-dom_client.js?v=eeedd95f:13259 [Violation] 'click' handler took 272ms
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"unsubscribe","session_id":"85fb93a2-7889-4186-a64d-7c3af0d20fe1"}
ws-manager.ts:246 [ws-mux] subscribePiSession: pending-1770025490622-qfz01eyk config: {cwd: '/home/wismut/byteowlz/octo'} isConnected: true
ws-manager.ts:261 [ws-mux] Sending create_session + subscribe for: pending-1770025490622-qfz01eyk
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"pending-1770025490622-qfz01eyk","config":{"cwd":"/home/wismut/byteowlz/octo"}}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"pending-1770025490622-qfz01eyk"}
ws-manager.ts:285 [ws-mux] Already subscribed to session: pending-1770025490622-qfz01eyk
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"unsubscribe","session_id":"pending-1770025490622-qfz01eyk"}
ws-manager.ts:246 [ws-mux] subscribePiSession: 0b15bbeb-c6c4-4839-96d9-0f1483d21dff config: {cwd: '/home/wismut/byteowlz/octo'} isConnected: true
ws-manager.ts:261 [ws-mux] Sending create_session + subscribe for: 0b15bbeb-c6c4-4839-96d9-0f1483d21dff
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"0b15bbeb-c6c4-4839-96d9-0f1483d21dff","config":{"cwd":"/home/wismut/byteowlz/octo"}}
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"0b15bbeb-c6c4-4839-96d9-0f1483d21dff"}
ws-manager.ts:285 [ws-mux] Already subscribed to session: 0b15bbeb-c6c4-4839-96d9-0f1483d21dff
ws-manager.ts:149 [ws-mux] Sending: {"channel":"pi","type":"prompt","session_id":"0b15bbeb-c6c4-4839-96d9-0f1483d21dff","message":"hello"}
react-dom_client.js?v=eeedd95f:13259 [Violation] 'keydown' handler took 197ms
 I dont get any response an no "working" indication. Use agent-browser for live debugging user:wismut pass:dev . Skill und ~/.pi/agent/skills/agent-browser

## user
_at 2026-02-02T09:47:02.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## user
_at 2026-02-02T09:47:04.000Z_

3000

## user
_at 2026-02-02T09:48:12.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## assistant
_at 2026-02-02T09:48:14.000Z_

<permissions instructions>
Filesystem sandboxing defines which files can be read or written. `sandbox_mode` is `danger-full-access`: No filesystem sandboxing - all commands are permitted. Network access is enabled.
 Approvals are your mechanism to get user consent to run shell commands without the sandbox. `approval_policy` is `never`: This is a non-interactive mode where you may NEVER ask the user for approval to run commands. Instead, you must always persist and work around constraints to solve the task for the user. You MUST do your utmost best to finish the task and validate your work before yielding. If this mode is paired with `danger-full-access`, take advantage of it to deliver the best outcome for the user. Further, in this mode, your default testing philosophy is overridden: Even if you don't see local patterns for testing, you may add tests and scripts to validate your work. Just remove them before yielding.
</permissions instructions>

## assistant
_at 2026-02-02T09:48:22.000Z_

<permissions instructions>
Filesystem sandboxing defines which files can be read or written. `sandbox_mode` is `danger-full-access`: No filesystem sandboxing - all commands are permitted. Network access is enabled.
 Approvals are your mechanism to get user consent to run shell commands without the sandbox. `approval_policy` is `never`: This is a non-interactive mode where you may NEVER ask the user for approval to run commands. Instead, you must always persist and work around constraints to solve the task for the user. You MUST do your utmost best to finish the task and validate your work before yielding. If this mode is paired with `danger-full-access`, take advantage of it to deliver the best outcome for the user. Further, in this mode, your default testing philosophy is overridden: Even if you don't see local patterns for testing, you may add tests and scripts to validate your work. Just remove them before yielding.
</permissions instructions>

## user
_at 2026-02-02T09:48:22.000Z_

try again you now have better permissions

## user
_at 2026-02-02T09:48:31.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## user
_at 2026-02-02T09:48:37.000Z_

it is already installed!

## user
_at 2026-02-02T09:51:36.000Z_

[image] data:image/png;base64,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

## assistant
_at 2026-02-02T10:00:54.000Z_

Stopped the ws-mux reconnect loop and duplicate create/subscribe sends, plus documented the change. The browser was still running the old ws-mux code (pong timeout logs kept showing), so you’ll need to restart the dev server to validate.

Changes
- Disable unused pong-timeout logic that was closing the mux socket.
- Avoid double-sending create_session/subscribe when a session is both pending + subscribed.
- Logged the change in `docs/changes.md`.

Files touched
- `frontend/lib/ws-manager.ts`
- `docs/changes.md`

Build
- `bun run build` (frontend) completed.

Why you still saw “Pong timeout”
- The console logs show the old ws-manager behavior, which means the dev server didn’t pick up the change. It still closes the mux socket every ~10s, so prompts never reach the backend.

Next steps to validate
1. Restart the frontend dev server (`bun dev`).
2. Reload the UI and send a “hello”.
3. Confirm no “Pong timeout” logs and that backend logs show `Pi prompt` in `backend/crates/octo/src/api/ws_multiplexed.rs`.

If you want, I can restart the dev server and re-run the agent-browser check end-to-end.

## user
_at 2026-02-02T10:11:35.000Z_

still not getting any responses. Any my already sent messages appear in the text input box for new chats

## assistant
_at 2026-02-02T10:14:49.000Z_

I fixed the draft bleed across sessions and added a queue so Pi prompts won’t be sent until the session is actually created (session_created). This should address both “old text in new chat input” and “no responses”.

What I changed
- Main Chat drafts are now keyed per session, not global, so input won’t carry over between chats.
- Pi messages (prompt/steer/follow_up) are queued until `session_created` arrives, then flushed.
- Kept the earlier WS reconnect fixes; documented all changes.

Files
- `frontend/lib/ws-manager.ts`
- `frontend/features/main-chat/components/MainChatPiView.tsx`
- `docs/changes.md`

Build
- `bun run build` (frontend) completed.

What you need to do now (important)
1. Restart the frontend dev server so the new `ws-manager` code is loaded (HMR hasn’t been picking it up):
   - Stop `bun dev` and run it again (or `just run-frontend`).
2. Hard-reload the UI.
3. Send a new “hello”.

What to watch for
- In the browser console, you should now see a `pi/session_created` event before the queued prompt is sent.
- In backend logs, you should see `Pi prompt: user=...` in `backend/crates/octo/src/api/ws_multiplexed.rs`.
- In `/tmp/octo-runner.log`, you should see `pi_prompt: session_id=...`.

If you want, I can restart the dev server and validate via agent-browser again.

## user
_at 2026-02-02T10:24:38.000Z_

Error with Permissions-Policy header: Unrecognized feature: 'browsing-topics'.
Error with Permissions-Policy header: Unrecognized feature: 'run-ad-auction'.
Error with Permissions-Policy header: Origin trial controlled feature not enabled: 'join-ad-interest-group'.
client:495 [vite] connecting...
client:618 [vite] connected.
use-auth.ts:22 [auth] fetchCurrentUser http://archlinux:8080/api/me
use-auth.ts:36 [auth] /me response: 200
use-auth.ts:55 [auth] /me user id: wismut
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
react-dom_client.js?v=eeedd95f:13259 [Violation] 'setTimeout' handler took 72ms
ws-manager.ts:251 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: {cwd: '/home/wismut/byteowlz/octo'} isConnected: false
ws-manager.ts:282 [ws-mux] Not connected, queueing subscription for: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:124 [ws-mux] Connecting to WebSocket...
ws-manager.ts:432 [ws-mux] Connecting to ws://archlinux:8080/api/ws/mux?token=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ3aXNtdXQiLCJpc3MiOiJ3b3Jrc3BhY2UtYmFja2VuZCIsImF1ZCI6bnVsbCwiZXhwIjoxNzcwMTEwMDA4LCJpYXQiOjE3NzAwMjM2MDgsIm5iZiI6bnVsbCwianRpIjpudWxsLCJlbWFpbCI6ImRldkBsb2NhbGhvc3QiLCJuYW1lIjoiRGV2ZWxvcGVyIiwicHJlZmVycmVkX3VzZXJuYW1lIjoid2lzbXV0Iiwicm9sZXMiOlsiYWRtaW4iXSwicm9sZSI6ImFkbWluIn0.VfH9tvC5M14di4OqIl6p_TuNh9faI3o9IfIr2BjaHB4
ws-manager.ts:120 [ws-mux] Connection already in progress
ws-manager.ts:291 [ws-mux] Already subscribed to session: dfde9584-841a-40ea-a733-9e035a78d5a2
installHook.js:1 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: {cwd: '/home/wismut/byteowlz/octo'} isConnected: false
installHook.js:1 [ws-mux] Already subscribed to session: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:437 [ws-mux] Connected!
ws-manager.ts:442 [ws-mux] Processing pending subscriptions: 1
ws-manager.ts:443 [ws-mux] Processing subscribed sessions: 1
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"dfde9584-841a-40ea-a733-9e035a78d5a2","config":{"cwd":"/home/wismut/byteowlz/octo"}}
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"dfde9584-841a-40ea-a733-9e035a78d5a2"}
[Violation] 'message' handler took <N>ms
ws-manager.ts:602 [ws-mux] Queued Pi message until session is ready: dfde9584-841a-40ea-a733-9e035a78d5a2 prompt
react-dom_client.js?v=eeedd95f:13259 [Violation] 'keydown' handler took 217ms
 I dont think the pi sessions get started or that messages go through. Can you check the backend/runner/pi?

## assistant
_at 2026-02-02T10:40:41.000Z_

Updated the ws-mux send path so Pi prompts don’t stall waiting on `session_created` and now flush any queued prompts as soon as the WS connects (queue only when disconnected). Also documented the change.

Changes:
- `frontend/lib/ws-manager.ts` — queue only while disconnected; flush pending prompts on WS connect.
- `docs/changes.md` — log entry for the new prompt send behavior.

Tests:
- `bun run build` (frontend)

Notes / verify:
- After reload, backend log should show `Received WS command: Pi(Prompt...)` right after a send. If it still doesn’t, the next likely culprit is “No active session” (check console) — we’d need to ensure `selectedSessionId` is set before send.

If you want, I can add a small guard to auto-create a Pi session when `selectedSessionId` is missing, or instrument a single debug log to confirm send execution.

Next steps you might want:
1) Refresh the UI and check backend log for a `Pi(Prompt...)` line.
2) If absent, confirm whether the console still shows “No active session.”

## user
_at 2026-02-02T10:40:41.000Z_

and the terminal cant connect currently

## assistant
_at 2026-02-02T10:44:33.000Z_

Terminal connect should be fixed now. The WS proxy path was missing `/api` when a direct base URL was set, so the browser was hitting `ws://archlinux:8080/workspace/term` and getting 404. I updated the terminal WS URL construction and the proxy path, and the backend log now shows a successful WS upgrade (`status=101`) for `/workspace/term` (legacy route still works).

Changes:
- `frontend/features/sessions/components/TerminalView.tsx` — build terminal WS URL via `controlPlaneApiUrl`.
- `frontend/lib/api/files.ts` — prefix terminal proxy paths with `/api`.
- `docs/changes.md` — documented the terminal fix.

Tests:
- `bun run build`

Can you hard-reload the UI and try the Terminal tab again? If it still fails, paste the newest terminal WS error line from the console (it should now include `/api/workspace/term` if it’s still wrong).

## user
_at 2026-02-02T10:45:04.000Z_

Error with Permissions-Policy header: Unrecognized feature: 'browsing-topics'.
Error with Permissions-Policy header: Unrecognized feature: 'run-ad-auction'.
Error with Permissions-Policy header: Origin trial controlled feature not enabled: 'join-ad-interest-group'.
client:495 [vite] connecting...
client:618 [vite] connected.
use-auth.ts:22 [auth] fetchCurrentUser http://archlinux:8080/api/me
use-auth.ts:36 [auth] /me response: 200
use-auth.ts:55 [auth] /me user id: wismut
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
react-dom_client.js?v=eeedd95f:13259 [Violation] 'setTimeout' handler took 75ms
ws-manager.ts:251 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: {cwd: '/home/wismut/byteowlz/octo'} isConnected: false
ws-manager.ts:282 [ws-mux] Not connected, queueing subscription for: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:124 [ws-mux] Connecting to WebSocket...
ws-manager.ts:433 [ws-mux] Connecting to ws://archlinux:8080/api/ws/mux?token=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ3aXNtdXQiLCJpc3MiOiJ3b3Jrc3BhY2UtYmFja2VuZCIsImF1ZCI6bnVsbCwiZXhwIjoxNzcwMTEwMDA4LCJpYXQiOjE3NzAwMjM2MDgsIm5iZiI6bnVsbCwianRpIjpudWxsLCJlbWFpbCI6ImRldkBsb2NhbGhvc3QiLCJuYW1lIjoiRGV2ZWxvcGVyIiwicHJlZmVycmVkX3VzZXJuYW1lIjoid2lzbXV0Iiwicm9sZXMiOlsiYWRtaW4iXSwicm9sZSI6ImFkbWluIn0.VfH9tvC5M14di4OqIl6p_TuNh9faI3o9IfIr2BjaHB4
ws-manager.ts:120 [ws-mux] Connection already in progress
ws-manager.ts:251 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: {cwd: '/home/wismut/byteowlz/octo'} isConnected: false
ws-manager.ts:291 [ws-mux] Already subscribed to session: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:120 [ws-mux] Connection already in progress
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
installHook.js:1 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: {cwd: '/home/wismut/byteowlz/octo'} isConnected: false
installHook.js:1 [ws-mux] Already subscribed to session: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:120 [ws-mux] Connection already in progress
installHook.js:1 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: {cwd: '/home/wismut/byteowlz/octo'} isConnected: false
installHook.js:1 [ws-mux] Already subscribed to session: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:120 [ws-mux] Connection already in progress
ws-manager.ts:438 [ws-mux] Connected!
ws-manager.ts:443 [ws-mux] Processing pending subscriptions: 1
ws-manager.ts:444 [ws-mux] Processing subscribed sessions: 1
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"dfde9584-841a-40ea-a733-9e035a78d5a2","config":{"cwd":"/home/wismut/byteowlz/octo"}}
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"dfde9584-841a-40ea-a733-9e035a78d5a2"}
chunk-AAYXMYGC.js?v=eeedd95f:10 [Violation] 'message' handler took 191ms
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"prompt","session_id":"dfde9584-841a-40ea-a733-9e035a78d5a2","message":"hi"}
chunk-AAYXMYGC.js?v=eeedd95f:10 [Violation] 'message' handler took 187ms
chunk-AAYXMYGC.js?v=eeedd95f:10 [Violation] 'message' handler took 186ms
chunk-AAYXMYGC.js?v=eeedd95f:10 [Violation] 'message' handler took 191ms
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
ws-manager.ts:303 Uncaught TypeError: this.clearPendingPiMessageFlushTimer is not a function
    at Object.current (ws-manager.ts:303:10)
    at usePiChatV2.ts:558:20
    at Object.react_stack_bottom_frame (react-dom_client.js?v=eeedd95f:18305:13)
    at runWithFiberInDEV (react-dom_client.js?v=eeedd95f:729:72)
    at commitHookEffectListUnmount (react-dom_client.js?v=eeedd95f:9181:160)
    at commitHookPassiveUnmountEffects (react-dom_client.js?v=eeedd95f:9200:60)
    at commitPassiveUnmountEffectsInsideOfDeletedTree_begin (react-dom_client.js?v=eeedd95f:11325:15)
    at recursivelyTraversePassiveUnmountEffects (react-dom_client.js?v=eeedd95f:11197:15)
    at commitPassiveUnmountOnFiber (react-dom_client.js?v=eeedd95f:11248:13)
    at recursivelyTraversePassiveUnmountEffects (react-dom_client.js?v=eeedd95f:11213:13)
(anonymous) @ ws-manager.ts:303
(anonymous) @ usePiChatV2.ts:558
react_stack_bottom_frame @ react-dom_client.js?v=eeedd95f:18305
runWithFiberInDEV @ react-dom_client.js?v=eeedd95f:729
commitHookEffectListUnmount @ react-dom_client.js?v=eeedd95f:9181
commitHookPassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:9200
commitPassiveUnmountEffectsInsideOfDeletedTree_begin @ react-dom_client.js?v=eeedd95f:11325
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11197
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11221
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11248
recursivelyTraversePassiveUnmountEffects @ react-dom_client.js?v=eeedd95f:11213
commitPassiveUnmountOnFiber @ react-dom_client.js?v=eeedd95f:11230
flushPassiveEffects @ react-dom_client.js?v=eeedd95f:12878
flushPendingEffects @ react-dom_client.js?v=eeedd95f:12820
flushSpawnedWork @ react-dom_client.js?v=eeedd95f:12794
commitRoot @ react-dom_client.js?v=eeedd95f:12536
commitRootWhenReady @ react-dom_client.js?v=eeedd95f:11748
performWorkOnRoot @ react-dom_client.js?v=eeedd95f:11682
performSyncWorkOnRoot @ react-dom_client.js?v=eeedd95f:13249
flushSyncWorkAcrossRoots_impl @ react-dom_client.js?v=eeedd95f:13146
processRootScheduleInMicrotask @ react-dom_client.js?v=eeedd95f:13169
(anonymous) @ react-dom_client.js?v=eeedd95f:13263
installHook.js:1 An error occurred in the <MainChatPiView> component.

Consider adding an error boundary to your tree to customize error handling behavior.
Visit https://react.dev/link/error-boundaries to learn more about error boundaries.

overrideMethod @ installHook.js:1
defaultOnUncaughtError @ react-dom_client.js?v=eeedd95f:6698
logUncaughtError @ react-dom_client.js?v=eeedd95f:6752
runWithFiberInDEV @ react-dom_client.js?v=eeedd95f:729
lane.callback @ react-dom_client.js?v=eeedd95f:6780
callCallback @ react-dom_client.js?v=eeedd95f:5223
commitCallbacks @ react-dom_client.js?v=eeedd95f:5235
runWithFiberInDEV @ react-dom_client.js?v=eeedd95f:731
commitLayoutEffectOnFiber @ react-dom_client.js?v=eeedd95f:9708
flushLayoutEffects @ react-dom_client.js?v=eeedd95f:12656
commitRoot @ react-dom_client.js?v=eeedd95f:12535
commitRootWhenReady @ react-dom_client.js?v=eeedd95f:11748
performWorkOnRoot @ react-dom_client.js?v=eeedd95f:11682
performSyncWorkOnRoot @ react-dom_client.js?v=eeedd95f:13249
flushSyncWorkAcrossRoots_impl @ react-dom_client.js?v=eeedd95f:13146
processRootScheduleInMicrotask @ react-dom_client.js?v=eeedd95f:13169
(anonymous) @ react-dom_client.js?v=eeedd95f:13263
react-dom_client.js?v=eeedd95f:13259 [Violation] 'click' handler took 294ms

## assistant
_at 2026-02-02T10:45:31.000Z_

Fixed the crash: removed the stale `clearPendingPiMessageFlushTimer` call in `frontend/lib/ws-manager.ts` and rebuilt.

Please reload and try again. If the terminal still shows a WS 404, paste the exact WS URL line again so I can verify it’s now using `/api/workspace/term`.

## user
_at 2026-02-02T10:46:55.000Z_

I still dont get any responses. The "working" indication also does not appear

## assistant
_at 2026-02-02T10:57:51.000Z_

Made Pi session creation idempotent in the runner and restored “queue until session_created” in the WS manager so prompts aren’t sent before the session is actually ready. This should unblock prompts that were being queued forever when the session already existed, and prevent missing responses on reconnect. Documented the change.

Files updated:
- `backend/crates/octo/src/bin/octo-runner.rs`
- `frontend/lib/ws-manager.ts`
- `docs/changes.md`

Tests:
- `cargo check` in `backend/` (passes; existing warnings from ts-rs/unused/unsafe noted)

Notes on warnings:
- The `cargo check` output includes long-standing ts-rs serde-attribute warnings and unused/unsafe warnings; no new errors.

Suggested next steps:
1) Restart or let Vite HMR refresh, then send a message again.
2) Verify backend sees the prompt:
   - `tmux capture-pane -pt 0:1 -S -300 | rg -i "Received WS command: Pi\\((CreateSession|Prompt)"`

If you want me to keep digging live, tell me which window has the backend logs (or I’ll keep using `0:1`).

## user
_at 2026-02-02T11:00:39.000Z_

client:495 [vite] connecting...
client:618 [vite] connected.
use-auth.ts:22 [auth] fetchCurrentUser http://archlinux:8080/api/me
use-auth.ts:36 [auth] /me response: 200
use-auth.ts:55 [auth] /me user id: wismut
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
ws-manager.ts:251 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: Object isConnected: false
ws-manager.ts:282 [ws-mux] Not connected, queueing subscription for: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:124 [ws-mux] Connecting to WebSocket...
ws-manager.ts:432 [ws-mux] Connecting to ws://archlinux:8080/api/ws/mux?token=eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ3aXNtdXQiLCJpc3MiOiJ3b3Jrc3BhY2UtYmFja2VuZCIsImF1ZCI6bnVsbCwiZXhwIjoxNzcwMTEwMDA4LCJpYXQiOjE3NzAwMjM2MDgsIm5iZiI6bnVsbCwianRpIjpudWxsLCJlbWFpbCI6ImRldkBsb2NhbGhvc3QiLCJuYW1lIjoiRGV2ZWxvcGVyIiwicHJlZmVycmVkX3VzZXJuYW1lIjoid2lzbXV0Iiwicm9sZXMiOlsiYWRtaW4iXSwicm9sZSI6ImFkbWluIn0.VfH9tvC5M14di4OqIl6p_TuNh9faI3o9IfIr2BjaHB4
ws-manager.ts:120 [ws-mux] Connection already in progress
ws-manager.ts:251 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: Object isConnected: false
ws-manager.ts:291 [ws-mux] Already subscribed to session: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:120 [ws-mux] Connection already in progress
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
installHook.js:1 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: Object isConnected: false
installHook.js:1 [ws-mux] Already subscribed to session: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:120 [ws-mux] Connection already in progress
installHook.js:1 [ws-mux] subscribePiSession: dfde9584-841a-40ea-a733-9e035a78d5a2 config: Object isConnected: false
installHook.js:1 [ws-mux] Already subscribed to session: dfde9584-841a-40ea-a733-9e035a78d5a2
ws-manager.ts:120 [ws-mux] Connection already in progress
ws-manager.ts:437 [ws-mux] Connected!
ws-manager.ts:442 [ws-mux] Processing pending subscriptions: 1
ws-manager.ts:443 [ws-mux] Processing subscribed sessions: 1
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"dfde9584-841a-40ea-a733-9e035a78d5a2","config":{"cwd":"/home/wismut/byteowlz/octo"}}
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"dfde9584-841a-40ea-a733-9e035a78d5a2"}
[Violation] 'message' handler took <N>ms
[Violation] 'message' handler took <N>ms
[Violation] 'message' handler took <N>ms
[Violation] 'message' handler took <N>ms
[Violation] 'message' handler took <N>ms
[Violation] 'message' handler took <N>ms
[Violation] 'message' handler took <N>ms
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"unsubscribe","session_id":"dfde9584-841a-40ea-a733-9e035a78d5a2"}
ws-manager.ts:251 [ws-mux] subscribePiSession: pending-1770029992775-0r8xo1h2 config: {cwd: '/home/wismut/byteowlz/hstry'} isConnected: true
ws-manager.ts:266 [ws-mux] Sending create_session + subscribe for: pending-1770029992775-0r8xo1h2
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"pending-1770029992775-0r8xo1h2","config":{"cwd":"/home/wismut/byteowlz/hstry"}}
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"pending-1770029992775-0r8xo1h2"}
ws-manager.ts:115 [ws-mux] Already connected
ws-manager.ts:251 [ws-mux] subscribePiSession: pending-1770029992775-0r8xo1h2 config: {cwd: '/home/wismut/byteowlz/hstry'} isConnected: true
ws-manager.ts:291 [ws-mux] Already subscribed to session: pending-1770029992775-0r8xo1h2
ws-manager.ts:115 [ws-mux] Already connected
SessionScreen.tsx:854 [Models] No opencode URL, clearing model options
react-dom_client.js?v=eeedd95f:13259 [Violation] 'click' handler took 301ms
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"unsubscribe","session_id":"pending-1770029992775-0r8xo1h2"}
ws-manager.ts:251 [ws-mux] subscribePiSession: 4db5daec-75d0-4c8c-a1cf-5dde3f0e6035 config: {cwd: '/home/wismut/byteowlz/hstry'} isConnected: true
ws-manager.ts:266 [ws-mux] Sending create_session + subscribe for: 4db5daec-75d0-4c8c-a1cf-5dde3f0e6035
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"create_session","session_id":"4db5daec-75d0-4c8c-a1cf-5dde3f0e6035","config":{"cwd":"/home/wismut/byteowlz/hstry"}}
ws-manager.ts:154 [ws-mux] Sending: {"channel":"pi","type":"subscribe","session_id":"4db5daec-75d0-4c8c-a1cf-5dde3f0e6035"}
ws-manager.ts:115 [ws-mux] Already connected
ws-manager.ts:251 [ws-mux] subscribePiSession: 4db5daec-75d0-4c8c-a1cf-5dde3f0e6035 config: {cwd: '/home/wismut/byteowlz/hstry'} isConnected: true
ws-manager.ts:291 [ws-mux] Already subscribed to session: 4db5daec-75d0-4c8c-a1cf-5dde3f0e6035
ws-manager.ts:115 [ws-mux] Already connected
tauri-fetch-polyfill.ts:105  GET http://archlinux:8080/api/workspace/files/tree?path=.&workspace_path=%2Fhome%2Fwismut%2Fbyteowlz%2Fhstry 502 (Bad Gateway)
fetchWithAuth @ tauri-fetch-polyfill.ts:105
fetchFileTree @ FileTreeView.tsx:87
(anonymous) @ FileTreeView.tsx:450
(anonymous) @ FileTreeView.tsx:481
react_stack_bottom_frame @ react-dom_client.js?v=eeedd95f:18299
runWithFiberInDEV @ react-dom_client.js?v=eeedd95f:729
commitHookEffectListMount @ react-dom_client.js?v=eeedd95f:9143
commitHookPassiveMountEffects @ react-dom_client.js?v=eeedd95f:9197
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10772
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10765
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10933
recursivelyTraversePassiveMountEffects @ react-dom_client.js?v=eeedd95f:10742
commitPassiveMountOnFiber @ react-dom_client.js?v=eeedd95f:10798
flushPassiveEffects @ react-dom_client.js?v=eeedd95f:12882
flushPendingEffects @ react-dom_client.js?v=eeedd95f:12820
flushSpawnedWork @ react-dom_client.js?v=eeedd95f:12794
commitRoot @ react-dom_client.js?v=eeedd95f:12536
commitRootWhenReady @ react-dom_client.js?v=eeedd95f:11748
performWorkOnRoot @ react-dom_client.js?v=eeedd95f:11682
performSyncWorkOnRoot @ react-dom_client.js?v=eeedd95f:13249
flushSyncWorkAcrossRoots_impl @ react-dom_client.js?v=eeedd95f:13146
processRootScheduleInMicrotask @ react-dom_client.js?v=eeedd95f:13169
(anonymous) @ react-dom_client.js?v=eeedd95f:13263
2026-02-02T10:59:54.701490Z  WARN request{method=GET uri=/workspace/files/tree?path=.&workspace_path=%2Fhome%2Fwismut%2Fbyteowlz%2Fhstry version=HTTP/1.1}: Proxy request body size: 0 bytes
2026-02-02T10:59:54.701504Z  WARN request{method=GET uri=/workspace/files/tree?path=.&workspace_path=%2Fhome%2Fwismut%2Fbyteowlz%2Fhstry version=HTTP/1.1}: Setting Content-Length to 0 (was: None)
2026-02-02T10:59:54.702530Z ERROR request{method=GET uri=/workspace/files/tree?path=.&workspace_path=%2Fhome%2Fwismut%2Fbyteowlz%2Fhstry version=HTTP/1.1}: Proxy request failed: hyper_util::client::legacy::E
rror(Connect, ConnectError("tcp connect error", 127.0.0.1:41899, Os { code: 111, kind: ConnectionRefused, message: "Connection refused" }))
2026-02-02T10:59:54.702563Z  INFO request{method=GET uri=/workspace/files/tree?path=.&workspace_path=%2Fhome%2Fwismut%2Fbyteowlz%2Fhstry version=HTTP/1.1}: finished processing request latency=1598 ms status=
502
2026-02-02T10:59:54.702571Z ERROR request{method=GET uri=/workspace/files/tree?path=.&workspace_path=%2Fhome%2Fwismut%2Fbyteowlz%2Fhstry version=HTTP/1.1}: response failed classification=Status code: 502 Bad
 Gateway latency=1598 ms
2026-02-02T10:59:55.198763Z  INFO request{method=OPTIONS uri=/sessions version=HTTP/1.1}: started processing request
2026-02-02T10:59:55.198796Z  INFO request{method=OPTIONS uri=/sessions version=HTTP/1.1}: finished processing request latency=0 ms status=200
2026-02-02T10:59:55.198862Z  INFO request{method=OPTIONS uri=/projects version=HTTP/1.1}: started processing request
2026-02-02T10:59:55.198882Z  INFO request{method=OPTIONS uri=/projects version=HTTP/1.1}: finished processing request latency=0 ms status=200
2026-02-02T10:59:55.202022Z  INFO request{method=GET uri=/sessions version=HTTP/1.1}: started processing request
 No responses, no "working" indication still!

## user
_at 2026-02-02T11:05:26.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## user
_at 2026-02-02T11:05:43.000Z_

Why dont we remove the old endpoints completely?

## assistant
_at 2026-02-02T11:05:49.000Z_

I’m on board, but we should be explicit about what “old” means here. Do you want to fully remove:

1) legacy WebSocket endpoints (`/ws`, `/api/ws`) and keep only `/api/ws/mux`
2) legacy Pi HTTP endpoints (`/pi/workspace/*`, `/main/pi/*`)
3) legacy terminal/file proxies (`/workspace/*` without `/api`)
4) all of the above in one sweep

If you say the scope, I’ll rip them out, update client routing, and add a short migration note in `docs/changes.md`.

## user
_at 2026-02-02T11:06:25.000Z_

yes, everything should exclusively go through the one websocket from now on, right?

## assistant
_at 2026-02-02T11:08:10.000Z_

Short answer: yes, that’s the intended architecture — one mux WS for Pi + ttyd + hstry + mmry + octo-files — but we’re not there yet. Right now:

- mux only fully implements **Pi**; **files/terminal/hstry are still placeholders** in `backend/crates/octo/src/api/ws_multiplexed.rs`.
- the frontend still uses **legacy HTTP/WS endpoints** for files/terminal and session events.
- if we remove legacy endpoints today, **terminal + file tree + history will break**.

I can take it the rest of the way, but it’s a real migration, not a delete‑only change. Proposed steps:

1) **Implement mux channels**:
   - files: map `files.read/write/list/stat` to Runner file ops
   - hstry: expose query over mux (currently a placeholder)
   - terminal: mux‑proxy ttyd with JSON/base64 frames (new protocol), update frontend TerminalView to use it

2) **Switch frontend** to mux for everything, remove ws-client and `/api/*` polling.

3) **Delete legacy routes** (`/ws`, `/api/ws`, `/main/pi/*`, `/pi/workspace/*`, `/workspace/*`), and update auth allowlist.

If you want me to start this migration now, say the word and I’ll proceed.

## user
_at 2026-02-02T11:09:06.000Z_

yes, unless there are arguments against it. Backwards compatibility is not required

## user
_at 2026-02-02T11:10:01.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## user
_at 2026-02-02T11:10:15.000Z_

Think about how a 10x engineer would build this

## assistant
_at 2026-02-02T11:10:27.000Z_

A 10x way to do this is: **define the mux protocol as the only API, implement it end‑to‑end, then delete everything else in one cut.** No half‑migrations.

Here’s how I’d build it:

**1) Lock the mux contract first**
- Define concrete, minimal message schemas for:
  - `terminal`: `open`, `close`, `input`, `resize`, `output`, `exit`
  - `files`: `read`, `write`, `list`, `stat`, `delete`, `mkdir`
  - `hstry`: `query`, `results`
  - `system`: `error`, `connected`, `ping`
- Make these types the single source of truth (TS + Rust share the same shape).

**2) Implement mux server side in one pass**
- **Terminal**: proxy ttyd *through mux* (binary frames or base64 data in JSON) and keep one ttyd connection per `terminal_id`.
- **Files**: implement via Runner file ops with strict path validation; return structured errors.
- **Hstry**: run queries in backend and stream results if large.
- Add backpressure and session cleanup logic (idle timeout, close on disconnect).

**3) Update frontend to use mux only**
- Replace `ws-client` with `ws-manager` everywhere.
- File tree + file previews use mux `files.*` calls.
- Terminal uses mux `terminal.*` channel; remove direct ws URL.
- Session events (UI updates, busy/idle) come from mux.

**4) Remove old endpoints in one shot**
- Delete `/api/ws`, `/api/main/pi/*`, `/api/pi/workspace/*`, `/api/workspace/*`, legacy ws‑client, and related auth allowlists.

**5) Validate**
- Add a small mux integration test (Pi prompt + file list + terminal echo).
- Smoke test UI flows.

If you want, I’ll start implementing this now with the mux contract and terminal/files/hstry channel plumbing, then flip the frontend and delete the old routes.

## user
_at 2026-02-02T11:11:13.000Z_

yes, but we also want copy, move, rename for files

## user
_at 2026-02-02T11:20:54.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## user
_at 2026-02-02T11:21:27.000Z_

continue

## user
_at 2026-02-02T11:30:04.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## user
_at 2026-02-02T11:30:07.000Z_

continue

## user
_at 2026-02-02T11:37:09.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## user
_at 2026-02-02T11:37:21.000Z_

oh and dont forget trx integration!

## user
_at 2026-02-02T11:37:45.000Z_

<turn_aborted>
The user interrupted the previous turn on purpose. If any tools/commands were aborted, they may have partially executed; verify current state before retrying.
</turn_aborted>

## user
_at 2026-02-02T11:38:16.000Z_

trx is the issue tracker we have in octo. this and the todo lists need to work under the multiplexed websocket as well of course!
