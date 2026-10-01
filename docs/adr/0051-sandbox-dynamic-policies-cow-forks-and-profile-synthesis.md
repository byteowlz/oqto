# Sandbox growth: dynamic seccomp policies, CoW environment forks, determinism controls, and profile synthesis

## Status

Proposed (2026-09-27). Tracked by `oqto-dcgt`. Extends [ADR-0019](0019-container-per-workspace-placement.md) (container-per-workspace placement) and [ADR-0020](0020-portability-oci-substrate-pluggable-supervisor.md) (portable OCI substrate, pluggable supervisor); composes with the egress tiers of [ADR-0035](0035-unified-egress-tiers-traceability-pluggable-enforcers.md) and the isolation-tier boundary documented in `docs/active/design/20260609-isolation-tiers-and-egress.md`. Relates to `oqto-nppq.7` (portable feature-gated oqto-sandbox, runner-applied Landlock+seccomp) and `oqto-nppq.11` (container isolation tier + non-privileged Landlock/seccomp defense-in-depth). The deterministic controls serve the verification-gate direction recorded in the byteowlz wiki (decision-model research).

## Context

`oqto-sandbox` today gives each workspace a bwrap user namespace with an inner Landlock shim, a seccomp deny filter, three egress tiers whose strongest local tier places the workload in a dedicated network namespace with a single veth (no forwarding, no masquerade — a raw-socket bypass still reaches only the host-side veth), guard limits where cgroup delegation exists, and per-backend capability contracts for Linux and macOS.

Five capability gaps remain, each observed in external process-sandbox tooling and each needed by a concrete Oqto use case:

1. **Dynamic resource policies without cgroups.** Guard limits (`max_memory_bytes`, `max_cpu_seconds`, `max_open_files`) require cgroup delegation that is unavailable on several deployment hosts, and they are static for the lifetime of the process. There is no per-sandbox memory accounting, no process-count cap, and no way to bound a workload on a host where cgroup v2 delegation cannot be granted.
2. **No deterministic mode.** Verification and benchmark runs (retry-the-turn loops, verification gates, decision-model evaluation) want a frozen or adjustable clock and a deterministic random source so repeated runs of the same task observe the same environment.
3. **No environment fork.** Spawning a second attempt at "what the workspace looks like right now" means a full copy or a git worktree plus setup rerun. There is no cheap fork of a *confined, running* environment for parallel attempts or map-reduce fan-out.
4. **Egress pre-filter is transport-only.** The tier-2 relay is deliberately pure mechanism: the EAVS policy decision happens per connection upstream. A deterministic, in-namespace HTTP-level allow/deny (method + host + path) would let obviously-disallowed destinations fail inside the namespace without a policy round trip, and would give the egress checkpoint a structural layer beneath the model-facing policy.
5. **Profiles are hand-authored.** Confinement profiles (paths, domains, syscalls) are written by a human from an observed run. The `oqto-egress` compiler generates egress config; nothing generates the confinement profile itself.

## Decision

Grow `oqto-sandbox` with five capabilities, implemented as feature-gated additions to the existing confinement sequence (user namespace → Landlock → seccomp deny+notify → exec). Each capability is independently shippable; none changes the placement contracts of ADR-0019/0020 or the mechanism/policy split of ADR-0035.

### 1. Seccomp user-notification policy engine

Adopt seccomp user notification as a supervised interception layer, opt-in per sandbox profile, with handlers executed by the existing supervisor:

- `clone/fork/vfork` → **process-count cap** per sandbox.
- `mmap/munmap/brk/mremap` → **memory accounting** with an enforced per-sandbox limit, replacing cgroup delegation where it is unavailable; `mprotect` accounting optional behind a profile flag.
- `openat` on procfs paths → **/proc virtualization**: network tables filtered to the sandbox's own sockets (socket-cookie ownership), PID entries filtered to the sandbox's process group.
- `getdents64` on procfs → **PID filtering** consistent with the virtualization above.

Latency note: an intercepted syscall costs a supervisor round trip. Profiles that enable handlers must record which syscalls they intercept; hot-path interception (e.g. `openat`) is enabled only when the virtualization or accounting it provides is requested.

### 2. Determinism controls

Profile-level options:

- `clock = "host" | "frozen" | "offset(<duration>)"` — implemented by intercepting the clock/sleep syscall family and adjusting results; frozen mode pins to a captured start time.
- `prng = "host" | "deterministic(<seed>)"` — `getrandom` serviced deterministically.

Deterministic mode is for verification and benchmark runs; profiles must refuse determinism + `allow_internet_access` together (a deterministic clock with live network traffic is a false reproducibility claim).

