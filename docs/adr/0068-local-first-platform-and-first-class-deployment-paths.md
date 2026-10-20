# ADR-0068: One local-first platform with equally first-class deployment paths

Status: Accepted architectural direction (2026-10-20); implementation and deployment qualification remain separate. Tracked by `oqto-mey5`.

## Context

Oqto should serve a solo developer starting on a laptop, a family starting with a home server, and an enterprise deploying for an entire team. Each is a legitimate starting point and a worthwhile destination. Users should be able to add devices, people, storage and execution capacity without abandoning their Sessions or adopting a different product.

Making a central server mandatory weakens the personal/offline experience. Making Kubernetes mandatory burdens small installations. Treating team deployment as a larger personal installation weakens authorization, isolation and operations. Building three separate products duplicates domain logic and creates incompatible state. We choose one platform with deployment-independent contracts and deliberately different operational profiles.

## Decision

### 1. Each path must be good in its own right

All three deployment profiles are first-class product experiences:

| Profile | Starting experience | Growth without starting over |
| --- | --- | --- |
| Local | Install on a laptop and do useful work without an Oqto server or cluster. With a suitable installed local model and dependencies, work can continue offline. | Optionally enroll other devices, add a server, sync permitted state and use remote execution. Remaining local is equally valid. |
| Appliance | Install a dedicated home/small-team server from a turnkey installer ISO, with guided provisioning, storage, networking, updates and recovery. | Add family members, personal devices, GPUs or further execution hosts when needed. A single home server remains a supported destination. |
| Operator-managed | Deploy into existing Linux/KVM or Kubernetes infrastructure, integrating supported identity, storage, model and credential services. | Expand team capacity and infrastructure through the same domain and capability contracts, without adopting the appliance's host layout. |

**The laptop is not a demo, the family server is not an enterprise deployment exercise, and the enterprise installation is not a personal installation with more users.** None is a deliberately degraded funnel to another profile. Family members need intelligible private/shared Workspace boundaries; solo developers need low overhead and ownership of their data; enterprises need enforceable policy, auditability and predictable operations.

Equal quality does not mean identical hardware requirements or every capability on every target. Each supported profile must have a coherent onboarding, daily-use, diagnostics, update, export and recovery story. Capability differences must be discoverable before an action, with actionable unsupported results rather than silent weakening or a broken workflow. Product acceptance must exercise each journey, not infer quality from shared code.

A household can start with a home server without first creating a laptop installation. An enterprise can start directly on its own infrastructure. Growth is additive, not a mandatory laptop-to-appliance-to-cluster migration ladder.

### 2. Deployment does not define the domain

Retain the existing domain language in `CONTEXT.md`: Workspace, work directory, Session, Runner, Placement and Placement Supervisor. Deployment profile is not a backend runtime mode, a tenancy identity or a security tier.

- Agents, Sessions, retained work and execution environments have independent lifetimes. A persistent workstation is supported as a design target, not a required one-machine-per-Agent mapping.
- All platform execution continues through Runners, per [ADR-0001](0001-runner-is-the-sole-execution-interface.md). Runners may still start and supervise Pi. The backend routes; an infrastructure adapter does not become a second Session supervisor or history writer.
- Reuse the Placement Supervisor and existing operator/provider seams for provisioning, start/stop, health, reachability, resource attachment and advertised capabilities. Do not create a parallel execution or authorization path for the ISO or Kubernetes.
- Preserve public Session identity across reconnect, added devices and placement changes. Harness-native identities remain provenance-bearing bindings, not replacement public IDs.
- The CLI, OqtoUI/native clients and headless integrations consume the same semantic contracts. No deployment is usable only through a proprietary or web-only management surface.

### 3. Appliance and Kubernetes are substrates, not competing products

The appliance is an opinionated integrated distribution: supported Linux, virtualization, storage, networking and Oqto components configured together. It is not a new general-purpose hypervisor project. Reuse existing virtualization and orchestration where they meet our requirements.

A candidate appliance layout has a control-plane VM, storage and trusted-service boundaries, persistent Workspace VMs and disposable microVMs. This is not a mandated VM count or final topology. Separate services according to trust, resource and recovery needs, not one VM per executable; small machines must not require the full fleet layout. Keep credential custodians/brokers outside untrusted agent workloads. Hosting services in separate VMs on one machine does not remove trust in that host's administrator.

Prefer host-level management of ordinary VMs and microVMs over requiring nested virtualization. Desktop environments and GPU workloads remain legitimate execution requirements; a headless microVM is not automatically their replacement.

Kubernetes is a first-class operator-managed target, not a dependency for local or appliance use. A Pod is not automatically a VM security boundary. VM/microVM and GPU support require qualified integrations, hardware and policy; Kubernetes installation alone does not establish those guarantees. Existing databases, object storage, identity and model gateways should be consumable through explicit supported contracts, without requiring Oqto-managed service VMs.

This ADR selects no hypervisor, host distribution, storage engine, Kubernetes integration or external platform. Overalls and similar systems remain integration/reuse candidates, not dependencies. Distribution choices must include a license and redistribution review of the complete shipped stack.

### 4. Local-first state and optional sync enable outward growth

Sync is optional, policy-controlled replication of authorized portable state, not a permanent connection requirement or transfer of a live process. Administrators may prohibit syncing managed data or enrolling devices. Adding a server must not automatically upload all personal data, credentials or configuration; scope and ownership remain explicit.

Offline continuation requires locally available history/context, selected memory, project bytes, model, tools and permissions. Remote-only capabilities must report unavailable. A model switch restores usable Session context, not another model's internal state or KV cache. A sync grant is not a resource grant, and cached authorization cannot create fresh remote authority.

