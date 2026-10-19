# Worker-owned operator result receipts

## Status

Implemented in the source-only local oqt composition (2026-10-17), `oqto-g7rk.2`.
Refines ADR-0059/0060. Not an installed-runtime update, Account-authenticated
runner qualification, native mobile SDK, or release/deployment approval.

## Context

An operator shell can finish an external effect before its TUI receives the
response. A client-local output queue loses context on restart, cannot serve a
second client, and can duplicate accepted text after an ambiguous response.
Deleting effect-journal rows at executor completion also loses the distinction
between confirmed result admission and merely observed process exit.

## Decision

The native worker is the sole owner of durable operator result content in a
Session. The permission ledger stores effect identity and sealed result hashes,
not output bytes or a second Chat timeline. Hosted canonical history remains
`oqto-log`; this local native composition does not change that authority.

1. The guard allocates a fresh Session-bound invocation ID, validates the bounded
   command, checks a registered native result consumer, and journals admission
   before running confined hands. Model/operator exclusion covers the full
   invocation and confirmed executor retirement. Cancellation never means replay.
2. After observing completion, the guard confirms descendant stop and rechecks
   authority/resources before publication. Unconfirmed teardown freezes authority;
   resource expiry/revocation prevents unpublished output disclosure.
3. The guard seals the exact publication digest in protected effect metadata,
   then publishes to the registered worker on its Session-bound tool connection.
   `!!` publishes an opaque envelope with empty command/output: private bytes do
   not cross into the brain, model context, native documents or entries. Its
   human-facing output is intentionally not recoverable from Session history.
4. The worker transactionally binds invocation ID to the complete publication
   digest and, for `!`, payload. Receipt v2 (ADR-0065) uses a bounded Session-owned
   index with explicit native ConversationId, not a fork-copied document. Admission
   pins the observed conversation before effects; recovery covers all branches.
   Changed-payload ID reuse and foreign Session/conversation IDs fail closed. This
   explicit binding is necessary because native request-ID lookup does not itself
   compare input content.
5. Visible context is admitted through native `Conversation.submit(type=write)`
   with a stable request ID, never prompt/steer/follow-up. It is passive and cannot
   initiate inference. Acknowledgment means **durable admission**, not placement,
   terminal rendering or delivery to a model. Waiting for placement while a model
   is blocked behind guard serialization would deadlock.
6. Only an exact native acknowledgment clears the guard's live/interrupted row.
   The sealed digest remains as a tombstone preventing ID re-entry. Reopen can
   reconcile a lost acknowledgment from worker-owned durable state without
   executing the command again or restoring revoked grants.
7. The document-before-submission boundary is recoverable by stable IDs. Existing
   queued, placed or terminal native submissions are reused; user-aborted passive
   writes are not automatically resubmitted. The receipt still records the result
   even if its model-context placement was cancelled.
8. If publication or acknowledgment fails, the caller does not receive success;
   the journal remains uncertain until a verified receipt reconciles or the
   operator explicitly acknowledges inspection. A crash before native admission
   may lose output bytes, but never yields a completed-result claim or automatic
   replay. External exactly-once effects are not promised.
9. The initial bounded contract allows 128 operator invocations/results per
   Session, including pending/uncertain IDs, and canonical publication JSON below
   28 KiB. Capacity is enforced **before effects** in protected admission; no
   receipt eviction or automatic Session reset. Open a new Session at capacity.
10. `/shell-results` captures the selected public Session and observed native
    conversation and queries that exact worker target; interrupted IDs remain
    separately visible from protected authority.
    Startup observers replay the current snapshot, and notification identity is
    the stable invocation ID, not array length or text. Terminal callbacks cannot
    inject results into another Session after a switch.

The worker lifecycle has an explicit `beforeClose` hook, applied only through an
exact pinned-source patch. It unsubscribes result producers and drains native
writes before Harness close; post-close cleanup releases transport resources.
There is no monkeypatch of Harness methods and no direct Pi JSONL/SQLite writing.

## Recovery and evidence

- Worker/native SQLite tests cover identical retry, payload mismatch, Session
  isolation, opaque exclusions, passive queued admission, cancellation and
  document/submission crash boundaries with zero model calls.
- Composed guard/hands/native tests cover lost ACK without duplicate effect,
  native storage failure, teardown uncertainty, publication-boundary expiry,
  active-model admission without deadlock, privacy and exact identity.
- Real TUI fixtures cover multiple clients, client reopen, model-request content,
  host SIGKILL during an operator effect and after completed native admission.
- Rust public-API tests cover protected seal/completion, wrong actor/digest,
  interrupted reconciliation, persisted tombstones and pre-effect capacity.

These receipts qualify local source semantics, not remote Account/MFA routing,
physical device behavior, power-loss durability, arbitrary external providers,
or cross-machine ownership handoff. SQLite process-crash proof is not a promise
against storage/power failure. Classic import and portable mobile conformance
remain separate workstreams.
