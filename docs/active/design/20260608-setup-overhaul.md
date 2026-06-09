# Oqto setup overhaul: analysis and plan

Status: design proposal for `oqto-2q7r` / related provisioning work `oqto-3ct7.4`.

## Goal

Make Oqto installation boring, deterministic, and recoverable across the two real deployment shapes:

1. Personal/single-user local install.
2. Team/multi-user Linux install with isolated Linux users and per-user runners.

Container mode, macOS, and optional extras should not be presented as first-class install paths until they are fully tested.

## Current-state findings

### 1. The setup entrypoint is a long imperative shell pipeline

Evidence: `setup.sh` sources 22 numbered modules from `scripts/setup/`, then `scripts/setup/21-main.sh` runs a fixed sequence of prerequisite, tool, config, EAVS, build, isolation, service, admin, and start steps.

This gives us resumability via step files, but the model is still fragile:

- Step completion is partly state-file based and partly ad-hoc command probes.
- Some probes only check presence (`command -v`, `systemctl is-enabled`) instead of usable runtime state.
- Recovery is script-specific (`--redo`, `--fresh`) rather than a general reconcile loop.
- The actual desired system is not represented as one inspectable plan.

### 2. User-mode defaults and messaging conflict

Evidence:

- `scripts/setup/00-defaults.sh` defaults `OQTO_USER_MODE=multi`.
- `scripts/setup/21-main.sh --help` says `OQTO_USER_MODE ... (default: single)`.
- `SETUP.md` quick-start frames `./setup.sh` as development/local but step 2 prompts for single vs multi.

This makes first-run behavior surprising and increases the chance that users accidentally choose the harder multi-user path.

### 3. Container mode remains visible even though setup forces local

Evidence:

- Prior to this cleanup, `SETUP.md` described local/native and container modes as normal deployment choices.
- Prior to this cleanup, `scripts/setup/21-main.sh` showed a multi-user container production example.
- `scripts/setup/13-mode-selection.sh` and `21-main.sh` still force container to local with warnings.

This is a documentation/API contract mismatch. Unsupported modes should be hidden behind explicit experimental flags, not shown as normal setup choices.

### 4. Configuration, provisioning, and service installation are split across multiple authorities

Evidence:

- Shell generates `~/.config/oqto/config.toml` in `scripts/setup/14-config.sh`.
- Multi-user install copies runtime config into `~oqto/.config/oqto/` and `/etc/oqto/` in `scripts/setup/18-services.sh`.
- `backend/crates/oqtoctl/src/main.rs` still contains Linux user creation, runner setup, EAVS provisioning, admin bootstrap, and audit logic.
- `backend/crates/oqto-setup` currently only hydrates config files from TOML.

Because provisioning behavior is scattered, fixes for paths/permissions/services are easy to make in one path and miss another.

### 5. Multi-user runner socket paths are historically inconsistent

Evidence:

- Setup-generated multi-user config uses `/run/oqto/runner-sockets/{user}/oqto-runner.sock`.
- `oqtoctl user info/status` checks `/run/user/{uid}/oqto-runner.sock` in several places.
- `oqtoctl` audit logic explicitly detects both legacy `/run/user/{uid}/oqto-runner.sock` and shared `/run/oqto/runner-sockets/{user}/oqto-runner.sock` paths.

This directly matches the reported failure class: wrong paths and services that appear healthy from one command but are unreachable from another.

### 6. Multi-user permissions are hard to reason about

Evidence:

- `scripts/setup/15-user-isolation.sh` writes `/etc/sudoers.d/oqto-multiuser` with command aliases for useradd, chown, mkdir, chmod, loginctl, and systemctl.
- `scripts/setup/18-services.sh` creates `/run/oqto`, `/var/lib/oqto`, `~oqto/.config/oqto`, and copies configs between homes and `/etc`.
- `oqtoctl` separately creates Linux users, enables lingering, writes per-user EAVS env files, chmods them to `640`, and sets up runner services.

There is no single permission manifest that says which user/group owns each path, which mode it must have, and which component consumes it.

### 7. Health checks do not cover the install contract

Evidence:

- The setup flow verifies individual steps, but there is no final contract check for: backend process, admin socket, EAVS virtual key generation, Pi models/settings for each user, runner service active, runner socket reachable at the configured pattern, sandbox config readable by runner, and workspace path writable by the intended Linux user.
- Multi-user path problems are currently handled by specialized audit/remediation in `oqtoctl`, not as the setup success criterion.