### 3. CoW environment forks

A `fork` operation on a running (or prepared) sandbox environment: the confined workspace — mounts, applied Landlock ruleset, seccomp filters, and current writable layer — is duplicated into an independent sandbox with its own identity, sharing unmodified storage blocks.

- Forks are created from a sandbox in a quiescent or checkpointed state; the fork inherits the confinement (nothing escalates).
- Each fork keeps its own egress tier and policy identity; a fork never shares the parent's capability tokens (ADR-0039, ADR-0048: credentials are injected per placement at start, not inherited as state).
- Fan-out (`1 → N` forks) and re-merge are out of scope for the fork primitive itself; merging happens through the existing workspace/git paths (ADR-0033 attribution, ADR-0034 concurrency).

### 4. In-namespace HTTP egress pre-filter

An optional per-sandbox HTTP allow/deny list (`method host/path` rules) enforced **inside the namespace, before** the tier-2 relay hands the connection to EAVS. It is mechanism, not policy: the rules are compiled by `oqto-egress` from the same `WorkspaceEgressPolicy` that drives EAVS, and the relay/EAVS remain the policy authority and audit point (ADR-0035). Purpose: deterministic, in-namespace rejection of obviously-disallowed destinations (cheap fail, no policy round trip, no reliance on the workload cooperating with proxy env). HTTPS pre-filtering without content inspection requires only SNI/host matching; content-level rules stay with EAVS.

### 5. Profile synthesis

A `oqto-sandbox learn` mode: run a workload once under observation (syscalls, opened paths, contacted destinations), then emit a candidate profile (read/write path sets, syscall allowlist, egress domain list) for human review before use. Synthesized profiles are proposals with recorded provenance (which run produced them); they are never applied without an explicit adoption step, keeping the user-owned-config and policy-authority rules intact.

## Constraints

- **Kernel floors**: seccomp user notification ≥5.6, Landlock TCP ≥6.7, Landlock IPC ≥6.12. Hosts below the floor keep today's behavior; capabilities degrade by profile, with the capability contract (`capability.rs`) reporting which features a backend can enforce.
- **Same-user trust model**: Landlock/seccomp confine *what the workload can reach*, not *who the workload is*. This tier never substitutes for the Account/Placement authority boundary or the microVM tier for hostile workloads (per the isolation-tiers doc).
- **Supervisor latency**: every intercepted syscall pays a round trip; profiles gate interception so hot paths stay un-intercepted unless their handler is requested.
- **TOCTOU**: handlers that decide on path arguments must re-resolve against the supervisor's own view (documented guarantee required per handler); the review for this belongs in the per-feature children.
- **Mechanism/policy split (ADR-0035) holds**: the in-namespace pre-filter and the seccomp engine enforce; EAVS decides.

## Alternatives considered

- **Status quo** (bwrap + Landlock + deny filter, cgroup guards where available): leaves capabilities 1–5 unsolved on hosts without cgroup delegation, and leaves verification/benchmark runs non-reproducible.
- **Full microVM tier for everything** (external sandboxes of this class exist): strongest isolation, but pays seconds of start and hundreds of MB per sandbox for capabilities that are needed at process-sandbox latency and density; the tiers exist precisely so this choice is per workload.
- **Adopt an external process-sandbox project wholesale**: rejected — it would duplicate the existing capability contracts and Landlock shim, import a second policy owner beside EAVS, and give up control over the confinement sequence that the egress tiers depend on.

## Consequences

- `oqto-sandbox` grows a supervisor-based interception layer and profile knobs; the `capability.rs` contract per backend grows matching declarations so the runner can advertise what a profile can enforce.
- Determinism + CoW fork together give verification loops: fork per attempt → run → verify → discard, with a reproducible environment per run.
- Profile synthesis shifts profile authoring from hand-written to reviewed-generated, with provenance recorded.
- Kernel-floor and latency constraints become profile-visible: the runner can report "this profile requires 6.12" and "interception enabled on openat" instead of failing opaquely.

## See also

- [ADR-0019](0019-container-per-workspace-placement.md) · [ADR-0020](0020-portability-oci-substrate-pluggable-supervisor.md) · [ADR-0035](0035-unified-egress-tiers-traceability-pluggable-enforcers.md) · [ADR-0033](0033-session-file-attribution-and-supersession.md) · [ADR-0048](0048-pi-owns-machine-provider-credentials.md)
- byteowlz wiki: `research/oqto-three-speed-workspaces.md` (filesystem-fork counterpart), `research/decision-model-control-plane.md` (verification gates the determinism controls serve)