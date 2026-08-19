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

**Deployment Target**:
A named destination for operator actions: one Oqto Control Plane and its managed placements. The CLI's `--host` value names a Deployment Target, not a placement machine or SSH host.
_Avoid_: host (reserved for the App capability contract), server (ambiguous)

**Operator Identity**:
The authenticated actor using the Operator API: either a human Account or an automation Service Identity. Authentication evidence identifies it; capabilities and resource scopes separately authorize it.
_Avoid_: Principal (that is an OS identity), admin (a capability bundle, not an identity)

**Service Identity**:
A non-human Operator Identity for automation, with explicitly bounded capabilities and resource scopes. It never borrows a human login or claims human MFA.
_Avoid_: service account (ambiguous with Account), bot user

**Authentication Assurance**:
The verified strength, methods, issuer, and freshness of an authentication event. Authorization may require a stronger or more recent assurance for sensitive actions.
_Avoid_: `mfa=true` (loses method, strength, and freshness)

**Enrollment**:
A single-use, time-bounded delegation that lets a new Operator Identity bind its own authentication credentials up to a fixed authority ceiling. It is not a reusable login credential.
_Avoid_: invite (ambiguous with Workspace membership invitation), bootstrap token

**Principal**:
The OS-level identity an agent's processes run as (a Linux uid). Created at provisioning time, never the same thing as an Account; one Account maps to one Principal in multi-user deployments.
_Avoid_: user, linux user (use Principal when the OS identity is meant)

**Workspace**:
A named collection of work directories, owned by one Account (personal) or shared across Accounts (shared). The whole — the unit an Account opens and the unit of placement isolation.
_Avoid_: project; and note the code's current `workspace_path` / `SharedWorkspace.path` actually name a work directory / Workspace respectively — a pending rename.

**Work directory**:
The directory a harness process runs in and loads its `AGENTS.md` from — the unit carrying a single agent persona. A Workspace contains many; each can run its own agent. Oqto identifies it publicly with one stable work-directory id; host paths and mount sources are changeable binding facts, not public identity.
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

**Session Changeset**:
The durable set of file changes one Session made, with diffs anchored to the content that existed when that Session edited it. Projected from `oqto-log` tool-call parts and raw envelopes; never recomputed against current file content (see ADR-0033).
_Avoid_: dirty files (that is transient File Activity), commit (a Changeset may span or contain none)

**File Activity**:
The transient live view of what just changed in a work directory, joining runner watch events (which prove a change but name no actor) with the tool calls that claim a path. Changes no tool call claims stay unattributed rather than being assigned to the focused Session. Safe to lose on reload because the durable answer lives in the Session Changeset.
_Avoid_: changed flag (implies one shared mutable field across Sessions), session file events (the watcher cannot see which Session wrote)

**Supersession**:
The relation recording that a later Session changed what an earlier Session wrote. The earlier Changeset stays historically true and is additionally marked superseded; history is never rewritten (see ADR-0033).
_Avoid_: stacked diff (implies a linear rebased stack; Sessions interleave), overwrite

**Canonical Protocol**:
The harness-agnostic message/event/command format spoken between frontend, backend, and runner. Messages are durable; events are ephemeral UI signals; commands flow from frontend toward runners.

**Surface**:
A top-level section of the Oqto shell itself — routed, role-gated, build-time linked, and trusted. Surfaces have no capability boundary because they *are* the shell (see ADR-0027).
_Avoid_: app (a Surface is not installable, shareable, or sandboxed)

**OqtoUI**:
The platform-neutral user-interface contract for Oqto, implemented by web/React today and potentially native desktop or mobile clients later. It hosts rearrangeable Views over the same canonical protocol and domain state; it is not the name of one framework implementation or one fixed screen composition.
_Avoid_: Workbench (retired UI concept), frontend (too implementation-specific), app (an App may provide Views inside OqtoUI)

**View**:
One presentation instance hosted by OqtoUI, with stable identity and an explicit deployment, Account, Workspace, work-directory, Session, or resource owner. Docking, resizing, or focusing a View changes presentation only and never changes its owner, data binding, or authority.
_Avoid_: panel (only one possible placement), Surface (trusted top-level shell section), App (an App may provide one or more Views)

**Customization**:
User- or Agent-authored configuration that arranges and binds the cockpit — layouts, Slots, Menus, Bindings, Pickers, theme roles — expressed as declarative data, optionally produced by sandboxed Lua, and applied in precedence layers (dist Preset, deployment, user, grant-gated workspace, ephemeral). A Customization arranges and binds; it never confers authority, computes content, or touches stores (ADR-0040).
_Avoid_: plugin (that is the Apps or Runtime Add-on plane), settings (too narrow), script (the contract is the data, not the code)

**Preset**:
A shipped, immutable Customization that defines a complete cockpit arrangement (classic split, corner mode, big picture). The first-party UI is itself a Preset on the public primitives; Presets are readable and forkable, never privileged code paths.

**Provider**:
A streaming, capability-gated list source (Sessions, work directories, file listings, message search, App-contributed, sandbox-tool-backed) declared with its version in a queryable catalog. Configuration binds semantic Provider IDs; the catalog resolves availability; absence degrades along declared fallbacks and is reported, never discovered by failure.
_Avoid_: binary/tool (an implementation detail behind the catalog), source (ambiguous)

**App**:
An installable, shareable UI capability that reaches everything outside itself through the Host contract and may provide native-declarative and sandboxed-web presentations. Keep its facts independent: the Definition (versioned, content-addressed bundle + manifest), an Installation (availability and provenance under one owner), an Instance (one durable/logical use), its binding (explicit durable data owner/resource), and any local App Views.
_Avoid_: mini-app (the SDK name, not the domain term), plugin, Surface, global app (name deployment/Account/Workspace/work-directory availability and binding explicitly)

**Host**:
The implementation of the capability contract an App is handed — files, kv, theme, notifications, egress, agent. The only surface through which an App reaches the outside world; a standalone App is one backed by a local or mock Host.
_Avoid_: apphost (that names one transport, not the contract)

**Bridge**:
A transport carrying the Host contract across an isolation boundary, typically `postMessage` to an iframe. Transport only — it never defines its own app-facing API (see ADR-0027).

**Gate**:
The runner-side enforcement point deciding whether an App Instance may exercise a server-affecting granted capability for its binding and acting Account/Operator Identity; execution must additionally occur under the mapped target Principal. Host-local presentation capabilities are separately allowlisted by the client Host and cannot confer server authority. Enforced by authorization plus uid separation inside the Pod, never by network reachability — a Workspace Pod's shared netns is not a boundary.
_Avoid_: permission check (the Gate is one place, not a scattered pattern)

**App Sidecar**:
A server-side component of an App, running as a container in the Workspace Pod alongside the runner (ADR-0019). Carries an App's heavy or native compute; not required, and not available to every trust tier.
_Avoid_: app server, service (a Sidecar is per-Workspace, not shared infra)
