# Oqto Deployment

Transactional release deployment with strict preflight gates, canary support, and automatic rollback.

## Quick Start

```bash
just deploy                                 # Build + prepare + activate on all hosts
just deploy --canary                        # Deploy only hosts with canary=true
just deploy --canary-then-fleet             # Canary first, then remaining hosts
just deploy --prepare-only                  # Stage release, no activation
just deploy --activate-only --release-id X  # Activate a prepared release
just deploy --resume --release-id X         # Resume interrupted deployment
just deploy --status --release-id X         # Show per-host state
just deploy-dry-run                         # Preview commands
```

## Host Configuration

`deploy/hosts.toml` controls rollout targets.

```toml
[[host]]
name = "octo-azure"
ssh = "octo-azure"
mode = "multi-user"         # "single-user" (or legacy "local") | "multi-user"
canary = true                # optional; used by canary rollout
user = "tommy"
frontend = true
web_root = "/var/www/oqto"
binaries = ["oqto", "oqto-runner", "oqto-files", "oqto-sandbox", "oqto-usermgr"]
services = ["oqto"]
```

## Update Lifecycle

For each host, deploy executes these phases:

1. `preflight.start` / `preflight.pass|fail`
   - Mode validation (`single-user` or `multi-user`)
   - Disk space gate (`--min-free-mb`, default 1024 MB)
   - Required tooling gate (`systemctl`, `install`)
   - Dependency compatibility gates (from `dependencies.toml`):
     - `eavs`, `hstry`, `mmry`, `trx`, `agntz`, `sx`, `skdlr`
     - Host versions must satisfy `installed >= required`.
     - `hstry adapters --help` must succeed (adapter CLI compatibility guard).
     - If hstry DB exists, `conversations.parent_conversation_id` and `conversations.fork_type` must exist (session-tree schema guard).
   - Multi-user security gates:
     - `/etc/oqto/sandbox.toml` must exist, be readable, owner `root:root`, perms `<= 0644`
     - If seccomp enforce is configured in sandbox config, `/etc/oqto/seccomp/default.bpf` must exist

2. `deploy.prepare.start` / `deploy.prepare.pass|fail`
   - Stage binaries and frontend under: `/var/lib/oqto/releases/<release-id>/`
   - Write `.prepared` marker (idempotent)

3. `deploy.activate.start` / `deploy.activate.pass|fail`
   - Atomic symlink switch: `/var/lib/oqto/releases/current -> <release-id>`
   - `/usr/local/bin/*` relinked to `current/bin/*`
   - Ordered restarts: runner → control plane (`oqto`) → dependent services
   - Bounded health checks (`--health-timeout`, default 90s)

4. `rollback.start` / `rollback.pass|fail` (only on activation failure)
   - Restore previous `current` release
   - Relink binaries + restart services

5. `deploy.prune.pass|fail` (after successful activation or rollback)
   - Remove old release directories under `/var/lib/oqto/releases/`
   - Always preserves `current` and `last-good` symlink targets
   - Keeps the `--keep-releases` (default 3) newest directories on top of those
   - Failure is non-fatal (logged as warn)
   - Re-run health checks

## Audit Trail

Structured events are appended to:

- `/var/log/oqto/update-events.jsonl`

Each event contains:
- `timestamp`
- `release_id`
- `host`
- `actor`
- `phase`
- `result`
- `reason_code`

## Idempotency and Resume

- Re-running with same `--release-id` converges safely:
  - Prepared releases are skipped when `.prepared` exists
  - Already-active releases are skipped in `--resume` mode if health passes
- Use `--resume --release-id <id>` after interruption
- Use `--status --release-id <id>` to inspect `prepared/current/last-good`

## Canary + Fleet Rollout

### Canary only
```bash
just deploy --canary
```

### Canary then full rollout
```bash
just deploy --canary-then-fleet
```

Set `canary = true` on one or more hosts in `deploy/hosts.toml`.

## Operator Runbook

### Safe release flow

```bash
# 1) Build + stage + activate canary
just deploy --canary-then-fleet

# 2) Verify audit events and health
ssh <host> 'tail -n 50 /var/log/oqto/update-events.jsonl'
ssh <host> 'curl -sf http://127.0.0.1:8080/api/health && echo ok'

# 3) If interrupted, resume exactly same release
just deploy --resume --release-id <release-id>
```

### Manual rollback to last-good (emergency)

```bash
sudo ln -sfn "$(readlink -f /var/lib/oqto/releases/last-good)" /var/lib/oqto/releases/current
sudo systemctl restart oqto
```

## Options

```text
--host NAME
--release-id ID
--skip-build
--skip-frontend
--skip-backend
--skip-services
--prepare-only
--activate-only
--resume
--status
--canary
--canary-then-fleet
--health-timeout SEC
--min-free-mb MB
--keep-releases N     # default 3; also OQTO_KEEP_RELEASES env var
--dry-run
--config FILE
```
