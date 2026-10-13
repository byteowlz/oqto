# Adaptive capability gates and replaceable executors

## Status

Proposed implementation contract (2026-10-12), `oqto-pdbs` / `oqto-g7rk`. User approved the prototype direction, not production activation. Extends ADR-0001, ADR-0023, ADR-0039 and ADR-0048. ADR-0051's proposed seccomp notification work is not a dependency.

## Decision

Keep Pi Durable orchestration in a stable, narrow brain environment. History, mmry and trx operations cross trusted capability providers; arbitrary code, files, shell and child processes cross confined executors. A permission change updates protected authority, not agent-editable configuration. Workspace/global policy remains a ceiling: approval cannot turn a hard denial into an allow.

Build `oq-permissions` as a host-neutral Rust crate in the existing backend workspace. It owns the permission lifecycle and its protected ledger API, not Oqto Account authentication, history, memory, issue storage, or OS confinement. The owning runner supplies verified binding and approval identity, operates the ledger, invokes existing domain providers and supervises executors. Clients render/decide requests; they do not grant authority by changing a tool list. Standalone local approval needs an explicitly trusted local operator surface, not a model-accessible approve function.

Rules distinguish baseline allow, requestable access and hard denial. Operations bind Account, Principal, Workspace, work-directory, public Session, disclosure audience, capability/resource, exact arguments and policy revision. Approval binds the reviewed request and supports once or Session lifetime with expiry. Request transitions use compare-and-swap. Gates reauthorize at invocation and result delivery. Revocation does not erase already disclosed data or undo effects.

Use stable native tool-task identities plus protected store/Session bindings for request correlation, never text/order or native integer IDs alone. Persist pending decisions outside agent-writable files. Native Durable remains the execution-state owner; this ledger is permission authority, not a second Chat timeline. Current oqto-log Chat authority is unchanged.

Broker grants need no process restart. Direct mount/seccomp expansion uses fresh or replacement hands; no unsandboxed fallback. Freeze old admissions, drain/cancel, confirm stop, replace and fence generations. Failed replacement fails closed. Do not claim that Durable checkpoints a shell process or that uncertain effects may safely replay. Every shell/codemode/skill/subagent path must use the same executor boundary before production use.

## Prototype boundary and security checklist

- [ ] Verify actor and target binding independently of model arguments; prototype identities are explicitly synthetic until runner authentication is wired.
- [x] Fixture tool pipe rejects approval/control commands; protected ledger/history stay outside actual hands mounts. Production carrier authentication remains below.
- [x] Exact bounded request, policy revision and pinned audience; no implicit all-Accounts "global" scope.
- [x] Hard denial, cross-scope access and changed arguments fail before fixture provider access.
- [x] Single-winner approval; once/Session expiry, revocation and delivery fencing tested.
- [x] Protected pending request survives authority and native harness reopen; no raw domain databases mounted in hands.
- [x] Actual bwrap/seccomp denies extra access before grant/after revoked replacement. Native fresh hands preserve native conversation identity; persistent replacement is separately proven with a stable supervisor/fixture Session binding.
- [ ] Descendant cleanup and stale-generation rejection; interrupted effects explicitly uncertain.
- [x] Native Durable read-only permission wait/recovery demonstrated separately from synthetic core tests.
- [x] Source-only upstream Pi TUI fixture demo uses the same ledger/gates: in-turn Once/Session approval, default denial and live revoke/re-request pass PTY coverage. See `tools/oqt/README.md`.
- [ ] Authenticated TUI/Web/headless integration and real coding profiles; the private local demo is not a shipped production UI.
- [x] Resource-bounded gates, no user config/live service changes, no default activation.

Initial proof uses disposable resources and a deterministic provider. A headless trusted operator is the initial approval surface. App grants already have reviewed-definition binding but are App-instance-specific; do not reuse those rows as generic Session grants or introduce a bypass to App authority.

## Consequences

The runner remains sole execution authority. Narrow service grants can adapt immediately; confinement generation changes may require a brief reload-like transition. Pending work, decisions and effect disposition are inspectable. Multi-client private-history disclosure must be approved for the Session audience; hiding a tool card alone is insufficient. Authentication, real mmry/trx providers, full oqto-log translation and production Pi TUI integration remain explicit gates. The source-only `oqt` fixture TUI is separately demonstrated and does not qualify those gates.
