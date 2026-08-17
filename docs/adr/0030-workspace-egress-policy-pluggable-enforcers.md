# Workspace egress policy: one model, pluggable enforcers, bought proxies

## Status

Proposed (2026-07-29). Builds on ADR-0019 (deny-all network + granted endpoint
sockets) and the EAVS trusted-endpoint work. Tracked by `oqto-whgd`.

**Partially superseded by ADR-0035 (2026-08-06):** the enforcer decision
(CONNECT proxy as canonical enforcement) is superseded — transparent capture
at a placement-owned netns choke point is canonical authorization, and the
CONNECT proxy is demoted to the optional credential channel. The policy-model
and pluggable-enforcer decisions here are retained and extended by ADR-0035.

## Context

Container placements ship with `--network=none` plus explicitly granted Unix
socket endpoints. That is airtight but all-or-nothing: it can express "this
Workspace may reach EAVS" but not "this Workspace may reach `*.github.com`".
Domain-level egress cannot be done honestly with packet filtering: nftables has
no process identity, DNS-based allowlisting is bypassable by the client, and
SNI inspection is spoofable and dying with Encrypted ClientHello. The only
mechanism where a domain name is *authorized* rather than *inferred* is a
forward proxy with CONNECT semantics: the client names the destination, and the
proxy — outside the sandbox — decides and dials.

Oqto's future topologies (single host, multi-node, k8s, remote runners) need
the same primitive with different enforcement mechanics.

## Decision

### 1. One canonical policy model, owned by the backend

The backend holds one typed egress truth per Workspace:

```text
WorkspaceEgressPolicy {
  endpoints: [named service grants]       # existing (EAVS, etc.)
  allow_domains: [exact + wildcard hosts] # from workspace grants + admin policy
}
```

Workspace `.oqto/sandbox.toml` may tighten this; only global/admin policy may
grant. Policy compiles to whatever the placement's enforcer consumes. The
model, not any enforcer config, is the audit/authorization authority.

### 2. Enforcement is a per-placement adapter ("policy engine → enforcer")

```text
backend (canonical policy)
  ├─ podman/local: network=none + endpoint sockets
  │                 + /run/oqto/endpoints/egress.sock → CONNECT proxy per node
  ├─ k8s:          CiliumNetworkPolicy (toFQDNs/toEndpoints) compiled per Workspace
  └─ fallback:     sidecar CONNECT proxy where no network-layer enforcer exists
```

Identity is placement-native: for podman placements, *which socket a workspace
can reach* is the credential (one egress listener per Workspace); for k8s, the
Pod's network identity. Enforcement is fail-closed: no policy delivered means
no dial. Placements advertise their enforcement tier (endpoints-only,
proxy-domains, cilium-domains) rather than claiming uniform guarantees; the
advertised guarantee is the intersection semantics "egress restricted to the
allowlist", not enforcer detail (a CONNECT proxy authorizes names at request
time; Cilium authorizes DNS-derived IPs — both acceptable, not identical).

### 3. Buy the proxy, own the policy

We do not write a proxy. The adversarial surface (HTTP/CONNECT parsing,
connection handling, SSRF/private-IP hardening) is commodity; the
workspace→policy mapping is the part we own either way.

- **Primary: smokescreen** (Stripe). Purpose-built for egress control of
  untrusted code: CONNECT, deny-by-default domain ACLs, private-IP/SSRF
  blocking, decision logging, single Go binary. Client identity comes from the
  per-Workspace Unix listener (small fork/shim if upstream cannot map
  listener→role directly).
- **Growth path: Envoy + xDS** if Oqto becomes a cluster platform: one Envoy
  per node, one listener per Workspace, backend as xDS control plane pushing
  per-Workspace policy dynamically. The policy model carries over unchanged.
- **Documented fallback: HAProxy** if operational simplicity outweighs dynamic
  policy sophistication (runtime ACL API, first-class Unix listeners, but no
  xDS equivalent and awkward per-tenant dynamics).

Standard `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` env vars wire git, cargo, curl,
npm, and pip to the endpoint with zero client changes.

### 4. EAVS is not replaced

An egress proxy authorizes *connections*; EAVS authorizes *model usage* and
mediates credentials (provider master keys never enter workspaces; virtual
keys are per-Workspace, revocable, metered). Model traffic uses the EAVS
endpoint and needs no general egress. Allowing provider domains through the
egress proxy instead would put raw provider keys in workspace volumes and lose
per-tenant revocation/metering; rejected.

## Rejected alternatives

- **Write our own proxy**: security-critical parser surface for zero policy
  benefit.
- **nftables/DNS/SNI classification**: privileged, bypassable, or dishonest
  (reachability inference is not authorization; see EAVS transparent-egress
  fix).
- **Cilium as the primary/local enforcer**: wrong first implementation — hard
  hostname policy outside k8s, poor request-level audit, heavy operational
  cost; it is the natural *compiler target* for k8s provisions instead.
- **OPA/Rego as the policy layer now**: introduces a second policy language
  into the trust chain while our policy is one typed structure; revisit only
  if policy complexity outgrows the canonical model.
- **Squid/tinyproxy/NGINX/Caddy/Traefik/mitmproxy/ghostunnel**: mature tools
  that miss the threat model (hostile tenants, dynamic per-workspace identity,
  CONNECT-first, control-plane-driven policy).

## Consequences

- A new per-node daemon (smokescreen) joins the deployment surface, managed
  and configured by Oqto; its decision logs feed workspace-attributed audit.
- `sandbox.toml` `allow_domains` finally gains a real enforcement point;
  config/docs must state the tier honestly per placement.
- Current deployments remain valid: deny-all + EAVS endpoint stays a shippable
  posture; domain egress is additive, not a blocker for first container VPS
  rollouts.
- Isolation-tier advertising (oqto-nppq.4/.11) must include the egress tier.