## Proposed install experience

### Commands

Replace the current broad `setup.sh` contract with a small native CLI surface backed by one provisioning engine:

```bash
# Personal install: default recommendation
curl -fsSL https://oqto.dev/install.sh | sh
oqto setup personal

# Team/server install
sudo oqto setup team --domain oqto.example.com

# Never guesses; prints exactly what will change
oqto setup plan --profile personal
sudo oqto setup apply --profile team --domain oqto.example.com

# Repair/reconcile any existing install
sudo oqto doctor --fix

# Show the exact contract and current drift
oqtoctl doctor --json
```

`scripts/setup.sh` can remain as a compatibility wrapper, but it should delegate to `oqto setup ...` after bootstrapping the binary.

### Profiles

| Profile | Intended user | Privileges | Runtime | Default |
|---|---|---:|---|---|
| `personal` | laptop/single operator | no sudo unless installing system packages | user systemd or foreground | yes |
| `team` | Linux server, many app users | sudo/root | system `oqto` + per-user runners | no, explicit |
| `experimental-container` | internal testing only | sudo/root | container | hidden/flagged |

### Architecture

Create a real provisioning crate/engine, aligned with `oqto-3ct7.4`:

```text
oqto setup CLI / setup.sh wrapper
  -> oqto-provisioning
       - plan model: desired files, packages, services, users, groups, sockets
       - detector: current host facts
       - reconciler: idempotent actions
       - verifier: contract-level checks
       - reporter: human + JSON output
  -> adapters
       - systemd, users/groups, filesystem, packages, EAVS, Pi config, runner RPC
```

The important shift is from `run step N` to `declare desired state -> compare -> apply -> verify`.

## Setup contract to encode

### Personal profile success criteria

- `oqto`, `oqtoctl`, `oqto-runner`, `oqto-files`, `oqto-sandbox`, `hstry`, `eavs`, `pi`, and required tool binaries resolve from the service PATH.
- Config exists at `$XDG_CONFIG_HOME/oqto/config.toml` with `local.single_user=true`.
- Backend runner socket pattern is `/run/user/{uid}/oqto-runner.sock` or equivalent `%t/oqto-runner.sock` and every component agrees.
- User systemd units exist or foreground mode is explicitly selected.
- `hstry`, `mmry` when enabled, `eavs`, `oqto-runner`, and `oqto` are active or startable.
- `~/.pi/agent/models.json` and settings are generated from EAVS.
- A login/admin user exists and can authenticate.
- `oqto doctor` can open a runner RPC connection and perform a dry-run session capability check.

### Team profile success criteria

- System user `oqto` exists with the expected home and owns service runtime config.
- Group `oqto` exists; service user and managed app users have correct membership.
- `/etc/oqto`, `/var/lib/oqto`, `/run/oqto`, `/run/oqto/runner-sockets`, shared workspace roots, and per-user homes match an explicit owner/mode table.
- `/etc/sudoers.d/oqto-multiuser` validates with `visudo` and matches the current configured prefix/group/UID range.
- `oqto.service`, `eavs.service`, and optional reverse proxy services are enabled/active.
- Every active Oqto user has a persisted `linux_username`/`linux_uid` and those identities match `/etc/passwd`.
- Every active Oqto user has lingering enabled, a user `oqto-runner.service`, and a reachable socket at the configured canonical path.
- The backend's configured `runner_socket_pattern` is the same path that runner services bind.
- Per-user EAVS env/settings/models files exist, have expected ownership/mode, and are injectable by the runner/backend without leaking to other users.
- Sandbox policy is root-owned, readable by runners, and has required seccomp/landlock assets installed.
- `oqto doctor --team` performs an end-to-end canary: create/invite user or use a test user, ensure runner starts, open RPC, list models, create a session, stop it, and report logs on failure.

## Implementation plan

### Phase 0: reduce immediate confusion

1. Make `personal/single-user` the default in docs, help, and environment defaults. Initial cleanup done in this change; keep future docs aligned.
2. Remove normal container-mode setup examples until container mode is supported. Initial quick-start/help cleanup done in this change.
3. Add `oqto setup doctor` or `oqtoctl doctor` wrapper that runs existing audit checks and prints one remediation command per failure. Initial cleanup added top-level `oqtoctl doctor` as the ergonomic entrypoint for identity/runner socket drift and improved `oqtoctl user doctor-identity` socket diagnostics so missing runner sockets and user-runtime-vs-shared path drift are reported explicitly.

