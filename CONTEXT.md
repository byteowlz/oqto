# Oqto

Self-hosted platform for managing AI coding agents. This file defines the canonical language of the project — terms only, no implementation.

## Language

**Runner**:
The sole execution interface of the platform. All agent work — spawning harnesses, translating native events, persisting history — happens inside a runner; the backend only ever speaks to runners.
_Avoid_: daemon, agent host

**Placement**:
Where a runner lives: the local host, a remote machine, a container, or a pod. A deployment concern, invisible to the protocol — a runner behaves identically regardless of placement.
_Avoid_: runtime mode (the backend-level `local`/`runner`/`container` distinction is dead; see ADR-0001)

**Placement Supervisor**:
Whatever owns a runner's lifecycle at its placement: systemd locally, k8s for pods, a remote machine's init. The backend never supervises runners it routes to (see ADR-0002).

**Control Plane**:
The standalone authority for fleet state: which runners exist, what agents run where, health, capabilities. Slow-path only — it answers "where," then clients talk to runners directly (see ADR-0003).
_Avoid_: event bus (transport, not truth), monitoring (observation, not authority)

**Account**:
A platform identity in Oqto — who a person is to the product (roles, auth, API keys, ownership). Distinct from the OS principal they run as.
_Avoid_: user (ambiguous — collides with the OS principal)

**Principal**:
The OS-level identity an agent's processes run as (a Linux uid). Created at provisioning time, never the same thing as an Account; one Account maps to one Principal in multi-user deployments.
_Avoid_: user, linux user (use Principal when the OS identity is meant)

**Workspace**:
A named collection of work directories, owned by one Account (personal) or shared across Accounts (shared). The whole — the unit an Account opens and the unit of placement isolation.
_Avoid_: project; and note the code's current `workspace_path` / `SharedWorkspace.path` actually name a work directory / Workspace respectively — a pending rename.

**Work directory**:
The directory a harness process runs in and loads its `AGENTS.md` from — the unit carrying a single agent persona. A Workspace contains many; each can run its own agent.
_Avoid_: workspace (that is the whole), cwd, repo.

**Harness**:
An agent runtime a runner can spawn. The runner translates the harness's native protocol into the canonical protocol; nothing outside the runner sees harness-native messages. Pi is the only first-class harness (native full-fidelity translator); all others attach via bridges (ACP or their own interface) and advertise their capability set per session (see ADR-0006).
_Avoid_: agent (ambiguous), backend (wrong layer)

**Isolation Tier**:
The level of network/process containment a session runs under: open, level-2 (captured egress — netns redirect to the eavs egress firewall + domain ACL), level-3 (microVM). A runner-side per-session policy, advertised as a placement capability, not a backend runtime mode (see ADR-0001, ADR-0007).
_Avoid_: sandbox profile (that is the bwrap/landlock file-access config, a different axis), network mode

**Session**:
One durable harness conversation identified publicly by one Oqto session id. A Session may contain an in-session entry tree, but remains one Session until explicitly forked.
_Avoid_: conversation (ambiguous), process (one Session may have many runtime incarnations)

**Fork**:
A hard-copy operation that copies one selected root-to-entry path from a parent Session into a new, independent child Session with its own Oqto and harness identities. Fork provenance is immutable and child Sessions appear beneath their parent in Session listings.
_Avoid_: branch (a Branch remains inside one Session)

**Branch**:
One root-to-leaf path inside a Session's entry tree. Changing Branch changes the active leaf without creating a Session.
_Avoid_: fork, child session

**Session Tree**:
The complete in-session tree of harness entries formed by stable entry ids and parent-entry links. Distinct from Fork lineage, which relates separate Sessions.
_Avoid_: session hierarchy (that means Fork lineage in Session listings)

**Canonical Protocol**:
The harness-agnostic message/event/command format spoken between frontend, backend, and runner. Messages are durable; events are ephemeral UI signals; commands flow from frontend toward runners.
