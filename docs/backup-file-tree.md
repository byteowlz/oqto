# Oqto Backup File Tree

> Fiat tree of every file and directory that must be captured by the per-user
> and platform-wide backup system.  Paths are written from the perspective of a
> single Linux host; for multi-host or container deployments the same tree
> applies inside every data-bearing node.

---

## Legend

| Marker | Meaning |
|--------|---------|
| `[P]` | **Platform scope** -- backed up once per host / cluster |
| `[U]` | **User scope** -- backed up per Linux user (everyone who has run `oqto` or owns a workspace) |
| `[W]` | **Workspace scope** -- may be shared across users; back up once, restore with ACL awareness |
| `🔐` | Contains secrets; must be encrypted at rest and in transit with envelope encryption |
| `⚡` | Hot file -- database WAL/SHM; use `sqlite3 .backup` or quiesce before snapshot |
| `📦` | Large / high-churn; candidate for incremental-only or separate object-store stream |

---

## I. User-scoped data (`[U]`)

These directories live inside each user's `$HOME`.  In **single-user mode** there is
only one user (e.g. `wismut`).  In **multi-user mode** there are `oqto_` prefixed
Linux users + the shared workspace owner.

### 1.1 Oqto application state

```
~/.config/oqto/                                        [U]  user config + secrets
├── config.toml                                        [U]  backend URL, mode, timeouts, features
├── config.toml.backup.<timestamp>                     [U]  auto-backups from setup.sh (safe to skip)
├── sandbox.toml                                       [U]  sandbox allow/deny lists
├── skdlr-agent.toml                                   [U]  skdlr agent config
├── hstry.deploy.toml                                  [U]  hstry deployment overrides
└── setup-state.env                              🔐    [U]  re-runnable setup decisions + JWT_SECRET + EAVS_MASTER_KEY

~/.local/share/oqto/                                   [U]  runtime data
├── oqto.db                                      ⚡    [U]  SQLite: users, workspaces, sessions metadata
├── oqto.db-wal                                    [U]  WAL (co-backup with oqto.db via .backup)
├── oqto.db-shm                                    [U]  SHM (ephemeral, safe to skip if .backup used)
├── oqto-templates/                                    [U]  cloned template repo (skills, agent dotfiles)
│   ├── .git/
│   ├── .trx/
│   │   ├── config.toml
│   │   └── issues.jsonl
│   ├── agents/
│   │   └── <role>/
│   │       ├── AGENTS.md
│   │       └── ...
│   ├── skills/
│   │   └── <skill-name>/
│   │       └── SKILL.md
│   └── dotfiles/
│       ├── .zshrc
│       └── ...
└── oqto-log/                                          [U]  one DB per workspace hash
    └── <workspace_hash_16>/
        ├── oqto-log.sqlite                      ⚡    [W]  authoritative timeline (canonical protocol)
        ├── oqto-log.sqlite-wal                        [U]  WAL
        └── oqto-log.sqlite-shm                        [U]  SHM (ephemeral)
```

### 1.2 Pi harness state

```
~/.pi/agent/                                           [U]  Pi's own state -- Oqto must NEVER write here
├── AGENTS.md                                          [U]  personal agent instructions
├── auth.json                                    🔐    [U]  Pi auth state
├── auto-rename.json                                   [U]  session auto-naming config
├── extensions/                                        [U]  Pi extensions (code, manifest, assets)
│   └── <ext-id>/
│       ├── extension.json
│       └── ...
├── models.json                                        [U]  Pi model manifest (regenerated from eavs)
├── mcp.json                                           [U]  MCP server definitions
├── mcp-cache.json                                     [U]  MCP tool cache
├── prompts/                                           [U]  prompt templates
├── sessions/                                    📦    [U]  **Pi session JSONL files -- harness authority**
│   └── --<safe_cwd>--/
│       └── <timestamp>_<session_id>.jsonl
├── settings.json                                      [U]  Pi UI settings
├── skills/                                            [U]  Pi skill definitions
│   └── <skill-name>/
│       └── SKILL.md
├── run-history.jsonl                                  [U]  Pi run history
└── oauth.json.migrated                                [U]  legacy OAuth migration marker
```

### 1.3 Hstry (legacy / interop)

