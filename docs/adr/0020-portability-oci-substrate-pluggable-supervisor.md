# Portability: OCI container as isolation substrate; the placement supervisor is pluggable; no platform tooling in the core

Status: proposed (grilled 2026-06-25). Generalizes ADR-0011 (placement contract) and **amends ADR-0002** (systemd is demoted from "the local supervisor" to one supervisor backend). Foundation for ADR-0019.

Oqto must run across very different infrastructure: a single VPS, several VMs, a Kubernetes cluster, and developer machines on macOS/Windows. The temptation is to pick one mechanism (systemd, or podman, or k8s) and build on it. That repeats the ADR-0001 mistake at the deployment layer — baking one platform into the core. Instead we commit to portable abstractions and make every platform a swappable backend behind them. The runner already cross-compiles; what leaks platform assumptions is *how the runner is isolated and supervised*, so that is what we abstract.

## Decisions

1. **OCI containers are the universal isolation substrate.** A Workspace runs as an OCI container/Pod. OCI images run on podman, Docker, containerd, Kubernetes, and Docker-Desktop/`podman machine` VMs on macOS/Windows. Choosing container-per-Workspace is what makes Kubernetes *natural* (a Pod is a supervised container), not a port.

2. **The Placement Supervisor is a pluggable contract, not systemd.** Backends implement one trait (start/stop/health/reach a runner at its placement). The core never assumes any of them:

   | Supervisor backend | Where it runs | Tenancy |
   |---|---|---|
   | **rootless podman** *(primary)* | Linux native **and** macOS/Windows via `podman machine` (a Linux VM) | multi-tenant; per-Workspace userns subuid ranges |
   | **kubernetes** | clusters | multi-tenant; Pod per Workspace, `runAsNonRoot` |
   | **Docker** | compatibility (existing Docker users, Docker Desktop) | multi-tenant; container-boundary isolation (weaker userns posture) |
   | **local-process** | macOS/Windows/dev with no VM | single-tenant; OS sandbox only (seatbelt / restricted tokens) |

3. **Rootless podman is the primary backend on every OS.** `podman machine` runs the same managed Linux VM on macOS/Windows that Docker Desktop does, so `podman` is `podman` everywhere and Oqto needs **no Mac/Windows-specific container code** — the machine/VM layer is the engine's concern. podman's **native `pod` primitive** matches the Kubernetes Pod (shared netns), so "Workspace = Pod" runs unchanged from single-node to cluster. Docker remains a compatibility backend (you run it today; some users have Docker Desktop) but is not required by any platform.

4. **systemd is one backend, not the contract.** It stays the supervisor for the Linux single-node/multi-VM case where it is genuinely good (socket activation, ADR-0002), but nothing in the core may assume it. This is the explicit amendment to ADR-0002.

5. **Single-VPS is demoted, not deprecated.** It becomes the `single-node` deployment — the cheapest config, the dev story, the on-ramp — and stops being privileged or special. What we kill is single-VPS-*isms* leaking into the core (host-user tangling, systemd assumptions, `PathBuf`-typed locations).

6. **Tenancy is a property of the placement, advertised by the runner — not of the OS.** The runner declares its tenancy/isolation capability (`local-process` → single-tenant; container → multi-tenant) and the control plane refuses to co-place a second tenant on a single-tenant placement. No OS special-casing in routing.

7. **Work directories are addressed by a typed mount source, not a host path.** Today: `local`/`bind`. Designed-for (not built now): `smb`/SharePoint, object storage, Google Drive — via rclone on single-node, CSI volumes on Kubernetes. Decide the *shape* now (a mount descriptor) so the code stops assuming `workspace_path: PathBuf`.

## Sequencing

Build the **contracts** (Placement Supervisor trait, mount-source type, runner reachability) and ship **one** implementation first: `single-node` with rootless podman on Linux. Because the contracts are honest, Kubernetes and Mac become *additional supervisor impls*, not rewrites — the same inversion ADR-0011 used for gvnr. Validate the seam with a **second** impl early (`local-process`, which doubles as the Mac/dev on-ramp) so single-node assumptions can't masquerade as the abstraction. **Build for k8s-shaped, run on single-node first.**

## Consequences

- ADR-0002's "systemd owns local runner lifecycle" narrows to "systemd is the supervisor for the systemd-Linux placement."
- `oqto-sandbox` becomes a portable, feature-gated, runtime-selected crate (Linux Landlock/seccomp/namespaces, macOS seatbelt, Windows best-effort) — see ADR-0019.
- Mac/Windows become reachable: cross-compiled runner + OCI (via `podman machine`) for multi-tenant, or `local-process` for single-tenant dev.
- New cross-cutting requirement: anything platform-specific lives behind a backend trait; a lint/review gate should reject systemd/podman/path assumptions in core crates.
