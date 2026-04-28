# oqto-sandbox hardening tests

Bullet-proof end-to-end checks for every hardening knob in `oqto-sandbox`.
Each scenario is a standalone bash script; the orchestrator runs them in
numerical order and reports aggregate pass/fail.

## Prerequisites

- Linux host with `bwrap` and user-namespaces enabled.
- `oqto-sandbox` reachable via one of:
  - `$OQTO_SANDBOX_BIN` (explicit override)
  - `oqto-sandbox` on `PATH` (default — covers `just deploy` and `cargo install`)
  - `/var/lib/oqto/releases/current/bin/oqto-sandbox`
  - `backend/target/{release,debug}/oqto-sandbox` (local cargo build)
- Optional: seccomp BPF installed at `/etc/oqto/seccomp/default.bpf` via
  `scripts/sandbox/generate-seccomp-artifacts.sh` + `scripts/sandbox/install-seccomp-policy.sh`.
- Optional: Linux kernel with Landlock (`>= 5.13`, ABI 1+).

Scenarios gracefully `skip` (not fail) when optional pieces are missing, so you
can run the full suite on any dev box.

The suite does **not** build anything. If you want to validate unreleased
local sandbox changes, either `just deploy` first or explicitly pass
`OQTO_SANDBOX_BIN=$PWD/backend/target/release/oqto-sandbox` so the installed
binary is bypassed.

## Running

```bash
# Everything (uses installed oqto-sandbox)
just test-sandbox

# Selected scenarios by numeric prefix
just test-sandbox 03 06 07

# Keep the scratch dir for post-mortem
OQTO_TEST_KEEP_DIR=1 just test-sandbox 07

# Test a local build without installing
OQTO_SANDBOX_BIN=$PWD/backend/target/release/oqto-sandbox just test-sandbox

# Move scratch off tmpfs if /tmp is quota-capped
OQTO_TEST_RUN_DIR=$HOME/oqto-sandbox-tests just test-sandbox
```

Individual scenarios can be run directly:

```bash
scripts/sandbox/tests/07-landlock.sh
```

## Scenarios

| # | Name | What it proves |
|---|------|----------------|
| 01 | preflight | Host meets the suite's requirements; missing optional pieces are surfaced. |
| 02 | profile-presets | `minimal` / `development` / `strict` produce the expected bwrap flag set. Uses `--dry-run`, no execution. |
| 03 | filesystem-boundary | Workspace and `allow_write` paths are writable; `deny_read` secrets are unreadable; home paths outside `allow_write` are read-only. |
| 04 | network-isolation | `isolate_network=true` drops the default route and blocks outbound TCP; `false` preserves network. |
| 05 | pid-isolation | `isolate_pid=true` hides host PIDs; `false` preserves them. |
| 06 | seccomp | Modes `off`, `audit` (with/without BPF), `enforce` (with/without BPF) behave correctly; enforce without BPF aborts the CLI. |
| 07 | landlock | Landlock blocks writes outside `allow_write` in `enforce`. Includes a regression marker for trx **oqto-b4za** (Landlock silently disabled when `disable_userns=true`). |
| 08 | no-new-privs | `no_new_privs=true` sets `NoNewPrivs:1` in `/proc/self/status`. |
| 09 | workspace-merge | `.oqto/sandbox.toml` merges correctly: `deny_read` union, `isolate_network` OR, `allow_write` intersection. |

## Environment variables

| Var | Default | Purpose |
|-----|---------|---------|
| `OQTO_SANDBOX_BIN` | auto-discover | Explicit path to binary; bypasses `PATH` |
| `OQTO_TEST_RUN_DIR` | `$TMPDIR/oqto-sandbox-tests.$$` | Scratch dir (move off tmpfs if quota-capped) |
| `OQTO_SECCOMP_BPF` | `/etc/oqto/seccomp/default.bpf` | Seccomp BPF artifact |
| `OQTO_TEST_KEEP_DIR` | `0` | Keep scratch dir on exit for post-mortem |
| `VERBOSE` | unset | Reserved for future `-v` pass-through |

## Exit codes

- `0` — all selected scenarios passed.
- `1` — one or more scenarios reported failures.
- `2` — fatal preflight problem (e.g. binary not found).

## Adding a scenario

1. Name the script `NN-short-name.sh` so `run-all.sh` picks it up in order.
2. Source `lib.sh`, set `SCENARIO_NAME`, `trap cleanup_run_dir EXIT`, call `scenario_header`.
3. Emit per-check `pass` / `fail` / `skip` lines.
4. End with `scenario_summary`.

## Known issues tracked in trx

- **oqto-b4za** — Landlock is silently skipped when `disable_userns=true`, which is the default for every built-in profile. Scenario 07 documents this with a regression marker.
