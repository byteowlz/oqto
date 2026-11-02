# Oqto Deployment

One complete, verified release per host, activated by `oqto-setup install` inside a
single root transaction with an in-transaction service check and automatic rollback.
Source and artifact boundaries: [Build and deployment](../docs/reference/build-and-deploy.md).

## Quick Start

Create a private host configuration from `deploy/hosts.toml.example`, review it,
and pass `--config /path/to/hosts.toml`. Actual host inventories are not shipped
in the public repository. The commands below assume that configuration has been
provided; source checkouts with no `deploy/hosts.toml` must use `--config`.

```bash
just deploy                                  # Build a clean release, deploy to all hosts
just deploy-host dev-host                      # One host
just deploy --artifact oqto-0.5.9-x86_64-unknown-linux-gnu.tar.gz \
            --checksum oqto-0.5.9-x86_64-unknown-linux-gnu.tar.gz.sha256
just deploy --canary-then-fleet              # Canary hosts first, then the rest
just deploy --status                         # current / last-good per host
just deploy-dry-run                          # Print the plan, change nothing
just deploy-backfill team-host              # Protected oqto-log backfill only
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

Without `--artifact`, deploy requires clean source, captures one commit and
re-executes from a private independent checkout. Later commits, edits and branch
switches in the original checkout cannot change the running build. Host
configuration is copied separately from product source; caller-relative paths
keep their original meaning. Authentication happens in the frozen child, before
compilation or activation.

The build runs `dist-sync → dist-stage-pi-runtime → dist-stage-binaries --build →
dist-stage-frontend --build → lint-dist-manifest-strict → dist-package`.
Frontend installation uses the frozen lockfile; Rust compilation uses `--locked`.
Source, Cargo output, dependency checkouts and distribution staging are private to
the invocation. Developer `node_modules`, targets, sibling checkouts and ignored
build outputs are not copied. Download caches may still be shared by their tools.

Build records live under `$XDG_STATE_HOME/oqto/deploy-builds/` (default
`~/.local/state/oqto/deploy-builds/`). Each private run retains its log and
`artifacts/`; source and build scratch are removed after the child exits, including
failure or cancellation. Printed artifact paths replace the old assumption that
every deploy writes to the caller's `dist/out/`. Retained build records are not
automatically pruned.

`package.sh` records `release.version`, `source_commit` and `source_dirty` from the
frozen checkout. The existing provenance and post-build cleanliness checks remain
in force; `oqto-setup check-release` prints the result. Artifact, status, dry-run
and backfill-only invocations do not create source snapshots.

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
name = "team-host"
ssh = "team-host"
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

## Fast development deployments

`just deploy --dev --host dev-host` uses Cargo's `deploy-fast` profile (thin LTO,
16 codegen units, incremental) from a clean frozen source checkout. It retains
the normal frontend build, portable glibc floor, checksummed packaging, activation
transaction, health checks and rollback. Compile outputs are isolated per run;
no warm incremental-cache or measured build-time guarantee is claimed.

In the private working repository, before freezing source, `--dev` qualifies and
may commit the newest extension pin. Public snapshots must instead use
`--no-extension-update`: dependency changes belong in the working repository and
a new checked export, never a hand-edited public checkout. This preparation happens once in the caller checkout; `--no-extension-update`
keeps the existing pin. The resulting commit is the build's fixed source identity,
and no pin update runs in the frozen child.

Dirty and untracked source is now refused even with `--dev`, rather than silently
omitted from a committed snapshot. Commit the intended inputs or explicitly deploy
a previously produced artifact. `--allow-dirty-source` remains an installation
policy for an already selected artifact; it does not bypass source isolation.

Dev archives use `<Cargo version>-dev.<UTC timestamp>.g<sha8>`; no `just bump` is
required. Manifest provenance records the captured commit. Dev builds replace the
release of the same core version or older dev builds, never a newer core. The
ordinary release replaces its own dev builds. Same-second builds can be rejected;
retry later. Dev versions must not be tagged or publicly published.

Multi-user hosts require `--allow-dev-shared` as well. `--dev` cannot be combined
with `--artifact`. Use `--dry-run` to inspect without building or restarting.
Normal deployments still require clean source and a newer release version.
