# Capability endpoints are placement-neutral

## Status

Proposed (2026-08-14). Tracked by `oqto-deer`.

Generalises the SSH agent proxy (`oqto-8v0y`) into a contract every
agent-facing capability follows. Companion to ADR-0035 (egress tiers) on the
action/credential side: ADR-0035 governs bytes, this governs capabilities.
Consumes ADR-0019 (granted endpoint sockets), ADR-0020 (pluggable supervisors),
and ADR-0024 (transport-neutral runner wire).

## Context

The SSH agent proxy shipped as a spawn inside `pi_manager`, i.e. inside
whichever runner starts the agent process. That works for bwrap and local
placements and cannot work for container or microVM placements, where the
runner lives inside the placement and the credential lives outside it. The
contract had been stated correctly during design ("an `SSH_AUTH_SOCK` endpoint
appears in the placement") and then not followed, because the session socket
directory was already at hand at the spawn site. Nothing in the codebase forced
the claim to be checked against the other placement types.

The same shape recurs for every capability Oqto will mediate: model access
(EAVS), credential injection, OAuth exchange, tool gatekeepers. Each will face
the same question, and each can get it wrong the same way.

A second failure occurred in the same work: a missing ssh-agent aborted session
creation. Treating a capability like a containment control denies service
without protecting anything -- a session without the capability is strictly
less privileged.

## Decision

### 1. One contract: capabilities arrive as endpoints

A capability is delivered to a workload as an **endpoint** — a socket the
placement can reach — plus environment naming it. Never as key material, and
never by code that runs inside the placement reaching outward.

The provider runs where the secret already is. The placement receives only a
mediated, filtered endpoint. This is the pattern EAVS and the exposed
fileserver/ttyd bridges already use; the SSH agent proxy is the third instance
and must not invent a fourth mechanism.

### 2. Endpoints are session-scoped or workspace-scoped, and the two must not
be confused

- **Workspace-scoped** (EAVS): one socket per placement, deliberately shared by
  every session in it.
- **Session-scoped** (SSH agent): one socket per session, bound only into that
  session's namespace. Binding a session-scoped endpoint through a shared
  endpoint directory silently converts grants into a union across sessions and
  must be rejected in review and by test.

### 3. Grant scope equals the innermost isolation applied to the session

A grant cannot be finer than the boundary that enforces it:

| Setup | Enforceable scope |
|---|---|
| bwrap per session (host) | per session |
| container + nested bwrap per session | per session |
| container, in-container sandbox disabled | per workspace |
| microVM, one VM per workspace | per workspace |

Everything inside one isolation boundary is one principal: same uid, same mount
namespace, same reachable sockets. Placements therefore **advertise** the scope
they can enforce, and a placement that cannot enforce per-session scope must not
claim it. Where workdirs sharing a placement declare different grants, that is a
configuration error and fails closed; grants are never silently unioned.

Practical consequence: two projects that must not share credentials must not
share a placement.

### 4. Remote placements ride the authenticated runner channel

Session-scoped endpoints for remote runners are carried over the mutually
authenticated transport from ADR-0024 (TLS/Iroh), never a new listening port and
never a raw socket exposed to a network. This adds no new attack surface: an
attacker who can use that channel already controls the runner.

Consequences accepted deliberately:

- The raw agent is never forwarded. SSH agent forwarding (`ssh -A`) exposes
  every key to the remote host and is forbidden.
- A remote placement loses the capability during a partition, because signing
  requires the key and the key stays home. Autonomy is a separate, advertised
  capability, not something obtained by relaxing credential handling.

**Deferred:** short-lived CA-issued certificates placed on a node are the only
sound way to keep a disconnected runner working, because they bound the exposure
by time and principal rather than by trust. They are not built now: they put key
material on remote nodes to solve a problem that only exists once autonomous
runners exist. The trigger to revisit is the first genuinely autonomous runner
profile.

### 5. Capabilities fail soft; containment fails closed

An unavailable capability degrades to *not having it*, with a warning naming
what is missing. An unavailable containment control refuses to start the
workload. A failure must never widen a grant, materialise a secret as a
fallback, or substitute a broader credential.

### 6. Providers are found by configuration, not convention

Upstream provider locations are configured explicitly, with any well-known path
as a fallback only. `$XDG_RUNTIME_DIR/ssh-agent.socket` is an
openssh-on-systemd convention: Arch ships it, Debian/Ubuntu desktops commonly
expose gnome-keyring elsewhere, macOS uses launchd, and headless multi-user
hosts have no login agent at all. Guessing produces silent capability loss on
every platform that differs.

### 7. A capability is not done until the placement matrix is proven

Every agent-facing capability carries a conformance test across local, bwrap and
container placements (microVM and remote when they exist). A capability that
cannot state how it reaches a container is incomplete. This is the mechanism
that would have caught the original defect, and it generalises to the credential
channel (ADR-0035) and the capability broker.

## Consequences

- The SSH agent proxy moves host-side and is delivered as a session-scoped
  granted endpoint; the current in-runner spawn becomes the bwrap/local case of
  the general mechanism.
- The container doctor gains a probe for nested unprivileged user namespaces
  (`userns.nested`): it runs bwrap inside the configured workspace image. This
  succeeds on Arch with Podman 6.0 and `userns=auto` (verified 2026-08-14), so
  container placements can enforce per-session scope there — but it depends on
  host kernel policy, which is why the scope is probed rather than assumed. A
  host that fails the probe advertises per-workspace scope and still runs
  placements; the check warns rather than blocking.
- `[ssh]` configuration gains an explicit upstream socket; the well-known path
  remains a fallback.
- Endpoint kind (session- or workspace-scoped) becomes part of how an endpoint
  is declared, so the distinction is enforced rather than remembered.
- Remote support requires no new listener, only multiplexing over the existing
  runner channel.

## Rejected

- **Spawning capability providers inside the placement.** Cannot reach
  host-held secrets, and inverts the trust direction for containers and VMs.
- **SSH agent forwarding.** Grants every key to the remote host, defeating the
  grant model.
- **A dedicated network listener for agent protocols.** An authenticated
  channel already exists; a second one is unnecessary attack surface, and an
  unauthenticated one is a remote signing oracle.
- **Silently unioning grants across workdirs in a shared placement.** Lets a
  workdir with no grants inherit a neighbour's credentials.
- **Copying keys to remote nodes for availability.** Trades the property the
  design exists to protect; short-lived certificates are the bounded form, and
  are deferred rather than approximated.
