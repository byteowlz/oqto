# Isolation Tiers, Egress Capture, and Secret Mediation

Status: Accepted direction (from 2026-06-09 gondolin evaluation session)
Related issues: `oqto-pgwp` (reconcile epic closure), `oqto-h6hr` (egress capture), `oqto-0q7x` (tool-credential injection), `oqto-8v0y` (ssh-proxy), `oqto-430k` (microvm tier)
Supersedes nothing; extends `20260401-sandbox-v2-threat-model.md` with verified current state and a tiered enforcement model.

---

## 1) Verified current state (2026-06-09)

What is actually wired, as opposed to what config/docs claim:

| Capability | Status |
|---|---|
| bwrap + landlock + seccomp process sandbox | Live |
| EAVS LLM-credential mediation (virtual key in agent env; real provider keys host-side in eavs) | Live -- LLM traffic only |
| `NetworkMode::Open` / `NetworkMode::Isolated` | Live (binary: full network or none) |
| `NetworkMode::Proxy` (domain-filtered egress) | **Config enum only.** Only non-definition reference is a unit test (`oqto-sandbox/src/config.rs:2461`). `oqto-host/src/sandbox.rs` imports the type but never branches on it. `spawn.rs` emits no network args. |
| Tool-credential mediation (kyz-backed) | **Not implemented.** Zero kyz references in backend code. No injection engine. |
| `oqto-ssh-proxy` | **Orphaned.** Binary exists (`oqto/src/bin/oqto-ssh-proxy.rs`), compiles, nothing spawns it, absent from deploy scripts. |

Consequence: when a workspace has network access it has *unfiltered* network access, and any
non-eavs credential provisioned into an agent would sit in plaintext env, exfiltratable over any
outbound path. The sandbox-v2 epic (`oqto-6er8`) and its secret-mediation children are closed in
trx, but the deliverables are not in the tree -- see `oqto-pgwp`.

The one genuinely strong piece: the eavs pattern (agent holds a scoped virtual key; the proxy
holds real provider keys and injects them per allowlisted upstream) is exactly the right model.
The work below generalizes it; it does not replace it.

## 2) Threat framing: two independent boundaries

- **Boundary A -- at rest / who may request.** Where secrets live, policy, audit. Owner: **kyz**
  (age-encrypted vault, brokered JIT grants with TTL/use-count/command/workspace scoping,
  redaction-by-default IPC).
- **Boundary B -- runtime exposure.** Does the real secret ever enter the agent process (env,
  memory, fs)? Owner: **egress proxy with placeholder substitution** (the gondolin
  `createHttpHooks` model; eavs already implements it for LLM keys).

kyz alone does not solve B (`kyz exec --env` still puts the token in agent env). A proxy alone
does not solve A. They compose: kyz is the vault behind the runner/eavs egress injector.

## 3) Egress capture levels

Secret injection at the proxy is worthless unless agent egress is *forced through* the proxy.
Three levels, strictly ordered:

1. **Env hints (`HTTP_PROXY`)** -- voluntary compliance, trivially bypassed (`unset`). Not enforcement.
2. **netns + transparent redirect** -- agent confined to its own network namespace; single
   host-managed veth; host-side nftables DNATs all TCP to the transparent proxy; DNS forced to a
   controlled resolver; default-DROP everything else (UDP incl. QUIC, ICMP, raw); seccomp denies
   `AF_PACKET`/`SOCK_RAW`; no `CAP_NET_ADMIN`. Captures all egress from a confined process.
   **Bounded by the shared kernel**: a kernel LPE flushes the rules; a missed
   capability/namespace primitive leaks. Also: capture != inspection (TLS MITM needs a trusted
   CA; pinned clients fail closed or tunnel opaquely), and QUIC must be dropped to force TCP.
3. **microVM** -- enforcement lives on the host side of a hardware-virtualization boundary. The
   guest's only network path is the single virtio-net device the host owns. Holds even if the
   guest kernel is fully compromised. This is the only level at which "all egress goes through
   the injector" and kernel-LPE containment are structural guarantees rather than strong defaults.

oqto today is at level 0/1 (Open) or air-gapped (Isolated). `oqto-h6hr` builds level 2.
`oqto-430k` adds level 3 as an opt-in tier.

## 4) Tiered runtime model

