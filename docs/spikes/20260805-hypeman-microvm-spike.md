# Spike: Hypeman as the microVM placement tier

**Date:** 2026-08-05  **Hypeman version:** v0.2.0 (server) / v0.16.1 (CLI)
**Goal:** Decide whether `github.com/kernel/hypeman` is a viable fourth placement
backend (ADR-0020) and whether its standby/restore provides an idle suspend/resume primitive.
**Host:** local workstation (Arch, KVM, 9 GB free), Firecracker v1.14.2 + Cloud Hypervisor v51.1
(auto-downloaded by Hypeman). Full teardown performed; workstation restored.

## Verdict

**Viable — with two forks that must be resolved before adoption.** The substrate
boots OCI images as microVMs and standby/restore is real and fast (the headline).
The egress *machinery* is present but its enforcement was inert in this environment,
which is a "claimed-but-unwired" gap that must be closed and proven. The workspace
filesystem model has no live host-directory mount, which forces an architecture decision.

## Proven at runtime

| Capability | Result |
|---|---|
| OCI image → microVM boot | ✅ Firecracker **and** Cloud Hypervisor; OCI→erofs conversion via `mkfs.erofs` |
| `exec` into VM | ✅ on CH (guest-agent over **vsock :2222**); `uname` → `Linux hypeman 6.12.8+ …` |
| VM internet egress (NAT bridge) | ✅ `wget ifconfig.me` returned content |
| **Standby** (snapshot to disk) | ✅ **229 ms** |
| **Restore** (resume from snapshot) | ✅ **66 ms cold / 72 ms warm** |
| VMM + kernel acquisition | ✅ auto-downloaded (FC v1.14.2, CH v51.1, 5× CH kernels) |
| Network allocation | ✅ TAP + IP per VM, bridged + MASQUERADE |

**The sleep/wake primitive is real.** Sub-100 ms restore means wake-on-incoming-request
is viable — an idle workspace VM can snapshot and resume within interactive tolerance.

## Source-level architecture (read, not run)

- **Egress proxy** (`lib/egressproxy/`): host-side HTTP/HTTPS **MITM** proxy on the bridge
  gateway; injects `HTTP_PROXY`/`HTTPS_PROXY` + CA into guest; **mock→real secret
  substitution** allowlisted per destination host; **rotation without reboot** via
  `PATCH /instances/{id}` (guest keeps `mock-<NAME>`, host recompiles injection rules).
  This is ADR-0007 layer-2 + ADR-0030's model, already implemented for the VM tier.
- **Enforcement** (`enforce_linux.go`): `iptables -I FORWARD … -i <tap> … -j REJECT`
  to block direct non-proxy TCP (`all` or `http_https_only`).
- **Volumes** (`lib/volumes/`): persistent **block** storage (ext4 sparse → `/dev/vdX`),
  overlay (COW), multi-attach (read-only share), from-archive creation. **No live
  host-directory mount.**
- **Standby/restore** (`lib/snapshot`, `lib/autostandby`, `uffdpager`/`uffdgraduate`):
  conntrack-based auto-standby, userfaultfd lazy restore, scheduled snapshots, forking.
- **Guest agent**: baked into the **initrd** (NOT the Docker "builder image", which is
  only for the `/builds` feature).

## Forks / gaps / concerns

### 1. No live host-directory mount (workspace model) — **architectural decision**
Oqto's workspace is a live host directory. Hypeman offers block volumes + overlay only;
virtio-fs is used internally solely for the Rosetta share. Three options:
(a) contribute a general virtiofs host-share to Hypeman;
(b) invert ownership — VM owns the FS, host accesses via the Oqto file API;
(c) serve workspace files over the Oqto endpoint bridge (no in-VM POSIX).
This is the same wall that got Firecracker rejected in the original microVM design doc.

### 2. Egress enforcement was inert here — **security gap, must close & prove**
On this host `br_netfilter` is **not loaded** and the TAPs are enslaved to bridge `vmbr0`.
The L3 `iptables FORWARD` REJECT rule on the TAP therefore never sees bridged L2 frames,
so **direct VM egress succeeded** while the MITM proxy listened and env was injected.
- Proxy: ✅ listening `10.100.0.1:18080`; env ✅ injected; rule ✅ installed; effect ❌ none.
- Also: a proxied request to `1.1.1.1` returned **502** from the proxy.
- Likely fix: `modprobe br_netfilter` + `net.bridge.bridge-nf-call-iptables=1` (a host
  prerequisite Hypeman neither loads nor checks), then re-verify direct egress is blocked.
- This is exactly the honesty gap the design docs warn about — **do not rely on it unproven.**

### 3. Firecracker exec-vsock timed out
`hypeman exec` over vsock failed on Firecracker ("is exec-agent running in guest?") but
worked on Cloud Hypervisor (the default). The FC path is less mature here.

### 4. Release-binary packaging + implicit host deps
- The Docker **builder image** self-build fails in the release (build context not bundled:
  `lib/guest/*`, `lib/system/guest_agent/`, `go.mod`). Affects only `/builds`, not the
  guest agent (initrd). Cosmetic for our use; indicates v0.2.0 maturity.
- Implicit host deps the `curl|bash` installer hides: `mkfs.erofs` (erofs-utils), Docker
  (builder), a VMM (auto-downloaded), root for bridge/TAP/iptables.

