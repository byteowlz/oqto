# Unified egress: enforcement tiers, traceability, and pluggable enforcers

## Status

Proposed (2026-08-06). Tracked by `oqto-gyqr.4`. App consumption of this egress and credential channel is proposed separately in [ADR-0046](0046-app-egress-credential-mediation-and-mcp-adapters.md).

Refines ADR-0007 (three-layer egress) and ADR-0019 (deny-all network + granted
endpoints). **Supersedes ADR-0030's enforcer decision**: the CONNECT proxy is
demoted from canonical enforcer to optional credential channel; transparent
capture at a placement-owned choke point becomes the canonical authorization
mechanism. ADR-0030's policy-model and pluggability decisions are retained and
extended here. Uses the capability-advertisement pattern from ADR-0020
(decision 6).

## Context

Agents need internet access most of the time; `--network=none` plus endpoint
sockets is airtight but unusable for real work (`oqto-bydk`). Three distinct
problems have been conflated in prior decisions:

1. **Authorization** — may this byte leave at all? Must bind a *malicious or
   non-cooperating* agent. We empirically proved cooperation cannot be assumed:
   Node's fetch (undici) silently ignores `HTTP_PROXY`/`HTTPS_PROXY` and dials
   direct, while curl/git/npm/pip/bun comply. A prompt-injected agent is an
   adversary, not a polite client.
2. **Credential mediation** — does this byte carry the right secret? Inherently
   cooperative: the client must name its destination so the host can attach a
   secret there. Bypass fails closed by construction — skipping the channel
   means *not getting* the secret, never leaking it.
3. **Traceability** — regardless of what is allowed, can we unequivocally say
   what a workspace talked to? A product requirement even under open egress.

ADR-0007 built transparent capture (`eavs` egress relay: PROXY-v2
original-destination + SNI corroboration + passthrough) and kept credentials
separate. ADR-0030 collapsed authorization and credentials into one CONNECT
proxy, correctly rejecting SNI *as authorization* but overreaching: a CONNECT
proxy only binds clients that use it. Both cannot be the canonical enforcer.

A second requirement: operators (notably Kubernetes shops) must be able to
replace the enforcement mechanism with their own (CNI egress gateway, service
mesh sidecar, corporate proxy) without forking Oqto's policy model.

Empirical inputs: the undici bypass; a microVM runtime whose egress controls
were fully configured yet non-enforcing because `br_netfilter` was not loaded
(bridged L2 traffic never hit the L3 rules) — configuration presence proved
nothing; and the observation that every placement type that can enforce
ultimately contains a Linux kernel Oqto (or the operator) controls.

## Decision

### 1. Separate the three problems

Authorization, credential mediation, and traceability are distinct layers with
distinct trust models. No single component may claim to solve all three; each
is specified, enforced, and verified independently. They compose at the same
choke point but never depend on each other's guarantees.

### 2. Enforcement tiers name what a malicious agent cannot do

Three tiers, strictly ordered. A tier is a falsifiable promise about a
*non-cooperating* client. There is no "cooperative" tier: a control that only
binds polite clients is `none` wearing a costume (see undici).

- **`none`** — no promise. Traffic leaves unfiltered. Exists so dev/local is
  honest instead of pretending. Native macOS/local-process placements are
  `none` today.
- **`transparent`** — there is no network path except through the enforcer.
  All egress crosses a choke point the enforcer owns: host-mediated DNS
  (non-allowlisted names do not resolve), destination-IP authorization from
  the actual original destination, default-drop for everything else. Client
  cooperation is irrelevant; raw sockets, DoH, and proxy-ignoring runtimes hit
  the same wall. Non-promise: the enforcement point shares a kernel with the
  workload; a kernel/netns escape defeats it. Covert channels through
  *allowed* destinations remain (an allowlist-scoping problem no tier fixes).
- **`isolated`** — `transparent`, plus the enforcement point lives outside the
  guest kernel (host-side TAP/VMM for microVM placements). A full guest-kernel
  compromise cannot reach the enforcer.

**Tier claims are proven by demonstrated denial, not configuration presence.**
The doctor/probe harness attempts a forbidden connection *from inside the
placement* and requires it to fail. This rule is a direct mechanism response
to the br_netfilter incident.

### 3. Orthogonal capabilities, advertised alongside the tier

- `credential_channel: bool` — a CONNECT endpoint with host-side secret
  substitution (ADR-0007 layer 2/3, `oqto-0q7x`). Available at any tier,
  including `none`, because its safety never depended on enforcement.