| Tier | Mechanism | Kernel boundary | Availability |
|---|---|---|---|
| local | direct spawn | shared | dev only |
| runner (baseline) | bwrap + landlock + seccomp + level-2 egress | shared | everywhere Linux runs |
| container | + namespaces/OCI | shared | everywhere |
| **microvm** (new) | guest VM, **runner tier re-applied inside the guest** | **separate** | KVM-capable runners only |

Principles:

- **The shared-kernel tier stays the portable baseline.** KVM is not universal (AWS: `.metal`
  only; nested virt is opt-in/SKU-dependent elsewhere). The VM tier must never be a hard
  requirement for running oqto.
- **Capability negotiation, fail closed.** Runner probes `/dev/kvm` at startup and advertises
  the tier. Per-workspace policy (sandbox profile templates) may *require* `microvm`; if the
  assigned runner cannot provide it, refuse to run -- never silently downgrade.
- **Defense in depth composes.** A VM without seccomp/landlock inside is weaker than with them.
  The microvm tier wraps the runner tier; it does not replace it.
- **Routing, not fleet-wide KVM.** A subset of bare-metal/nested-virt runners serves
  high-sensitivity workspaces (fits the remote-runner roadmap); everything else stays on cheap
  shared-kernel runners.

## 5) Secret mediation architecture (target)

The gondolin invariant, adopted: **the agent never holds a real secret in env, memory, or fs.**

- Agent env carries random placeholders (`resolveSecretPlaceholder` pattern: high-entropy,
  non-overlapping, never equal to the value).
- The egress proxy substitutes placeholder -> real value **only** for that secret's allowlisted
  destination hosts, at the last moment after any request rewriting.
- Anti-exfil: requests already containing a *real* secret value headed to a non-allowlisted host
  (redirect hops) are blocked. Internal/metadata IP ranges blocked by default (SSRF).
- kyz is the backing vault (Boundary A): brokered JIT grants, TTL/use-count, audit log.
- **Scope honesty:** placeholder substitution works for HTTP(S)/WS only -- protocols with a
  rewritable credential slot. SSH gets its own leg (`oqto-ssh-proxy`: agent-socket proxying so
  private keys never enter the agent; `oqto-8v0y` decides wire-or-delete). DB wire protocols etc.
  need dedicated proxies or remain out of scope, explicitly.

Reference implementation to crib from: `external-repos/gondolin/host/src/http/hooks.ts`
(`createHttpHooks`, `applySecretsToRequest`, `assertSecretValuesAllowedForHost`).

## 6) microVM tier decisions (`oqto-430k`)

- **Granularity: per-user** (runner + that user's pi sessions inside one VM), not per-session.
  Scales with active users; maps onto the existing per-user runner.
- **Prototype: gondolin krun backend.** Fastest path to a working microVM with virtiofs
  workspace mounts and egress hooks already wired. Spike only -- v0.12, single-vendor,
  TS-host/Zig-guest, krun-on-x86_64 is CI-smoke-tested. Not a production dependency.
- **Production: Cloud Hypervisor** (Rust VMM, virtiofs, Kata's default backend), or Kata-on-CH
  if growing the `container` tier is preferable. **Not Firecracker**: it has no shared-filesystem
  support by deliberate design (virtio-block only); delivering workspaces as block images fights
  oqto's directory-mount model end to end.
- Control channel runner<->guest over vsock; oqto-sandbox runs inside the guest.

## 7) Sequencing

1. `oqto-pgwp` -- reconcile trx state with code reality; correct the threat-model doc's implied status.
2. `oqto-h6hr` -- level-2 egress capture (netns + transparent redirect + default-drop). The
   enforcement half; prerequisite for 3.
3. `oqto-0q7x` -- tool-credential placeholder injection (extend eavs into a general agent egress
   gateway, kyz-backed). Blocked by 2.
4. `oqto-8v0y` -- ssh-proxy: wire as the SSH leg or delete.
5. `oqto-430k` -- microvm tier (krun spike -> Cloud Hypervisor), making levels 2-3 a per-workspace choice.

Rationale for the order: a shared-kernel sandbox that honestly does what it claims is
defensible; claimed-but-unwired controls are not. Close the honesty gap first, then extend the
ceiling with the VM tier.
