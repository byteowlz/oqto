# Adaptive sandbox prototype

Tracked by `oqto-pdbs` and `oqto-g7rk`; design: [ADR-0053](../../docs/adr/0053-adaptive-capability-gates-and-replaceable-executors.md).

## Implemented and proven

- `backend/crates/oq-permissions`: host-neutral permission authority, protected SQLite permission ledger, pinned binding/audience/policy, bounded exact request arguments, allow/ask/hard-deny, once/Session approval with expiry, decision CAS, revocation and invocation/delivery fencing. SQLite stores permission decisions, NOT provider data or a Chat timeline.
- 22 lifecycle tests: baseline permissions, history escalation, memory/issue read vs write, once vs Session, denial, wrong operator/digest/revision, argument substitution, cross-Session permit rejection, changed audience/policy refusal, identical-binding cross-ledger permit/review replay refusal, concurrent first-open and approval, expiry/clock rewind, private/symlinked storage, canonical object keys, already-pending Session-grant adoption, overlapping grants and executor fences.
- One actual Linux confinement test using the existing `oqto-sandbox` compiler, bwrap and enforcing seccomp BPF: missing extra-directory access → approved read-only executor replacement → revoked replacement. No brain-process restart; stale executor admission blocked; protected history/permission ledger unreadable by hands; read-only writes fail; old descendants confirmed dead (zombies may await init reaping).
- Three exact upstream Durable 1.1.0 / native SQLite tests against commit `cdaf6c227d4718a72b59ccd02152c9614df44cfc`: own history without a prompt, broader history waits inside a turn and resumes after approval without restarting the brain/model plan; pending permission survives harness + permission-carrier reopen with the same native conversation/task request; approved file read runs through fresh real bwrap/seccomp hands, revocation/denial prevents the next provider read. Tool-result assertions use native tool-call IDs, not visible ordering.
- Headless fixture carrier (`backend/crates/oq-permissions/examples/permission_carrier.rs`) has separate trusted-supervisor-created tool and operator pipes. Tool pipe cannot inspect/approve/revoke. This is test authority, NOT Oqto authentication. History responses are explicit fixture data; memory/trx are represented in policy tests, not real provider adapters.

"Once" authorizes one logical operation/request with exactly reviewed arguments. It does not promise exactly-once external effects. Only the fixture's read-only tools declare `replay: safe`; effectful commands must retain interrupted/uncertain disposition and reconciliation.

## Run

First build the exact disposable upstream source as described in [the upstream service qualification](../durable-workflows/upstream-service-spike/README.md). Then:

```sh
CARGO_TARGET_DIR=/path/to/existing/cargo-cache \
  just test-adaptive-sandbox /tmp/oqto-pi-service-spike.PVTso2/source
```

`CARGO_TARGET_DIR` is optional. This lane currently requires Linux x86_64, working unprivileged bwrap, Python 3, Node compatible with the qualified source, and cached Cargo dependencies (`--offline`). Missing confinement/BPF/pin/build is a failure, never an unsandboxed fallback or skipped PASS.

Launcher: explicit new-crate/test filters, one CPU inherited by children, nice 19, one Cargo/test worker, per-phase 90-second deadline, clean HOME/environment for native tests, 512MiB V8 ceiling (not an aggregate memory quota), no installs/live services, and surviving-carrier check. Tests clean disposable `/tmp` and `/var/tmp` resources. Existing user profiles/config are never overwritten.

## Not yet implemented / no qualification claim

- Production runner authentication and Account→Principal binding; true approval authorities and shared-Session disclosure rules.
- A confined brain process, provider credential mediation and removal of every ordinary local-tool/plugin/codemode bypass. Native tests run trusted harness code in the test process; hands are actually confined. Do not expose this fixture carrier as a production endpoint.
- Pi client/server TUI facets, Oqto Web permission UI and `oq`/`oqt` launchers. Approvals here are test-supervisor decisions, not an interactive user acceptance test.
- Actual authorized oqto-log history, mmry and trx providers; protected-domain resource lookup and budgets. No direct writes to those stores.
- Workspace/global `sandbox.toml`→semantic policy compilation and approved-profile persistence/management. No new TOML mode is claimed or silently accepted.
- Production supervisor drain/cancel, multi-client reconnect, revocation of active effectful processes, in-flight SIGKILL/unknown-effect reconciliation. Executor fence is an admission primitive; it does not kill a process. The confinement test's supervisor confirms stop before installing a generation.
- Mac/mobile confinement, real providers/OAuth, full history paging, deployment/default activation or release acceptance.

The two boundaries are deliberate: service grants update live; direct confinement changes require fresh/replacement hands. Durable owns execution continuity. `oq-permissions` owns permission authority. The runner must compose them before this becomes a usable `oqt` session.