- `advisory_proxy: bool` — logging/accident-guard proxy for well-behaved
  clients. Carries no security claim, ever.
- `flow_log: none | ip | named` — traceability (decision 5).

### 4. Policy: one canonical model, three verdict modes, fail-closed matching

The backend owns one typed `WorkspaceEgressPolicy` per workspace (retained
from ADR-0030): granted endpoints (EAVS et al.) plus a domain/destination
policy with one of three verdict modes:

- **`allowlist`** — only named destinations resolve and connect.
- **`open_attributable`** — any destination is allowed *if the workspace
  obtained its IP through the host resolver*; connections to IPs the workspace
  never resolved are denied (or flagged, per policy). Free internet, but
  nothing untraceable: raw-IP beaconing and rogue-DoH become blocked or loud.
- **`open`** — everything allowed, everything logged. Requires an active
  choke point to be meaningful; on tier `none` it is labeled unenforced.

Policies additionally declare `minimum_tier` and required capabilities
(e.g. `flow_log: named`). Scheduling is fail-closed: a workspace places only
onto a runner advertising a satisfying tier/capability set. Downgrade is never
silent — it is a placement failure or an explicit, logged operator override.
Defaults: production `transparent`; dev profile `none`; untrusted-code profile
`isolated`.

Authorization basis, in order of authority: (1) host-mediated DNS — the
placement's only resolver is ours; allowlisted names resolve to real IPs,
everything else blackholes, and every resolution is logged per workspace;
(2) destination-IP authorization from the actual original destination against
the workspace's resolved set; (3) SNI/Host peeking is **corroboration only**
— never the primary basis (spoofable, dying with ECH). This answers
ADR-0030's valid objection to SNI-as-authorization without abandoning
transparent capture.

### 5. Traceability inherits the tier

The flow log records `(workspace, ts, dest ip:port, bytes in/out, verdict,
name-evidence)` per flow at the choke point. Attribution is structural — each
placement has its own netns/TAP, so a flow belongs to exactly one workspace
with no process-guessing.

Destination *naming* has three certainty levels: IP:port is always
unequivocal; names are unequivocal when obtained via host-mediated DNS
correlation (survives ECH — we issued the answer, we do not infer it); SNI is
corroboration. DNS logs alone are not a flow log (resolving ≠ connecting);
the trace pairs resolutions with connections.

An audit claim without a choke point is a lie: `flow_log` better than `none`
may only be advertised at tier `transparent` or above, and is verified by
demonstrated capture (probe generates a known flow, requires it in the log
with correct attribution). Flow events are durable in an append-only audit
stream in the backend database — not ad-hoc files.

### 6. Pluggable enforcers: Oqto owns the policy, not the mechanism

```text
EgressEnforcer
  tier()          -> none | transparent | isolated
  capabilities()  -> { credential_channel, advisory_proxy, flow_log }
  attestation()   -> self | operator
  apply(policy)   -> compile canonical policy to native config
  verify()        -> denial + capture probes; attestation evidence
  revoke()
```

Each implementation carries its own compiler from `WorkspaceEgressPolicy` to
its native form (nftables + resolver zone, NetworkPolicy + CNI gateway
config, sidecar config, corporate-proxy ACL). Attestation axis:

- **`self`** — Oqto's enforcer; Oqto configures it and can prove fail-closed
  end-to-end from its own code.
- **`operator`** — bring-your-own (k8s CNI, mesh sidecar, forced corporate
  proxy). The operator declares the tier; Oqto verifies wiring and artifacts
  and still runs the in-placement denial probe, but the standing guarantee is
  the operator's — the same trust class as "you run the node." An env-var-only
  corporate proxy is honestly tier `none` (with `advisory_proxy`); forced
  redirect through operator infrastructure is a real `transparent/operator`.

The probes are enforcer-independent by design: demonstrated denial and
demonstrated capture are evidence Oqto collects itself against anyone's
enforcer.

### 7. Reference implementation: the workspace netns is the unit

One insight unifies the placements: every placement type that can enforce
contains a Linux kernel we control, and all of them can attach to a network
namespace Oqto owns — bwrap joins it, podman joins it (`--net ns:<path>`,
replacing `--network=none` when a policy grants egress), a microVM's TAP is
enslaved into it. Build the enforcer once, inside that netns:

- **`oqto-egress` crate** (the durable value; unprivileged, unit-testable):
  policy model, verdict engine, per-enforcer compiler seam, the transparent
  relay (rehomed/extended from the eavs egress relay: original-destination
  authorization, DNS-correlation table for `open_attributable`, splice, flow
  events), the resolver, and the probe harness.
