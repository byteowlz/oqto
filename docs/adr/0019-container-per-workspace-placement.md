# Container-per-workspace is a Placement, not a runtime mode

Status: accepted (grilled 2026-06-25; accepted 2026-07-27 as the intended
direction, not yet implemented). Depends on the PlacementStore contract (`oqto-3ct7.16`, ADR-0011), the dead-code deletion (`oqto-3ct7.15`), the portability foundation (ADR-0020), and consumes the prebuilt bundle from ADR-0018. **Amends ADR-0009** (host-Principal lifecycle collapses for container placements).

We want each personal user and each shared workspace to run as its own container with a runner inside it. ADR-0001 already abolished backend "container mode": the backend only speaks the runner protocol and never orchestrates containers in the session path. So this is **not** a revival of `RuntimeMode::Container` — it is a new **Placement** whose **Placement Supervisor** is a container engine (ADR-0020), exactly parallel to "systemd locally, k8s for pods" (CONTEXT.md, ADR-0002). A runner behaves identically regardless of placement; the protocol does not change. The dead pre-ADR-0001 container code (`oqto/src/container/*`, `RuntimeMode`) is **deleted, not extended** (`oqto-3ct7.15`).

## Vocabulary

Per the corrected glossary (CONTEXT.md): a **Workspace** is the whole — a named collection of **work directories** owned by one Account (personal) or shared across Accounts (shared). A **work directory** is where a harness (Pi) process runs and loads its `AGENTS.md` from — the unit carrying one agent persona. The code's current `workspace_path` / `SharedWorkspace.path` actually name a *work directory* / *Workspace* respectively — a pending rename.

## Keying: Workspace = Principal = container

Routing already resolves an `ExecutionTarget` and maps it to a runner, storing no `runner_id` on sessions (`oqto/src/runner/router.rs`). Shared workspaces already resolve `workspace_id → dedicated Principal → dedicated runner socket`. Container placement is the same shape, substituting the isolation owner:

| | Today (host placement) | Container placement |
|---|---|---|
| Isolation owner | dedicated Linux **Principal** (host uid) | dedicated **container** (own fs + uid namespace) |
| Lifecycle owner | systemd template unit | container engine (ADR-0020) |
| Reachability | unix socket | same unix socket, **bind-mounted from the container** |
| Runner binary | unchanged | unchanged |

- **Container : Workspace is 1:1**, so **one runner per Workspace**.
- **Personal default:** one Account → one personal Workspace → one container holding that Account's many work directories. Finer isolation of a single work directory is achieved with `oqto-sandbox` (Landlock) **inside** the container, not by spawning a new container; a separate container is the rare strongest option.
- **Shared:** each shared Workspace → its own dedicated container, always; multiple Accounts attach.

"Per-user" and "per-shared-workspace" are the *same rule* once Workspace = the container scope: **each Principal becomes its own container.** Because the backend already dials a target-resolved socket and persists no runner identity, the backend session/routing path barely changes — the socket is now serviced by a runner in a container. Placement stays invisible to the protocol.

## Drop host Linux users

For container placement, host **login users** are no longer the isolation mechanism, so they go:

- **Delete:** `useradd`/home dirs/shells, sudoers entries, `loginctl enable-linger`, `oqto-host/linux_users.rs`, and `oqto-hostd`'s user-creation. ADR-0009's per-Principal host-uid lifecycle collapses here.
- **Replacement:** the container boundary + **userns subuid/subgid ranges per Workspace** (a number allocation, not an account) + unprivileged Landlock inside. `oqto-hostd` shrinks from "user-lifecycle broker" to "allocate a subuid range + start a container".
- **The Principal concept survives, realized per placement:** container → Principal = the container; `local-process` → Principal = the invoking OS user (single-tenant). "Drop Linux users" holds on every target; only the replacement differs (subuid ranges on single-node, `runAsNonRoot` on k8s, nothing on Mac dev).

