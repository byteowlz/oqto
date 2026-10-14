# Adaptive capability gates and replaceable executors

## Status

Proposed implementation contract (2026-10-12), `oqto-pdbs` / `oqto-g7rk`. User approved the prototype direction, not production activation. Extends ADR-0001, ADR-0023, ADR-0039 and ADR-0048. ADR-0051's proposed seccomp notification work is not a dependency.

## Decision

Keep Pi Durable orchestration in a stable, narrow brain environment. History, mmry and trx operations cross trusted capability providers; arbitrary code, files, shell and child processes cross confined executors. A permission change updates protected authority, not agent-editable configuration. Workspace/global policy remains a ceiling: approval cannot turn a hard denial into an allow.

Build `oq-permissions` as a host-neutral Rust crate in the existing backend workspace. It owns the permission lifecycle and its protected ledger API, not Oqto Account authentication, history, memory, issue storage, or OS confinement. The owning runner supplies verified binding and approval identity, operates the ledger, invokes existing domain providers and supervises executors. Clients render/decide requests; they do not grant authority by changing a tool list. Standalone local approval needs an explicitly trusted local operator surface, not a model-accessible approve function.

Rules distinguish baseline allow, requestable access and hard denial. Operations bind Account, Principal, Workspace, work-directory, public Session, disclosure audience, capability/resource, exact arguments and policy revision. Approval binds the reviewed request and supports once or Session lifetime with expiry. Request transitions use compare-and-swap. Gates reauthorize at invocation and result delivery. Revocation does not erase already disclosed data or undo effects.

Use stable native tool-task identities plus protected store/Session bindings for request correlation, never text/order or native integer IDs alone. Persist pending decisions outside agent-writable files. Native Durable remains the execution-state owner; this ledger is permission authority, not a second Chat timeline. Current oqto-log Chat authority is unchanged.

Broker grants need no process restart. Direct mount/seccomp expansion uses fresh or replacement hands; no unsandboxed fallback. Freeze old admissions, drain/cancel, confirm stop, replace and fence generations. Failed replacement fails closed. Do not claim that Durable checkpoints a shell process or that uncertain effects may safely replay. Every shell/codemode/skill/subagent path must use the same executor boundary before production use.

## Local standalone composition (oqt)

The standalone `oqt --coding` composition implements this contract locally:

- **Permission Guard outside the brain.** A detached per-workspace host runs upstream's server plus a guard. The guard owns the ledger carriers, Executors, owner credential broker and operator socket. Each Session worker (the brain) runs confined in its own bwrap/seccomp/Landlock sandbox with explicit binds only; owner credentials, the ledger and guard state are never bindable (inside or as an ancestor). The brain reaches only its per-Session tool socket; operator decisions arrive on a separate socket no brain can see. Approval authority therefore does not live in the brain.
- **Interrupted Invocations.** The ledger journals every admitted invocation. A brain loss (or a dead host on restart) marks live entries interrupted, revokes every grant and advances the generation; Durable reports the call as interrupted and never replays it (`replay: "unsafe"`). Retries need a fresh decision even if a Session grant was active. Hands stop with the brain or the host.
- **Ceilings.** A global owner file and a workspace file compile into path layers; each layer is evaluated independently and the most restrictive answer wins, so the agent-writable workspace file can only narrow. Built-in hard denies cover credential/system paths, protected runtime roots and anything containing them; `outside_paths = "allow"` is rejected. A changed ceiling is adopted only through an explicit owner path that revokes every grant; a plain reopen still fails closed.
- **Resource Grants.** Exact outside paths (read or write) and network are requestable, Session-scoped for one hour, and compiled into the next Executor generation after confirmed stop. Expiry and revocation remove the mount before the next admitted call. Individual tool calls are still admitted separately.
- **Owner credentials.** The guard renews near-expiry OAuth with upstream's own file-locked store and provider refresh, after a private backup; brains receive access tokens only.
- **Several clients.** Launchers attach to the same host/guard; decisions are single-winner CAS. Upstream server/client code is reused unchanged except for small reviewed source patches (worker spawn, worker pid attestation, client reattach after worker loss, server error logging), applied only when the pinned text matches exactly.

Limits: local OS ownership is the operator boundary, not Account authentication; the brain has network for providers; the guard and host are trusted, unconfined code; network grants expose the host network including localhost services.

## Prototype boundary and security checklist

- [ ] Verify actor and target binding independently of model arguments; prototype identities are explicitly synthetic until runner authentication is wired.
- [x] Fixture tool pipe rejects approval/control commands; protected ledger/history stay outside actual hands mounts. Production carrier authentication remains below.
- [x] Exact bounded request, policy revision and pinned audience; no implicit all-Accounts "global" scope.
- [x] Hard denial, cross-scope access and changed arguments fail before fixture provider access.
- [x] Single-winner approval; once/Session expiry, revocation and delivery fencing tested.
- [x] Protected pending request survives authority and native harness reopen; no raw domain databases mounted in hands.
- [x] Actual bwrap/seccomp denies extra access before grant/after revoked replacement. Native fresh hands preserve native conversation identity; persistent replacement is separately proven with a stable supervisor/fixture Session binding.
- [x] Local owned hands supervisor confirms bwrap/descendant stop before replacement; retained environment handles cannot reconnect. Composed native coding turn proves revocation during an active shell, withheld secret output, stopped detached writer and fresh-hand continuation. Interrupted coding effects use native unsafe replay and sanitized uncertainty errors. Authenticated runner/reopen/in-flight worker-kill qualification remains separate.
- [x] Native Durable read-only permission wait/recovery demonstrated separately from synthetic core tests.
- [x] Source-only upstream Pi TUI fixture demo uses the same ledger/gates: in-turn Once/Session approval, default denial and live revoke/re-request pass PTY coverage. See `tools/oqt/README.md`.
- [x] Opt-in standalone real coding uses owning defaults/read-only credentials, exact Once or explicit workspace-wide 60-second Session approval, bounded sequential invocations and confirmed executor retirement before delivery. Actual owning-model TUI write/continuation and disposable-profile all-tool/reopen proofs qualify local experimental use. The reviewed brain remains trusted, not OS-confined.
- [x] Local brain confinement behind a guard, ceiling compilation with workspace narrowing, exact external Resource Grants, in-flight brain/host kill recovery and multi-client attach are implemented and proven with real TUI/PTY runs (`tools/oqt/README.md`).
- [ ] Authenticated TUI/Web/headless runner integration and Account-level authority; the local guard is not that gate.
- [x] Resource-bounded gates, no user config/live service changes, no default activation.

Initial proof uses disposable resources and a deterministic provider. A headless trusted operator is the initial approval surface. App grants already have reviewed-definition binding but are App-instance-specific; do not reuse those rows as generic Session grants or introduce a bypass to App authority.

## Consequences

The runner remains sole execution authority. Narrow service grants can adapt immediately; confinement generation changes may require a brief reload-like transition. Pending work, decisions and effect disposition are inspectable. Multi-client private-history disclosure must be approved for the Session audience; hiding a tool card alone is insufficient. Authentication, real mmry/trx providers, full oqto-log translation and production Pi TUI integration remain explicit gates. The source-only `oqt` fixture TUI is separately demonstrated and does not qualify those gates.
