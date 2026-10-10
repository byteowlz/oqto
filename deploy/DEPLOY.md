# Oqto Deployment

One complete, verified release per host, activated by `oqto-setup install` inside a
single root transaction with an in-transaction service check and automatic rollback.
Design: [ADR-0018](../docs/adr/0018-single-install-update-deploy-engine.md),
[ADR-0056](../docs/adr/0056-deploy-is-one-root-transaction-and-history-backfill-is-separate.md).

## Quick Start

```bash
just deploy                                  # Build a clean release, deploy to all hosts
just deploy-host archvm                      # One host
just deploy --artifact oqto-0.5.9-x86_64-unknown-linux-gnu.tar.gz \
            --checksum oqto-0.5.9-x86_64-unknown-linux-gnu.tar.gz.sha256
just deploy --canary-then-fleet              # Canary hosts first, then the rest
just deploy --status                         # current / last-good per host
just deploy-dry-run                          # Print the plan, change nothing
just deploy-backfill octo-azure              # Protected oqto-log backfill only
```

## What a deploy refuses

| Refusal | Where | Override |
|---|---|---|
| Building from a worktree with uncommitted or untracked changes | `deploy.sh`, `package.sh` | commit/stash, or deploy a prebuilt `--artifact` |
| Cargo, dependency pin and frontend versions disagree | `deploy.sh` (`check-tag-version.py`) | `just bump` |
| Artifact name/manifest version mismatch, missing provenance | `oqto-setup install` | none |
| Release recorded as built from a dirty tree | `oqto-setup install` | `--allow-dirty-source` |
| Release not strictly newer than the active one (or active version unknowable) | `oqto-setup install`, before extraction | `--allow-downgrade` |
| Agent sessions currently streaming/compacting/starting/aborting | transaction inventory | `--allow-busy-sessions` |
| Managed tools older than their `dependencies.toml` pin (release mode) | preflight | acquire them, or `--mode dev` |
| `sandbox.toml` not `root:root` ≤0644; seccomp enforced without an installed BPF (team) | preflight | fix the host |

Removed for safety: `--skip-build`, `--skip-frontend`, `--skip-backend`, `--skip-services`,
`--prepare-only`, `--activate-only`, `--resume`, `--allow-legacy-path`, `--remote-build`,
`--trace-streams`, `scripts/deploy-binaries.sh`. Partial deploys are how mismatched
binaries, frontends and runtimes reached `current`.

## Release build

Without `--artifact`, deploy requires a clean checkout and runs
`dist-sync → dist-stage-binaries --build → dist-stage-frontend --build →
lint-dist-manifest-strict → dist-package <Cargo version> <host triple>`, then re-checks
that the build did not dirty the tree. `package.sh` records `release.version`,
`source_commit` and `source_dirty` in the bundled `manifest.toml`; `oqto-setup
check-release` prints them.

## Per-host transaction

1. **Preflight** (driver): SSH, free disk under `/var/lib`, team sandbox policy,
   dependency policy (`--mode release` enforces `installed >= pin` and never downgrades
   newer tools; `--mode dev` only requires presence).
2. **Private staging**: artifact, checksum, `deploy-transaction.py` and the verified
   bootstrap are copied into a per-host `mktemp` 0700 directory.
3. **Verified bootstrap**: `verified-setup-bootstrap.py` checks the archive against the
   independent checksum and extracts only `oqto-setup`; nothing from `PATH` is trusted.
4. **`deploy-transaction.py activate`** (root, receipts under `/root/oqto-deploy-*`):
   - inventories principals with a live managed runner socket (team: every
     `/run/oqto/runner-sockets/<user>/oqto-runner.sock` whose owner matches; personal:
     the host's `user`), refuses busy sessions, records `before.json`;
   - pauses the `oqto-healthcheck` watchdog timer (resumed at the end if it was
     running), stops services and refuses if a backend or runner comes back;
   - byte-snapshots only platform state: top-level `~/.local/share/oqto/*.db*`
     (`oqto.db` + WAL/SHM), `~/.config/oqto` and Pi `models/settings/auth.json`.
     oqto-log history, workspaces and runtimes are not copied (activation never
     touches them; the backfill transaction protects history);
   - runs `oqto-setup install --post-activate-check <check>`. The check restarts
     usermgr → runners → control plane and requires backend health **at the new
     version**, every runner's `MainPID` executing the new release's `oqto-runner`,
     the bundled Pi reporting the manifest `pi_version`, and (personal) a strict
     doctor. Only then does `last-good` advance;
   - on failure `oqto-setup` restores `current`, binaries and frontend, and the
     transaction restarts the previous services and writes `failure.json`;
   - points a managed `/usr/local/bin/pi` at `releases/current/immutable/pi-runtime`;
     unmanaged entrypoints are preserved and reported.
5. **Extensions** (unless `--skip-extensions`): the pinned extension set is synced
   preserve-first; changed or retired managed extensions move to
   `~/.pi/agent/extension-backups/<stamp>/`. The Pi runtime itself is never touched here.

The frontend ships inside the artifact; `oqto-setup` links `/var/www/oqto` to
`releases/current/immutable/frontend`, so rollback serves the matching bundle. Hosts
must use that web root.

## History backfill (never part of activation)

```bash
just deploy-backfill <host>                                  # sync-identities,bootstrap,validate
just deploy --host <host> --backfill-only --backfill-modes sync-identities,validate-changed
just deploy --host <host> --backfill-only --backfill-principal oqto_usr_alice
just deploy --host <host> --backfill                         # after a successful activation
```

`deploy-transaction.py backfill` processes one principal at a time: refuse
foreign-owned or symlinked oqto-log entries → stop that principal's runner (personal:
also the backend, which reads the store) → refuse if writers linger → snapshot the
store → run the modes **as the owner** → fail if any Pi JSONL changed → on failure
retain the failed store under the receipt and restore the exact snapshot → restart.
Only `sync-identities`, `bootstrap`, `validate` and `validate-changed` are allowed.
Receipts: `/root/oqto-backfill-*/backfill.json`.

## Host configuration

```toml
[[host]]
name = "octo-azure"
ssh = "octo-azure"
mode = "multi-user"      # personal: "single-user" (or "local"); team: "multi-user"
canary = true
user = "tommy"           # personal hosts: the owning, non-root principal
frontend = true
web_root = "/var/www/oqto"
```

`binaries`/`services` are accepted for compatibility but ignored: the release decides
what is installed, and the transaction decides what restarts.

## Emergency rollback

Releases are retained (`--keep-releases`, default 3, plus `current`/`last-good`).
Reinstall the previous artifact with `--allow-downgrade`, which keeps provenance,
service checks and receipts. Manual pointer edits bypass all of that; only use them
when the transaction itself cannot run:

```bash
sudo ln -sfn "$(readlink -f /var/lib/oqto/releases/last-good)" /var/lib/oqto/releases/current
```
