# Oqt placement latency — 2026-10-19

Workstream: `oqto-t7yh`. **Partial benchmark, not all-placement acceptance.**

## Reproduce

```sh
python3 tools/oqt/placement_benchmark_test.py
python3 tools/oqt/placement_benchmark.py --help
python3 tools/oqt/placement_benchmark.py --samples 6 --local-tui --pairs 5 \
  --output /tmp/oqt-placement-benchmark.json \
  --report /tmp/oqt-placement-report.md
```

Stdout is schema-versioned JSON; stderr is the human summary. Files contain
monotonic start/end nanoseconds, wall durations, exit statuses, artifact identity,
host/client versions and output hashes, not raw subprocess diagnostics. No Session
or owner profile is opened by `--runtime-info`. Every subprocess is pinned to one
allowed CPU at nice 19. Commands have bounded deadlines and timeout process-group
cleanup. No arbitrary prepared-environment command execution is provided.

## Standalone installed oqt: five cold/warm pairs

**Measured, not canonical runner integration.** `local_standalone_oqt` runs the
pinned installed `runtime/oqt`, never the checkout entrypoint. `coding_fixture`
creates a unique fake owner, loopback scripted provider, workspace and state per
pair. Provider calls remained **zero** throughout; owner files stayed identical.

| Metric / origin | Cold n=5 p50 / p95 | Warm n=5 p50 / p95 |
|---|---:|---:|
| Launch → editable frame + accepted unsubmitted draft | 11865.93 / 12266.90 ms | 8453.27 / 8608.47 ms |
| Launch → protected revoke RPC ACK (**upper bound**) | 11889.76 / 12292.47 ms | 8479.18 / 8631.98 ms |
| **Post-ready command send** → first sealed noop ACK | 153.77 / 158.77 ms | 159.87 / 179.21 ms |
| Launch → first sealed noop ACK | 12045.63 / 12446.26 ms | 8658.41 / 8791.86 ms |

Cold means a new fixture and new host (cached artifact, no OS cache drop).
Warm means a new `--continue` PTY client attached to the same fixture host PID.
All ten clients succeeded. Observed host affinity was CPU **0**, nice **19**.
No mixed cold/warm distribution is presented in the human report.

Editable-frame proof waits for `oqt coding`, types a draft, observes it, then
clears it without submission. Guard readiness is a client-observed upper bound:
`/permissions revoke` awaits the real protected guard's RPC acknowledgement,
not a provider response. It includes editor/draft time and probe overhead; it is
not an internal guard-ready timestamp. The command is `true` with a unique
fixture comment. A durable acknowledgement is timed first, then `/shell-results`
is queried twice to bind the **exact command to its Session:operator:UUID receipt
ID and admitted status**, require the same ID on re-read, and reject ID reuse.
Receipt checks do not reconcile by visible order or generic success text. Each
sample exports that exact receipt ID and unique command. The post-ready command
roundtrip and launch-to-noop have intentionally different origins.

All intervals use client `time.monotonic_ns()`; wall-clock traces are never
subtracted. Five cleanup records confirm positive owner/runtime-identified
PIDFD retirement, host manifest disappearance, and own IPC alias removal;
disposable fixture directories are removed by their context managers.

An intermediate run exposed a test-harness issue: repeating an identical
`/shell-results` screen produces no PTY redraw. Waiting for fresh bytes then
timed out. The adapter now changes status with an acknowledged protected RPC
before re-reading the exact receipt. The interrupted experiment's remaining
fixture host was separately retired using the same positive PIDFD checks. This
was a display-observation issue, not a guard or receipt failure.

Latest separate verification samples: first invocation 8736.18 ms (n=1),
subsequent p50 3724.14 / p95 4927.27 ms (n=5). These include host contention and
must not be subtracted from PTY startup timings.

## Initial verification-only baseline (superseded raw report)

Installed artifact:
`bf11f15f74645fcb2a001bf5cd844bee7c0d8a9199b4399686bcc191a597c31c`.
Linux x86_64 `7.1.11-arch1-1`, Python 3.14.7.

| Verification invocation | n | p50 | p95 |
|---|---:|---:|---:|
| First invocation in experiment | 1 | 3773.83 ms | 3773.83 ms |
| Subsequent invocations | 5 | 3874.31 ms | 3926.30 ms |
| Combined | 6 | 3874.18 ms | 3926.30 ms |

All six succeeded. Individual durations: 3773.83, 3823.77, 3924.23,
3874.18, 3874.31, 3926.30 ms. Quantiles use nearest rank; n=5 is not a
robust tail estimate. These are launcher plus full verification times, not a
separately instrumented verifier span. No baseline subtraction is performed.

The JSON phase spelling `cold` means first *verification* invocation only.
Inventory already executes `oqt --help`, which also verifies the artifact.
Neither phase is an OS-cache-cold measurement. `warm` does **not** mean a lingering
host attach. No caches were dropped or hosts restarted. Repeated independent
cold placement starts and warm attaches remain unmeasured.