- **`oqto-egressd`** (the only privileged component; small, auditable,
  systemd-installed like the Podman setup step): owns netns/veth/nftables
  lifecycle under `CAP_NET_ADMIN`. Local-socket API:
  `create_netns / attach_info / apply / verify / revoke / subscribe_flows`.
  **No egressd on a node ⇒ the node advertises tier `none`** — fail-closed,
  dev keeps working, nothing pretends.
- **Backend**: `ReferenceEnforcer` as a thin client via the runner channel;
  audit stream in the DB; tier/capability advertisement on the existing
  runner-capability path.

EAVS is unaffected: model traffic keeps its bind-mounted Unix socket endpoint
and never enters the egress data path. The credential channel, when enabled,
is a second listener in the same netns. Mounted-filesystem traffic (e.g. a
future VM file share) is host-destined and out of scope for egress verdicts,
but must be explicitly classified so the storage path never entangles the
enforcement path.

### 8. Platform scope

The reference enforcer is Linux-only by construction (netns). Non-Linux hosts
advertise `none` for native placements. VM-backed placements on those hosts
(podman machine, microVM) contain a Linux boundary and run the reference
enforcer inside it at full tier. Native macOS enforcement via a Network
Extension (`NETransparentProxyProvider`) is a possible future `EgressEnforcer`
behind the same trait, explicitly out of scope.


## Amendment (2026-08-20): bought enforcer, no privileged daemon, protocol-aware policy

Three findings from spiking [iron-proxy](https://github.com/paradigmxyz/iron-proxy)
(Apache-2.0, discovered via paradigmxyz/centaur, which deploys it as its credential boundary)
and from testing unprivileged network namespaces. Decisions 1–6 stand unchanged; decision 7 and
the policy model change.

### A. The reference implementation shrinks to policy compilation plus attachment

iron-proxy already implements, and was measured doing, most of what `oqto-egress`/`oqto-egressd`
was specified to build: default-deny domain/CIDR allowlist, placeholder credentials swapped for
real secrets at egress and bound per destination, per-request structured audit naming the swapped
secret and its location, MITM with a supplied CA, and native WebSocket/SSE/HTTP2 streaming. It
runs unprivileged once its listeners are moved off :80/:443, and enforced correctly from inside
the real workspace image.

Oqto therefore does not build a relay, a resolver, or a flow-event pipeline. It keeps what the
`EgressEnforcer` contract already assigns it: the canonical policy, compilation to an enforcer's
native config, per-workspace lifecycle, capability advertisement, and audit correlation.

### B. No privileged daemon: enforcement by absence of network

The premise that a `CAP_NET_ADMIN` daemon must own netns/veth/nftables does not survive contact
with rootless placements. An unprivileged user can create a network namespace and install an
nftables default-drop policy inside it (verified), but cannot pin that namespace where rootless
Podman could join it without a host-visible bind mount, which needs root.

The better mechanism is the one this codebase already ships. A workspace runs with
`--network=none` and receives granted service endpoints as bind-mounted Unix sockets, bridged to
loopback TCP inside the placement (`endpoint_bridge`). Bridging the enforcer's tunnel listener in
this way makes it **the only route out — by absence of any network, not by filtering it**. There
is nothing to bypass, no capability to hold, and no daemon to run.

Open-ended access is preserved: the single bridged endpoint carries arbitrary HTTP destinations,
and the enforcer decides which are permitted. Agents keep broad internet access without Oqto
enumerating hosts.

`oqto-egressd` is therefore **not built**. A privileged component returns only if a placement
needs a filtered *network* rather than enumerated endpoints — the microVM TAP case — and is then
scoped to that tier alone.

### C. Policy is protocol-aware, because one enforcer does not see everything

iron-proxy handles HTTP, HTTPS, WebSocket, SSE, HTTP/2 and (separately) PostgreSQL. Anything
else it **closes**: its own integration test opens a tunnel to port 22, sends non-TLS bytes, and
asserts the connection is dropped without forwarding.

So a policy modelled only as destinations is wrong. `WorkspaceEgressPolicy` carries
**protocol/port**, and destinations are routed to the mechanism that can mediate them:

| Traffic | Mediated by |
|---|---|
| HTTP-shaped | the egress enforcer (allowlist, credential injection, audit) |
| SSH | the SSH agent proxy for credentials (ADR-0039), plus its own granted endpoint for reachability |
| other TCP | an explicit per-destination granted endpoint, or denied |

This also removes an implicit assumption in decision 4: an allowlisted *name* is not sufficient
authorisation, because the mechanism that enforces it differs by protocol. Making the enforcer
the only route out would otherwise have silently broken git-over-SSH.

### D. Deny resolved addresses, not just names

Adopt an upstream IP deny list into the policy model regardless of which enforcer is used: even
when a host is allowlisted, refuse to dial it when its *resolved* address falls in a denied CIDR.
This closes SSRF and DNS-rebinding, and cloud metadata (169.254.169.254, fd00:ec2::254,
fd20:ce::254) plus loopback must be denied by default. The spike hit this protection unprompted
when a test upstream resolved to `::1`. ADR-0035 as written named neither.


### E. Delegated fetch: an allowed destination's capabilities are part of the policy

Discovered after the main text: Prime Intellect documented a universal escape from "offline"
agent sandboxes. An agent with no egress except the inference API used the OpenAI Responses
API's `input_file.file_url` parameter to make the *provider* fetch arbitrary URLs and return the
content into context, and drove further model instances as sub-agents over the same channel.
The egress enforcer sees a legitimate request to an allowlisted destination; the second hop is
invisible at the network layer.

Oqto's topology has exactly this leg: workspace -> EAVS -> provider, with the agent controlling
request content and EAVS passing bodies through unmodified today.

Two rules follow:

1. **A destination's grant includes its delegated-fetch capabilities.** "May call
   api.openai.com" is not one permission; server-side tools (`web_search`, `web_fetch`) and
   remote-content parameters (`input_file`) are separate grants, default-denied, enforced by the
   inference gateway on request bodies -- network-layer enforcers cannot see them.
2. **An escape probe joins the conformance matrix**: send a request containing a remote-fetch
   parameter through the full stack (session -> gateway -> provider) and assert it is stripped or
   refused. Demonstrated denial applies to delegated fetch exactly as to egress.

The enforcement point is EAVS: stripping or policy-gating these parameters is model-domain
semantics per the division of responsibility in this ADR's companion work (capability endpoints,
ADR-0039). Tracked as `oqto-gyqr.8`.