```
~/.local/share/hstry/                                  [U]  chat history (until full oqto-log cutover)
├── hstry.db                                     ⚡    [U]  SQLite: conversations, messages, branches
├── hstry.db-wal                                       [U]  WAL
├── hstry.db-shm                                       [U]  SHM
├── hstry.db.bak                                       [U]  manual backup (safe to skip)
└── index/
    └── tantivy/
        ├── meta.json
        ├── .managed.json
        └── ...
```

### 1.4 EAVS (per-user keys & model catalog)

```
~/.config/eavs/                                        [U]  EAVS user config
├── config.toml                                        [U]  provider endpoints, analysis settings
└── env                                            🔐    [U]  EAVS_MASTER_KEY (virtual key root secret)

~/.local/share/eavs/                                   [U]  EAVS runtime state
├── keys.db                                      ⚡    [U]  SQLite: virtual keys, quotas, ACLs
├── providers.db                                 ⚡    [U]  SQLite: provider configs & OAuth tokens
└── models_catalog.json                                [U]  cached provider model list
```

### 1.5 Mmry (semantic memory)

```
~/.local/share/mmry/                                   [U]  mmry vector store + episodic memories
└── ... (per-mmry internal layout)
```

### 1.6 User systemd overrides

```
~/.config/systemd/user/                                [U]  user-level service overrides
├── eavs.service                                       [U]  EAVS user service override
├── hstry.service                                      [U]  hstry user service override
├── mmry.service                                       [U]  mmry user service override
├── oqto.service                                       [U]  oqto user service override
├── oqto-healthcheck.service                           [U]  healthcheck service override
├── oqto-healthcheck.timer                             [U]  healthcheck timer override
├── oqto-runner.service                                [U]  runner user service override
├── skdlr-pull-external-repos.service                  [U]  skdlr service override
└── skdlr-pull-external-repos.timer                    [U]  skdlr timer override
```

### 1.7 Oqto workspace metadata (`.oqto/` directories)

Every git repository that has been opened as an Oqto workspace contains a
`.oqto/` or `.octo/` directory at its root with per-workspace metadata.

```
<workspace_root>/                                      [W]  e.g. ~/byteowlz/oqto_refactor/
├── .oqto/                                             [W]  workspace-scoped Oqto metadata
│   ├── workspace.toml                                 [W]  workspace config (name, settings)
│   └── sandbox.toml                                   [W]  workspace-specific sandbox overrides
│
├── .trx/                                              [W]  trx issue tracking for this repo
│   ├── config.toml
│   └── issues.jsonl
│
└── ... (rest of the repo is normal git -- backed up by git remote)
```

---

## II. Platform-scoped data (`[P]`)

These paths are host-level or shared-across-users.  In multi-user部署 they are
owned by the `oqto` system user or managed by the deployment tooling.

### 2.1 System configuration

```
/etc/oqto/                                             [P]  system-wide config
├── config.toml                                        [P]  server bind, TLS, runtime mode
├── sandbox.toml                                       [P]  default sandbox profile
└── skdlr-agent.toml                                   [P]  skdlr system agent config
```

### 2.2 Transactional release directory

Created by `just deploy` / `scripts/deploy.sh`.  The `current` symlink is the
active release; previous releases allow instant rollback.

```
/var/lib/oqto/                                         [P]  deployment state root
├── current -> releases/<release-id>/                  [P]  symlink to active release
├── releases/                                          [P]  staged releases
│   └── <release-id>/                                  [P]  e.g. 20260508-063045-abc123
│       ├── bin/                                       [P]  compiled binaries
│       ├── frontend/                                  [P]  built frontend assets
│       └── ...
└── ...
```

### 2.3 Platform log & event stream

```
/var/log/oqto/                                         [P]  platform audit & lifecycle logs
└── update-events.jsonl                                [P]  structured deploy/rollback events
```

### 2.4 Systemd service definitions

```
/usr/lib/systemd/user/                                 [P]  user services (installed by `just install-system`)
├── eavs.service                                       [P]
├── hstry.service                                      [P]
├── mmry-embeddings.service                            [P]
├── mmry-embeddings.config.toml                        [P]
├── oqto-agent@.service                                [P]  templated per-user agent service
├── oqto-healthcheck.service                           [P]
├── oqto-healthcheck.timer                             [P]
├── oqto-runner.service                                [P]
├── oqto-runner.socket                                 [P]
├── oqto-runner.tmpfiles.conf                          [P]
├── oqto-user.service                                  [P]
└── oqto.service                                       [P]
```

