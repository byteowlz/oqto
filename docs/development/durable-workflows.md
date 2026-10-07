# Durable workflows adapter (development slice)

Tracked by `oqto-g7rk`; qualification `oqto-pdbs` remains a prerequisite for production adoption. Source integration uses the portable core and trusted-host executor from `pi-agent-extensions`, not a copied workflow engine.

## High-risk checklist before integration

- [ ] Backend authenticated SessionTarget -> permitted Account/Principal/Workspace/work-directory/Session -> runner. Never accept those facts as model/client claims.
- [ ] Protected authority repository owns actual admission; Pi review entries are intent only. Pin approved job ID/revision/digest and runtime binding, expire/revoke grants, recheck immediately before model transport.
- [ ] Private SDK storage outside tool mounts, one writer/owner. No generated tool code in trusted host. Initial slice tool-less, no subagents, scheduling or history import.
- [ ] Native generation hook exceptions are NOT enforcement in Durable1.0.4. Verify zero provider calls on denial/revocation, using the executor's guarded transport.
- [ ] Stable public Session/job/run identities and explicit native conversation/submission binding; no text/order/index reconciliation. Do not turn output receipts into fabricated Chat entries.
- [ ] Decide history authority by accepted ADR before canonical Chat integration. Current oqto-log policy is unchanged; native SDK owns task checkpoints. This module does not write native JSONL/SQLite directly.
- [ ] Snapshot/watch/reconnect, process kill/restart, uncertain effects, cancellation confirmation and same-session cutover need independent proofs.
- [ ] Preserve ordinary Pi and user-owned configuration/pins. No global installation, deployment or runtime default changes.

## Current interface

`tools/durable-workflows/adapter.mjs` is a trusted JavaScript host module, NOT an agent-callable production activation endpoint. `createWorkflowAdapter` receives a protected authority implementation, the real portable core validator/executor, approved Models transport and private storage root. Its only execution method is manual `run(actingScope, {jobId, occurrenceId})`.

The caller derives actingScope from authenticated routing. The authority implementation must read protected records and provide `resolve` and `recordReceipt`; neither a JSON proposal nor a callback supplied by the model is a grant. Initial trusted module tests use a test authority; that does not prove production Account authentication or persistent grant storage. Missing authority fails closed.

A protected record binds Account, Principal, Workspace, work-directory and public Session, job revision/digest, binding ID, exact executor runtime and expiry. Each native admission/request phase resolves it again. Storage paths derive from that binding, never the proposal cwd/client path. Concurrent calls to the same binding in one host are rejected across revisions; total native concurrency defaults to four (configurable 1–64). These are admission bounds, not OS resource isolation. The executor's native ownership lease is still required across processes. The authority must bind the exact occurrence and stable public run ID, not just a reviewed job.

Native Durable1.0.4 conversation/task/submission IDs are positive safe-integer brands, NOT UUID strings. Receipts retain the protected store binding plus native numbers separately from public Session/job/run IDs; two stores may both contain conversation1.

## Gates

```sh
npm --prefix tools/durable-workflows run check
npm --prefix tools/durable-workflows test
OQTO_WORKFLOW_EXECUTOR_DIR=/absolute/disposable/built/executor \
  npm --prefix tools/durable-workflows run test:native
```

Native tests require exact executor runtime `pi-durable@1.0.4` and a built core. Their script adds Node's `--conditions=import` so `createRequire.resolve` can locate import-only SDK package exports relative to the isolated executor; actual loading still uses ESM import. Prepare dependencies/build in a disposable copy, preserving the canonical extensions checkout. The native test uses faux provider, private temporary stores and no tools; no live provider/config/scheduler.

## First-slice evidence

Tested against extension source e70d725, using a disposable copy with frozen executor package-lock, exact pi-durable/pi-ai/chord1.0.4 and Node26.8.1. Core and provider were real; authority repository was a test fixture, not Oqto Account authentication. Native tests prove primed tool-less manual execution, two occurrences in one native conversation, same-occurrence request dedup across close/reopen, frozen run cap, transport-level revocation with zero provider calls and tampered-review denial. Process kill/restart and power loss are NOT proved by reopen.

No Rust protocol, deployed config, frontend types, Session Chat, user profile or canonical history store changed. This module does not satisfy the authenticated runner/Oqto acceptance milestone on its own.

## Remaining integration

Rust runner-owned protected repository and private worker transport/supervision, actual authenticated routing, durable receipt inspection, canonical history authority decision/events, generated public types and UI, scheduling, crash and confinement proof. This host slice alone is NOT the completed Oqto adapter and does not make the extension's `activate` available.