### 5. Maturity
v0.2.0, ~10 months old, 253★. Young for a security-critical isolation substrate — but
ADR-0020's pluggable-supervisor contract makes it one backend of many: safe to try and
safe to drop (swap to Kata or direct Cloud Hypervisor).

## How it maps to Oqto

- **Placement:** a `HypemanSupervisor: PlacementSupervisor` that calls Hypeman's remote
  API (`run`/`stop`/`standby`/`restore`/`exec`) to manage workspace VMs — a fourth
  backend behind ADR-0020's contract, advertising the strongest (separate-kernel) tier.
- **Idle suspend/resume:** closed by standby/restore (66 ms restore). This is the win.
- **Egress:** Hypeman's MITM proxy is a candidate VM-tier mediator, but enforcement must
  be closed (br_netfilter) and proven. ADR-0030's smokescreen remains the cleaner
  general-egress layer; the two compose (Hypeman gives the kernel boundary + standby,
  smokescreen/Hypeman-proxy gives domain egress).
- **Transport:** Hypeman's host↔guest channel is vsock — matches the Kata requirement
  noted earlier; the Oqto runner would live inside the VM and speak the canonical wire
  (ADR-0024) over vsock, just as it would over a Unix socket today.

## Benchmark: Kata 4.0.0 vs Hypeman v0.2.0 (same host, both on Cloud Hypervisor)

Kata static 4.0.0 via containerd/`ctr` + `configuration-clh.toml` (CH, guest kernel 6.18.35);
Hypeman with its auto-downloaded CH v51.1. Alpine image, n=5 (n=3 for sleep/wake).

| Metric | Kata | Hypeman | Notes |
|---|---|---|---|
| Cold start | **~0.9 s** (855–1057 ms, full run+teardown cycle) | ~3.3 s (create→running, image cached) | Kata ~3.7× faster cold; Hypeman pays overlay/config-disk prep |
| First-ever start | n/a (containerd pull) | 3.7 s (includes one-time OCI→erofs conversion) | per-image, cached after |
| Exec round-trip | 27–51 ms | 20–36 ms | comparable; both vsock agents |
| VMM RSS per idle guest | 161 MB (2 GB guest default) | 241 MB (512 MB guest) | not normalized; both modest |
| **Sleep** | — (no snapshot via ctr/shim path) | standby 314–325 ms | |
| **Wake** | **~0.9 s full re-boot** (only equivalent) | **restore 266–329 ms** (disk); **66–72 ms** measured earlier on tmpfs | restore is storage-bound |
| Idle footprint while "asleep" | full VMM RSS forever (no standby) | **0 RSS** — VMM stopped, 667 MB snapshot on disk | the density argument |

Readings:
- **Kata wins cold start** (~0.9 s vs ~3.3 s) and matches on exec. As a pure "stronger isolation
  for a long-lived workspace" play, Kata is leaner and much more battle-tested.
- **Hypeman wins on suspend/resume**: restore beats Kata's only wake path 3–10× (storage-dependent),
  and standby frees **all** guest memory — idle-density scales with disk, not RAM. Kata simply
  has no equivalent in the containerd path (no VM snapshot/resume).
- Restore latency is storage-bound: 66 ms on tmpfs vs ~300 ms on disk — production placement of
  the snapshot dir matters (NVMe / tmpfs tier).
- Kata integration cost is real: shim-v2 requires containerd (podman cannot drive Kata 4.x —
  the OCI CLI wrapper is gone), so "Kata as a podman runtime flag" is off the table; it would
  mean a containerd-based supervisor lane instead.

## Follow-up: virtiofs overhead measurement (2026-08-06)

Question: does Kata's virtio-fs host-directory sharing make a mounted live workspace tree
cheap in a VM? Measured on the same host, default Kata CLH virtiofs settings (no DAX tuning),
same dir benched natively and bind-mounted into a Kata guest (debian userland, 2 runs):

| Operation (8000 small files) | native | Kata virtiofs | overhead |
|---|---|---|---|
| create (write+mkdir+sync) | 6.5 s | ~50 s | **~7.7×** |
| stat-walk (find + ls -lR) | 40 ms | ~154 ms | ~3.9× |
| read-all (cold cache) | 65 ms | ~2.0 s | ~30× (native had warm page cache; cold-path gap is real) |

Reading: every metadata op crosses guest→virtio→virtiofsd→host, and agent workloads are
metadata-saturated (a typical `npm install` touches 50k–150k files → minutes of pure FS tax).
Interactive use (edit/browse/git status) is acceptable; bulk dependency/build work is not.
DAX + cache tuning can pull reads near-native and deserves one pass before a final verdict;
creates/writes fundamentally keep crossing the boundary. NFS/SMB in the same seat are worse.

Implication: a mounted live tree in a VM carries a real tax on exactly the operations agents
do most. The isolated-clone working-copy model (one-time clone/setup, then native block speed)
is the performance-honest default for the VM tier; live mounted trees remain the container
tier's strength (bind mounts are free). This weakens "Kata preserves the live tree cheaply"
as an argument in the supervisor comparison above.

## Next steps (if pursued)
1. Resolve the workspace-model fork (virtiofs contribution vs ownership-invert vs bridge).
2. Close egress enforcement: load `br_netfilter` + sysctl on a test host, re-run the
   direct-egress bypass test, confirm blocked; also diagnose the proxy 502.
3. Verify the secret-substitution path end-to-end (mock in guest → real at allowlisted host).
4. Prototype `HypemanSupervisor` behind the `PlacementSupervisor` trait.