## Consequences

- `oqto-egressd` is not built (Amendment B). The privileged-daemon consequence below applies
  only to a future microVM TAP tier.
- `WorkspaceEgressPolicy` carries protocol/port and an upstream deny-CIDR list (Amendments C, D).
- `WorkspaceEgressPolicy` also gates delegated fetch — an allowlisted service's server-side tools and remote-content parameters are separate grants, default-denied and enforced by the inference gateway (Amendment E).
- The reference enforcer is bought rather than built; `oqto-egress` shrinks to the policy model,
  its compiler, and the probe harness (Amendment A).
- `NetworkMode::Proxy` (config-only, never implemented) is deleted rather than
  implemented; policy + tier supersede it (`oqto-pgwp`).
- `oqto-bydk` (strict profile makes agents unusable) is resolved by design:
  strict becomes `allowlist` or `open_attributable` at tier `transparent`
  instead of total isolation.
- `oqto-h6hr` (bwrap netns capture) becomes "bwrap attaches to the egressd
  workspace netns" — same mechanism as containers, not a parallel one.
- Smokescreen evaluation (`oqto-gyqr.1`) is rescoped to the credential
  channel; it is no longer on the authorization path.
- The container tier's `--network=none` posture is unchanged for workspaces
  whose policy grants no egress.
- Two supervisor candidates for the microVM tier both plug their TAP into the
  workspace netns; neither VMM's own egress machinery is load-bearing.
- Operators can replace enforcement without forking policy; Oqto's durable
  value is the policy model, the wiring, and the probes.
- New host prerequisite checks join the doctor (egressd present and healthy,
  capabilities, tier-claim probes; `br_netfilter`-class checks for VM tiers).

## Rejected

- **A "cooperative" enforcement tier.** Binds only polite clients; equals
  `none` against the actual threat model. Its real contents were unbundled
  into `credential_channel` and `advisory_proxy`.
- **CONNECT proxy as canonical authorization** (ADR-0030's enforcer choice).
  Cannot bind non-cooperating clients; the undici bypass is a live
  counterexample in the default workspace runtime.
- **SNI/DNS inference as authorization basis.** Retained from ADR-0030's
  rejection; SNI survives only as corroboration.
- **Per-VMM egress machinery as the enforcement point.** Proven bypassable in
  practice (br_netfilter incident); the netns choke point makes it
  unnecessary.
- **A second in-house proxy for credentials.** The credential channel buys or
  reuses (smokescreen or equivalent) per ADR-0030's build/buy analysis.