Disconnected devices can diverge. Preserve stable event identities, ancestry and provenance, and expose conflicting continuations explicitly. Never reconcile history by text, visible order or message counts, silently merge divergent conversations, or automatically replay external effects. The precise in-Session Branch versus explicit Fork treatment needs a follow-up sync contract consistent with [ADR-0063](0063-native-session-graph-convergence.md).

Runners and `oqto-log` remain the execution and canonical-history authorities. Sync must go through an authenticated, store-owned replication/import contract with idempotence and conflict checks, not copy live SQLite/WAL files or add ad-hoc database writers. Each replica's SQLite remains single-writer-local. Native Pi JSONL remains Pi-owned; Oqto must not fabricate or rewrite native files to simulate portability. No global exclusive-writer guarantee is claimed while authorized devices are disconnected.

Optional client-side encryption is a design requirement, including independently controlled Session keys and explicit multi-user sharing/recovery policy. This is not a selected cryptographic protocol. A runner/model receiving plaintext is inside the trust boundary; server-held automatic decryption is not privacy from that server. Revocation cannot erase history, keys or plaintext already downloaded. Key custody, sync protocol, recovery, metadata exposure and enterprise retention require separate detailed decisions and qualification.

### 5. One lifecycle and recovery discipline across profiles

Build installers, appliance images and operator-managed artifacts from shared versioned configuration, release manifests and domain validation. Reuse the [ADR-0018](0018-single-install-update-deploy-engine.md) acquisition/activation contract and preserve-first semantics; an ISO is not permission to invent a second Oqto installer. Infrastructure-native mechanisms may perform deployment, but must satisfy the same artifact, migration, health and rollback requirements rather than blindly executing host activation inside Pods.

The appliance must expose its actual topology, capacity, effective isolation, health, backup state and recovery procedures. Safe disk selection, interrupted provisioning, failed updates and rollback are core product work, not post-ISO polish. Applying a profile must not overwrite existing user-owned configuration or adopt unrelated services implicitly.

Object storage is appropriate for artifacts and backups; it is not a universal replacement for Workspace files, VM disks or local SQLite. Co-located storage is not an independent backup. Define application-consistent backup/restore, retained dirty work and explicit publish/conflict receipts. Restoring data does not restore the external world, and adding a second node is not proof of high availability.

## Relationship to existing decisions

- [ADR-0019](0019-container-per-workspace-placement.md) and [ADR-0020](0020-portability-oci-substrate-pluggable-supervisor.md): retain pluggable supervision, explicit tenancy, rooted resources and single-writer-local storage. Amend the universal container/Pod wording: OCI is an important provider, not a prerequisite for every supported environment. Standalone local use, full VMs and microVMs are first-class candidates; unsupported isolation must still fail explicitly. The existing container-per-Workspace mapping remains the contract for that provider, not a requirement to redesign its implementation here.
- [ADR-0023](0023-one-public-oqto-session-id-harness-ids-adapter-local.md): retain one public Oqto Session ID and distinct native bindings. Amend the backend-only minting assumption: an authorized standalone local Session authority must be able to mint stable public IDs without a central backend. Enrollment/sync must preserve them and reject identity collisions or incompatible provenance, not remint or infer ownership from an ID. Wire/storage mechanics remain follow-up work.
- [ADR-0062](0062-backendless-personal-host-device-pairing.md): preserve backendless personal pairing and independent grants. Pairing is not permission to sync all state, export keys, execute tools or control another Session.
- ADR-0018's coherent artifact and activation rules remain authoritative. Infrastructure independence does not create a bypass around release or provisioning gates.

## Acceptance required before claiming a profile works

| Journey | Required evidence on supported targets |
| --- | --- |
| Solo developer | Fresh laptop install, useful local Session, pre-provisioned offline local-model continuation, restart/reload recovery, export/restore, and optional server enrollment without identity loss, duplicate execution or unsolicited data upload. Disconnecting the server must not disable locally authorized work. |
| Family/home server | Fresh appliance installation, separate private Workspaces and explicitly shared work, device enrollment/revocation, bounded resources, comprehensible update/backup/restore and disk/network failure diagnostics. Membership or home-network access must not grant access to another member's private data. |
| Enterprise/team | Installation into supported existing Linux/KVM and Kubernetes targets, organizational identity/policy integration, enforceable sync/egress restrictions, tenant isolation, scoped automation/audit, resource governance and qualified upgrade/restore. No compulsory appliance takeover or mandatory locally managed copy of an existing supported service. |
| Growth and failure | Test adding a server/device/host, selective state transfer, unavailable capabilities, revoked grants, interrupted sync, offline divergence, storage loss and failed updates. Preserve stable IDs, retained bytes and auditable effect status; reject unsafe replay and policy violations. |

Record supported hardware/OS/runtime combinations, resource requirements and measured onboarding/operation/recovery results for each profile. Passing another profile's tests or merely rendering a deployment manifest is not acceptance. This ADR does not assert that these journeys currently pass.

## Sequencing and non-goals

Prove the platform and deployment contracts on supported laptop and ordinary Linux installations, validate another provider early, then package the proven system as an appliance ISO and qualify operator-managed targets. Sequencing is not permission to make later paths architecturally second-class.

This is a direction and boundary decision, not authorization to provision hosts, repartition disks, deploy Kubernetes, access private state, implement sync, publish an ISO or choose a distributed consensus/crypto system. Detailed sync, offline authorization, key management, environment-provider and appliance recovery contracts require follow-up designs and evidence. No automatic HA, transparent process migration, universal exactly-once effects or production-readiness claim follows.
