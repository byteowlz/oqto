# Backend build metrics

This file keeps the backend refactor honest. When moving code between crates, record comparable timings here instead of relying on subjective build-speed impressions.

## How to measure

Run from the repository root unless a command says otherwise. Use a mostly idle machine and record the commit, host, CPU count, Rust version, and whether the build is warm or cold.

Timing helper when `/usr/bin/time` is unavailable:

```bash
TIMEFORMAT='seconds=%3R'; time <command>
```

Baseline command set:

```bash
# Warm check of the main server crate
cd backend && TIMEFORMAT='seconds=%3R'; time cargo check -p oqto

# Warm clippy of the main server crate
cd backend && TIMEFORMAT='seconds=%3R'; time cargo clippy -p oqto

# Incremental rebuild after touching host-runtime code
touch backend/crates/oqto-host/src/runtime.rs
cd backend && TIMEFORMAT='seconds=%3R'; time cargo check -p oqto

# Incremental rebuild after touching session orchestration code
touch backend/crates/oqto/src/session/service.rs
cd backend && TIMEFORMAT='seconds=%3R'; time cargo check -p oqto

# Local deploy profile build for the main server package
cd backend && TIMEFORMAT='seconds=%3R'; time cargo build --profile deploy-fast -p oqto --bin oqto --bin oqto-sandbox

# Local deploy profile build sequence used by scripts/deploy.sh
cd backend && TIMEFORMAT='seconds=%3R'; time \
  (cargo build --profile deploy-fast -p oqto --bin oqto --bin oqto-sandbox && \
   cargo build --profile deploy-fast -p oqtoctl --bin oqtoctl && \
   cargo build --profile deploy-fast -p oqto-runner --bin oqto-runner && \
   cargo build --profile deploy-fast -p oqto-files --bin oqto-files && \
   cargo build --profile deploy-fast -p oqto-usermgr --bin oqto-usermgr)
```

## Baseline: 2026-04-30, commit `1560c8c6`

Environment:

| Field | Value |
| --- | --- |
| Host | `arch-dev-01` |
| CPU threads | 8 |
| OS | Linux `7.0.2-arch1-1` x86_64 |
| Rust | `rustc 1.95.0 (59807616e 2026-04-14)` |
| Cargo | `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` |
| State | Warm/incremental existing workspace; no target clean |
| Mold | Not used |

Results:

| Workload | Command summary | Seconds | Notes |
| --- | --- | ---: | --- |
| Warm server check | `cargo check -p oqto` | 31.421 | Rechecked `oqto` after recent edits |
| Warm server clippy | `cargo clippy -p oqto` | 0.353 | No codegen needed after check |
| Host-runtime incremental check | `touch oqto-host/src/runtime.rs && cargo check -p oqto` | 8.842 | Rebuilt `oqto-host` and `oqto` |
| Session-service incremental check | `touch oqto/src/session/service.rs && cargo check -p oqto` | 8.918 | Rebuilt only `oqto` |
| Deploy-fast server package build | `cargo build --profile deploy-fast -p oqto --bin oqto --bin oqto-sandbox` | 71.452 | Rebuilt touched host/server artifacts |
| Deploy-fast binary build sequence | deploy script cargo sequence | 55.120 | Mostly warm; rebuilt `oqto-runner` |

Previously measured cold-build context from the deploy-profile work:

| Workload | Seconds | Notes |
| --- | ---: | --- |
| Cold `--release` build | ~555 | About 9m15s before `deploy-fast` |
| Cold `--profile deploy-fast` build | ~269 | About 4m29s; roughly 51% faster |
| Cold `deploy-fast + mold` build | ~400 | About 6m40s; slower on this host, so mold remains opt-in |

## Interpreting future changes

- If a crate extraction is successful, touching an extracted crate should rebuild fewer packages or at least make ownership clearer without increasing rebuild time.
- If a dependency points upward into `oqto`, the extraction is incomplete and will usually worsen rebuild scope.
- Compare warm incremental numbers for day-to-day developer experience; compare cold deploy numbers for release/deploy impact.