## Data model: filesystem-as-truth, shared-nothing

Oqto's paradigm — the filesystem is the source of truth, state is per-Workspace SQLite (oqto-log) + files — is what *makes* this clean:

- **The container boundary is the data boundary is the Workspace.** Shared-nothing: no data crosses containers, no shared central DB, no distributed-systems tax.
- **SQLite stays single-writer-local**, which is the only safe way to run it. oqto-log lives on the Workspace volume, accessed only by that one container's runner. Never shared, never on a network FS.
- **Container = ephemeral compute (cattle); the per-Workspace volume = durable source of truth (oqto-log, mmry, work-dir files).** A volume snapshot is a complete, consistent Workspace backup.

## Workspace = Pod; shared trusted infra is wired as endpoints

A Workspace is a **Pod** (a podman/k8s pod — shared netns): the **runner container** (with oqto-log and mmry embedded on its volume) plus any genuinely per-Workspace sidecars.

- **Embedded (data, per-Workspace volume):** oqto-log SQLite, mmry-core (`oqto-6ek1`, ADR-0010).
- **Shared trusted infra, wired as an endpoint:** **eavs** (egress/LLM proxy) is a **shared node/cluster service by default**, not a sidecar. The boundary that matters is *untrusted agent code ↔ trusted infra*; agents reach eavs only through the per-Account virtual-key API (ADR-0007), which already provides per-Workspace policy on a shared instance. Replicating eavs per Workspace would isolate the trusted thing — low value, N× cost.
- **eavs is "an egress endpoint"; the runner is topology-agnostic.** Shared-node-service is the default; a **per-Workspace eavs sidecar is an opt-in knob** for defense-in-depth against a bug *in eavs itself* (hostile-multi-tenant). Flipping between them is wiring, not architecture.
- **Level-2 egress capture** (`oqto-h6hr`) works with shared eavs: the Pod's netns transparently redirects egress to the shared eavs address (a sidecar only changes the target to localhost). So level-2 does not force a sidecar.

## Sandbox inside a rootless container

The first rootless-Podman implementation runs both runner and harness as fixed, unprivileged uid 1000 inside a user namespace. The Podman boundary applies `NO_NEW_PRIVS`, drops all capabilities before adding only the reviewed tooling subset, restricts mounts, and enforces resource limits.

Do **not** run bubblewrap recursively by granting `CAP_SYS_ADMIN`: live container testing proved that nested bwrap cannot construct its namespace under the rootless/no-new-privileges boundary, and adding `SYS_ADMIN` would weaken the load-bearing cross-tenant boundary. The Workspace image therefore carries an explicit `enabled = false` runner-sandbox configuration for this mode; this is not an implicit fallback. The runner logs reduced isolation, and the outer container remains the security boundary.

A future supervisor capability may select a defense-in-depth backend that works without privileged nested namespaces (for example direct Landlock/seccomp application by the trusted runner). Until that is implemented and advertised, the supported container mode is fixed-unprivileged-user plus rootless Podman isolation—not in-container root. Any in-container-root mode remains fail-closed and must provide the monotonic Landlock/seccomp layer described below before it can be enabled:

- **The runner (trusted parent) installs the sandbox, then `execve`s the harness** with `NO_NEW_PRIVS`. The agent never installs or can skip its own sandbox.
- **Landlock + seccomp are the load-bearing intra-Workspace boundaries because they are monotonic and irrevocable—even for root.**
- **No supported mode grants `CAP_SYS_ADMIN`.**
- **`oqto-sandbox` is portable and feature-gated**; where its backend is unavailable, the runner stays unprivileged and advertises reduced isolation rather than silently claiming full isolation.

## Tool provisioning (containers are cattle)