### Phase 1: define the install manifest

Create `backend/crates/oqto-provisioning` with typed manifests. Initial crate now exists with:

- `InstallProfile::{Personal, Team}`.
- `DesiredPath { path, kind, owner, group, mode, purpose }`.
- `DesiredService { name, scope, user, enabled, active, purpose }`.
- `RunnerSocketContract { pattern, producer, consumer_config_key, purpose }`.
- `ProvisioningCheck { id, severity, description, remediation }`.

`docs/reference/install-contract.md` now records the current manifest-backed install contract for personal/team profiles. `./setup.sh --personal` and `./setup.sh --team` now provide explicit intent-level profile flags instead of requiring users to remember `OQTO_USER_MODE` values. `./setup.sh --personal --plan`, `./setup.sh --team --plan`, and `./setup.sh --team --plan --json` render the selected setup contract and exit before host mutation; `./setup.sh --team --doctor` evaluates setup drift through the same contract without mutation and prints summary counts plus concrete remediation command hints, `--apply` performs safe contract fixes such as repairing the shared runner socket directory, `--apply --apply-runners` additionally reprovisions per-user runner services with socket drift, `--apply --apply-services` additionally enables/starts declared system services, while `--strict` turns error-severity drift into a non-zero preflight gate. During installation, sudoers is now installed transactionally in `setup_linux_user_isolation` (candidate temp file -> `visudo` validate -> atomic `install` root:root 0440 -> post-validate). Normal setup now prints a profile summary plus the exact plan command before mutating the host and runs a non-strict post-setup doctor pass with remaining drift/remediation hints before the final summary. `oqto-setup plan --profile team|personal` renders the same setup plan from the manifest. `oqtoctl doctor --contract --profile team|personal` renders this initial manifest and now collects initial host facts for literal paths, system services, configured runner socket pattern, and multi-user sudoers validation before printing contract findings. The crate also has a side-effect-free `evaluate_manifest(manifest, facts)` contract evaluator with tests for runner socket pattern drift, wrong `/run/oqto/runner-sockets` permissions, and suppression of unexpanded per-user template noise. `oqtoctl doctor --contract --profile team` now expands concrete active users from the Oqto database to check Linux identity and canonical per-user runner sockets. Next step: add safe remediation for contract findings.

### Phase 2: move existing behavior into the provisioning engine

Move in this order to reduce risk:

1. Path and permission verifier only. No writes.
2. Service verifier only. No writes.
3. Runner socket verifier and remediation.
4. EAVS/Pi config verifier and remediation.
5. User/group/sudoers reconciliation.
6. Replace shell service/config generation with Rust reconciler.

### Phase 3: make setup self-healing

- `oqto setup apply` always ends with `doctor`.
- `oqto doctor --fix` applies only safe, typed remediations.
- Unsafe remediations require `--allow-destructive` and list exact actions.
- Every failure includes: observed value, expected value, owning component, and log command.

### Phase 4: test setup as a product

Add tests around the install contract, not just code units:

- Unit tests for manifest generation for personal/team profiles.
- Golden tests for generated config, systemd units, sudoers files, and path tables.
- VM/e2e tests for fresh personal install, fresh team install, rerun idempotency, and drift repair.
- Regression cases for known multi-user failures: wrong runner socket path, stale `/run/oqto` permissions, missing linger, stale EAVS config, missing per-user models.json, missing service user config copy.

## Recommended immediate next code changes

1. Fix default inconsistency: set setup default user mode to `single` or update all docs/help if team remains preferred. Recommendation: default to `single`.
2. Hide disabled container mode from quick-start and help examples.
3. Add a canonical runner socket path function/shared config used by setup generation, oqto runtime, oqtoctl status, and audit.
4. Add a generated `docs/reference/install-contract.md` from the provisioning manifest so docs cannot drift from code.
5. Start `oqto-provisioning` with verifier-only checks for the multi-user path table and runner socket contract.

## Why this will make setup awesome

- The user picks an intent (`personal` or `team`), not a matrix of low-level modes.
- The installer shows a concrete plan before mutating the host.
- Rerunning setup becomes normal and safe because it reconciles desired state.
- Multi-user failures become first-class contract violations with precise evidence.
- Paths, permissions, and services have one owner: the provisioning manifest.
- Docs, doctor checks, and install behavior can be generated from the same source of truth.