## Read-only inventory and supported path

Reviewed ADR-0011, ADR-0019, ADR-0020, the Hypeman spike, `oqto-430k`,
`oqto-nppq`, and `oqto-t7yh`. The placement crate currently contains
`LocalProcessSupervisor` and `PodmanSupervisor`; it has no microVM, Kubernetes,
Docker or external-runtime supervisor implementation. ADR tables are intended
architecture, not proof of implemented adapters.

- Podman 6.1.0: local image enumeration succeeds (34 image IDs). A follow-up
  read-only enumeration found `ghcr.io/byteowlz/oqto-workspace:0.5.0`, image ID
  `cea084830cefa28e1d012dfe15f35c7eefb21bb68c8b396fcb89ff222d304f15`,
  digest `sha256:a30517153b89cfd3c2382494c65e725092f536a9997d4bf7dbeacc5907112103`,
  source revision `44732ace0d4201a00ac11084b9a11dbcee7b9dcc`, plus a local dev
  image and dangling prior builds. Thus **images are present**, not absent.
  These old Workspace images have not been proven to contain the installed
  experimental oqt artifact or its guard wiring. The structured runtime-info
  query failed with exit 125; no engine readiness is inferred from image listing.
- Docker 29.7.2: explicit local Unix-socket enumeration succeeds (six images),
  none matching oqto/Kata/Hypeman names. Engine version is not a placement test.
- `dpty`, `hypeman`, `containerd-shim-kata-v2`, `ctr` v2.3.4,
  `oqto-runner`, and `oqtoctl` are installed. Only `--help` was used for dpty
  and Hypeman. Runner `--version` exits 2; no version is invented.
- Firecracker, Cloud Hypervisor, QEMU system, `kata-runtime`, and `kubectl`
  are not on PATH. This does not prove no private/off-PATH installation exists.
- `/dev/kvm` exists, mode 0666, and is readable/writable. No KVM ioctl or VM boot
  was attempted. Kata shim presence does not prove guest assets or usable runtime.
- Docker/containerd sockets exist; containerd socket is root-owned. No privilege
  escalation, daemon activation, fleet/SSH discovery, or private guest-store scan.

The real OCI path is `PodmanSupervisor` → `PlacementSpec` → dedicated Workspace
runner socket (or mutual TLS) → canonical runner protocol. The example
`backend/crates/oqto-placement/examples/run_container_placement.rs` persists a
registry entry, uses open networking and defaults to two CPUs; it was **not run**.
It is not an oqt guard-ready benchmark. Do not substitute `podman run true` for it.

## Metric coverage / exact missing inputs

Every placement has nine metric keys, including launch-to-first-sealed-noop.
`local_process` has verification samples; `local_standalone_oqt` has the four
PTY metrics above. Foreign/canonical integration gaps remain null, never zero.

| Placement | Missing input for authorized readiness measurement |
|---|---|
| Local process | Disposable canonical runner + oqt guard probe, authenticated reachability and no-op receipts; PTY editable-frame detector. The standalone installed coding fixture is measured separately above. |
| Rootless OCI | Prepared compatible oqt/guard Workspace image, isolated single-CPU PlacementSpec/registry lifecycle and canonical runner/auth/guard probes. Cached 0.5.0 alone is not proof. |
| MicroVM | Implemented supervisor/transport adapter; explicit prepared kernel/rootfs or Kata guest image and configuration; disposable VM lifecycle; guest runner/auth/guard proof. |
| Remote runner | Explicitly authorized disposable endpoint, credentials and lifecycle. No live remote workloads may be restarted. |
| Kubernetes | Implemented supervisor plus authorized disposable namespace/image/auth fixture. |
| Docker | Implemented supervisor and compatible authorized runner/guard fixture. |
| systemd | Explicit disposable unit and runner/guard probe, not a live service restart. |
| External runtime | Integrated placement adapter and authorized disposable runner/guard fixture; CLI presence is insufficient. |

`raw_boot`, `first_editable_frame`, `runner_reachability`, `runner_auth`,
`guard_ready`, and `first_noop_execution` remain **not measured** for canonical
runner placements; standalone oqt measurements are explicitly separate above.
`queued_input_acceptance` is explicitly **unsupported**, because startup queuing
is not implemented. Input echo must not be used as evidence of durable acceptance.
No LLM/provider/network latency is included. Historical spike boot/restore numbers
are not imported into this matrix: they describe another host/configuration and
raw substrate readiness, not authorized oqt runner + guard readiness.

## Regression mechanism and boundaries

Nine hermetic tests cover schema/matrix completeness, null unavailable quantiles,
nearest-rank statistics, timeout handling, CPU/nice restrictions, missing
executables, verification-vs-readiness separation, exact receipt identity binding,
cold/warm summary separation, and refusal to signal a foreign fixture owner. They do not claim live
placement E2E coverage. No production source, justfile, user config, live workload,
service or registry was changed. No image acquisition, source provisioning,
commits or deployment. Parent owns tracker status and artifact publication.