1. **Base image** ships the curated Oqto toolchain (from the ADR-0018 prebuilt bundle); updated by republishing + re-pull.
2. **Agent runtime installs persist via the Workspace volume**: `~/.cargo`, `~/.local`, `~/.npm`, venvs, `mise`/`nix` profiles are volume-backed, so `cargo install`/`uv`/`npm i -g`/`mise use` survive restarts with no image rebuild and no host privilege. Prefer user-space managers (mise/uv/nix) as the default tooling path.
3. **System packages are declarative, not imperative**: a per-Workspace tool manifest derives an image layer (reproducible). Ad-hoc `apt install` is allowed but ephemeral. Reproducibility over accreted mutation ("config must match runtime truth").

## Operability: transparency relocated, not lost

Containerizing obscures the host-as-truth debug story (host files owned by mapped subuids, processes in PID namespaces). This is **a first-class deliverable of this epic, not an afterthought** — otherwise debugging regresses:

- **Mandatory labels** (`oqto.workspace`, `oqto.account`, `oqto.placement`) on every container/volume — the identity layer replacing `/etc/passwd`.
- **`oqtoctl` debug verbs** (`ws ls / ps / exec / logs / inspect / fs`) — thin, label-driven wrappers over podman now, kubectl later (identical mental model).
- `podman unshare` makes mapped subuids legible again; `podman exec` is the new `sudo -u`.
- **Logs always exported to oqto-log + host journal**, never trapped in a container.
- A **Workspace → placement registry** (PlacementStore) answers "where is X". Transparency moves from the host filesystem to a queryable control plane — which is required anyway the moment you go multi-host/k8s, where "ssh and ls" never worked.

## Consequences

- New epic `oqto-nppq`; blocked on `oqto-3ct7.16` (PlacementStore) and `oqto-3ct7.15` (delete dead container/runtime-mode code).
- `oqto-hostd` privilege surface shrinks dramatically (subuid-range allocation + container start, not user lifecycle).
- The per-workspace image is downstream of ADR-0018's complete prebuilt bundle (`oqto-vemr.9`).
- `RunnerHello` gains a `placement`/capability field; sessions stay placement-agnostic.
- The existing all-in-one `deploy/docker` image is superseded for multi-tenant use; may remain as a personal/demo image.

## Volume layout (resolved 2026-07-29)

Two host directories per Workspace, both mounted into exactly one container
(single-writer-local; never shared/NFS):

| Host (backend-owned roots) | Container | Contents |
|---|---|---|
| `<data>/placements/<ws_id>/` | `/home/oqto` | durable state volume: `~/.pi` (Pi-owned session JSONL — sacred), `~/.local/share/mmry`, `~/.local/state/oqto` (oqto-log SQLite), `~/.cargo`, `~/.npm`, `~/.local`, venvs — agent tool installs persist without image rebuild |
| workspace path (canonical) | same path + `/workspace` | work-dir files; canonical path preserved so session metadata and Pi JSONL cwd references stay stable |
| `<state>/placements/<ws_id>/rsock/` | `/run/oqto` | runner socket (container-owned under auto userns) |
| `<state>/placements/<ws_id>/endpoints/` | `/run/oqto/endpoints` | granted endpoint sockets (backend-owned) |

Container = ephemeral compute; volume = durable truth; a filesystem snapshot
of the two volumes is a consistent Workspace backup. Under `userns=auto`,
volumes are chowned (`:U`) into the container's disjoint subuid range: tenant
separation on the host filesystem, at the cost that backend post-start writes
must go through the runner (oqto-nppq.15). Proof:
`oqto-placement/tests/volume_truth_live.rs` — destroy + recreate under a new
subuid range preserves Pi JSONL, mmry, oqto-log, tool installs, and
workspace files.

## Remaining open questions

1. **Remote reachability**: bind-mounted unix socket covers local; remote container hosts need dial-in registration — in scope now, or local-only first?
2. **Mount-source types**: model the typed mount descriptor now (ADR-0020), build only `local`; sequencing of smb/object-storage/gdrive later.
3. **Resource governance**: per-Pod cpu/mem limits, count caps, idle-eviction — PlacementStore policy vs supervisor config.