### 2.5 Shared workspace data (multi-user)

In multi-user mode, workspaces may be owned by a shared Linux user (e.g.
`oqto_workspaces`).  The `.oqto/` metadata then lives under that shared user's
home or under the shared directory root.

```
<shared_workspace_root>/                               [P]  configured in oqto config
└── <repo_name>/
    ├── .oqto/
    │   ├── workspace.toml
    │   └── sandbox.toml
    └── ...
```

### 2.6 tmpfiles.d socket directory

Created via `oqto-runner.tmpfiles.conf`; ensures runner sockets exist with
correct permissions at boot.

```
/run/user/<uid>/                                       [P]  XDG_RUNTIME_DIR
└── oqtoctl.sock                                       [P]  admin control socket (recreated at boot)
```

---

## III. What NOT to back up

| Path | Reason |
|------|--------|
| `~/.cache/oqto/` | Rebuildable cache (template clones, Vite cache) |
| `~/.pi/agent/.oqto/` | Internal Pi runtime marker (empty or ephemeral) |
| `~/.pi/agent/pi-crash.log` | Diagnostic only; grows unbounded |
| `~/.local/share/oqto/oqto.db-shm` / `*-wal` | WAL/SHM are only meaningful if the main DB file is copied raw; use `sqlite3 .backup` instead |
| `/tmp/*` | Temp files |
| Build artifacts (`target/`, `node_modules/`, `.vite/`) | Rebuildable from source |

---

## IV. Backup grouping (restore-unit cohesion)

For restore operations you want to recover coherent "slices" of state:

| Restore unit | Files |
|-------------|-------|
| **Single user** | `~/.config/oqto/` + `~/.local/share/oqto/` + `~/.local/share/hstry/` + `~/.local/share/eavs/` + `~/.local/share/mmry/` + `~/.pi/agent/` |
| **Single workspace** | `~/.local/share/oqto/oqto-log/<hash>/` + workspace root `.oqto/` + `.trx/` |
| **Single session** | `~/.pi/agent/sessions/--<safe_cwd>--/<timestamp>_<id>.jsonl` + corresponding `oqto-log` rows |
| **Platform state** | `/etc/oqto/` + `/var/lib/oqto/` + `/var/log/oqto/` + systemd units |
| **Full host DR** | platform + all users + all workspaces |

---

## V. Notes for implementation

1. **SQLite consistency**
   - For every sqlite database, use `sqlite3 <db> ".backup to /tmp/<db>.bak"`
     rather than copying the raw file.  This produces a clean snapshot without
     locking issues and eliminates the need to carry WAL/SHM files.

2. **WAL mode**
   - oqto-log, hstry, eavs keys.db, and eavs providers.db all run with WAL enabled.
   - `.backup` already merges the WAL into the snapshot, so WAL files can be
     skipped in the backup stream.

3. **`oqto.db` per-user vs system**
   - In single-user mode `oqto.db` lives under `~/.local/share/oqto/`
   - In multi-user mode the system oqto DB may live under a shared location
     (TBD -- verify against actual `config.toml` `server.db_path`).

4. **`oqto-log` workspace hash**
   - The `<workspace_hash_16>` directory names come from `oqto-log`'s internal
     hashing.  The backup tool does not need to interpret them, but a restore
     tool may need to map workspace paths -> hashes to offer per-workspace
     restore.

5. **Pi session JSONL**
   - These are **harness-authoritative** and Oqto never writes them.  They must
     be restored alongside the corresponding `oqto-log` entries or session IDs
     will drift.

6. **Secrets**
   - `setup-state.env`, `~/.config/eavs/env`, `~/.pi/agent/auth.json` contain
     credentials.  Envelope-encrypt these with a platform KMS key or per-user
     age key before uploading to object storage.

7. **Cross-host restores**
   - On restore to a new host, `setup-state.env` paths (`WORKSPACE_DIR`, etc.)
     may need rewriting if the new host has a different filesystem layout.

---

*Generated from host filesystem scan on 2026-05-10.*
*Tracked in trx: oqto-n181*
