# pi-durable-audit — Oqto and oqt, 2026-10-17

Tracker: **oqto-0d73**, coordinated with govnr **gvnr-1w2p**.

## Verdict

**Adopt Durable as an optional harness behind the runner; do not replace Oqto wholesale.** The implementation branch already demonstrates that integration locally. It is not production acceptance. Preserve runner placement/authorization, oqto-log canonical history, protected permission authority, owner credentials, and frontend identity contracts. Durable can replace harness-local queues, task scheduling/recovery, compaction orchestration and committed-view plumbing. It does not replace authentication, sandboxing, cross-machine ownership, workspace bytes, or exactly-once external effects.

**Do not extend the recent oqt `!` shell path as-is.** Static audit found missing grant-expiry synchronization, missing invocation journal, cancellation/revocation gaps, and session-unkeyed pending output. Its happy-path test does not cover these. See Appendix B F1–F8. These findings require focused regression tests and fixes before broader rollout; this audit changes no implementation.

**Correct prior advice:** Pi Durable already provides a transactional JSONL backend. Choosing SQLite is not necessary to obtain durable execution. Durable JSONL is a different multi-file schema from classic Pi JSONL; neither backend makes classic import or divergent-copy synchronization automatic. Classic Pi also already implements overflow compaction/retry; that is not unique to Durable.

## 1. Scope, provenance and proof level

- Oqto: `/home/wismut/byteowlz/oqto_refactor`, HEAD `701e42e62c96ff356964da7f4d3d5c57eba10ebf`, **dirty observed working tree**. Working-tree TUI control additions are not HEAD/release evidence.
- oqt: `/home/wismut/byteowlz/oqto-durable-support`, HEAD `52e492982d8745fe9e47285729bf67486bbf3988`.
- Upstream: `~/.local/share/oqt/pi-source`, pin `cdaf6c227d4718a72b59ccd02152c9614df44cfc`. This is a pinned audit, **not a claim about latest upstream main**. The owned oqt runtime adds exact patches in `tools/oqt/source_patches.py`.
- Read the complete upstream Durable README and all **32** `packages/durable/test/examples/00..31` files. Supplementary review covered complete spec, handoff, Chord notes and pi-ai README (Appendix C).
- Source findings below are static inspection. Examples were actually run separately; none establishes authenticated Oqto, browser/mobile, power-loss or distributed-owner correctness.
- No credentials, private conversations, production databases, user config contents, authenticated providers or live sessions were read/exercised. No production code/config/deployment changes.

Reference convention: **U** = pinned upstream `packages/durable/`; **O** = Oqto observed tree; **Q** = oqt implementation tree. Paths in appendices use their stated root. Source line numbers are snapshot-specific.

### Reproduction executed

All 32 examples were executed sequentially with a 60 s timeout per example, one CPU, nice 19, and `env -i` with disposable HOME, PATH and `PI_OFFLINE=1` only:

```sh
src="$HOME/.local/share/oqt/pi-source"
cd "$src/packages/durable"
env -i HOME=/tmp/pi-audit-examples-home PATH="$PATH" PI_OFFLINE=1 \
  node --import "$src/packages/coding-agent/src/experimental/source-resolver.ts" \
  test/examples/13-recovery.ts
```

Result: **32 commands exit 0; 31 examples execute their offline paths; `16-real-model` explicitly skips without OPENAI_API_KEY.** This is not 32 authenticated tests and these examples are runnable demonstrations, not an exhaustive assertion suite. Examples that use shell operate on source listings or disposable temporary files. Per-example result manifest is included below; raw run logs were retained locally in `/tmp/pi-audit-example-logs/` (ephemeral).

## 2. Replacement map — evidence, not deletion authority

| Concern | Upstream example backing capability | Exact Oqto/oqt seam | Assessment |
|---|---|---|---|
| Atomic history plus app state | U `01-documents.ts:23-43`; `11-extension-state.ts:36-47`; `09-context.ts:106-134` | O `backend/crates/oqto-history/src/oqto_log/store.rs:372-455,651-652,1037-1038`; Q `tools/oqt/coding-worker.ts:138-151` harness construction | Replace harness-local state, **not** canonical oqto-log under current ADRs. A separate DB is not atomically committed with Durable. |
| Follow-up/steer/write queue | U `20-inbox.ts:36-68` | O `frontend/src/oqto-ui/engine/transport.ts:90-141,277`; `backend/crates/oqto/src/api/ws_multiplexed/agent.rs:1025-1031,1098-1104,1171-1177` | Durable inbox replaces harness scheduling; browser outbox, auth and reconnect delivery still needed. |
| Durable submission admission | U README `Persist and Resume` requestId example; `23-subagent-background.ts:85-119` | O `backend/crates/oqto/src/api/ws_multiplexed.rs:48-100` process-local 1 h cache | Replace volatile acceptance with durable requestId receipts behind runner. Keep public request IDs stable through retries. |
| UI snapshot + committed updates | U `19-json.ts:75-100`; `21-late-join.ts:53-99`; `05-watches.ts:27-39` | O `backend/crates/oqto-runner/src/pi_manager.rs:520-620`; `frontend/src/oqto-ui/engine/session-engine.ts:105-133`; `frontend/src/oqto-ui/chat/ChatPane.tsx:239-258` | Durable provides snapshot/watch, **not** durable event replay or Oqto wire adapter. Cannot delete all frontend event state. |
| Process/task recovery | U `12-tasks.ts:13-43`; `13-recovery.ts:52-89` | O `backend/crates/oqto-runner/src/pi_manager.rs:4559-4589`; `backend/crates/oqto/src/api/ws_multiplexed/agent.rs:842-923`; Q `tools/oqt/recovery_tui_test.py:36-83,109-150` | Durable saves task checkpoints. Runner still owns process lifecycle; tool effects require fencing and replay policy. Existing oqt recovery test definitions are not rerun here. |
| Forks and per-conversation config | U `02-forks.ts:31-53`; `08-harness-conversations.ts:31-40`; `28-reviewer.ts:74-96` | O `backend/crates/oqto-runner/src/daemon/server.rs:3385-3431,3468-3479`; Q `tools/oqt/session-commands.ts:42-91` | Useful primitive, not drop-in `/tree`/`/branch`, workspace fork, history mapping or ownership transfer. |
| Summarization/compaction | U `25-compaction.ts:43-103`; U `src/harness/compaction.ts:108-140` hook | Classic Pi upstream `packages/coding-agent/src/core/agent-session.ts:2947-3044,3088-3097`; O runs that Pi process | Durable adds durable/background task orchestration, not inherently better summaries. Classic overflow recovery already exists. Kompressor can implement beforeCompact; quality needs retention eval. |
| Retry/checkpoint/child tasks | U `12-tasks.ts:20-43`; `24-child-tasks.ts:63-91`; `23-subagent-background.ts:85-124` | O `backend/crates/oqto-runner/src/pi_manager.rs:4559-4617`, restart path above | Replaces a custom workflow only where behavior matches. Durable does not supply deployment, scheduler activation adapters, or external idempotency. |
| Approval pause/resume | U `17-coding-tools.ts:46-62` demonstrates hooks; `27-plan-mode.ts:54-57` demonstrates tool selection; **no equivalent protected human-approval authority in these examples** | Q `tools/oqt/guard.ts:748-788`; `backend/crates/oq-permissions/src/ledger.rs:362-374` | Keep separate trusted ledger/guard. Hook block or plan mode is not OS confinement, Account authentication, audience authorization or atomic approval persistence. |
| Sandbox/environment routing | U `29-sandbox-per-conversation.ts:37-64`; `30-tool-override.ts:24-51` | Q `tools/oqt/guard.ts:826` onward; `backend/crates/oq-permissions/examples/oqt_brain.rs`, `oqt_hands.rs`; O oqto-sandbox | U example's “sandbox” is a directory, not an OS security boundary. Keep bwrap/seccomp/Landlock, protected roots, provider network/credentials boundary. |
| Classic JSONL import / pi-session-sync | **No equivalent found** among 00–31. `09-context` and `19-json` are primitives only | O `backend/crates/oqto-runner/src/daemon/jsonl_ingest.rs:186-284`; Q native-only session commands | Required separate compatibility work. Source identity/provenance, branches, incremental imports, attachments and path mapping need proof. |
| Notifications, machine placement, Account auth | **No equivalent found** among 00–31 | O `backend/crates/oqto/src/api/ws_multiplexed/agent.rs:46-149` routing/re-authorization | Retain Oqto responsibilities. A local multi-client demo is not authenticated multiplayer acceptance. |

## 3. Current versus Durable crash behavior

### Current Oqto

Pi owns classic continuation JSONL; runner snapshots and native ingest feed **oqto-log**, the canonical UI read authority. The runner seeds its memory projection from oqto-log (`pi_manager.rs:1231-1249`) and emits process-exit/stopping when Pi exits (`:4559-4589`). Restart resolves native continuation via authorized canonical/native bindings (`session_resume.rs:21-90`). That recovers transcript/context, not a general durable tool task graph. Snapshot failure is logged (`pi_manager.rs:4282`), so intended persist-before-idle ordering is not a proof that every idle event means successful persistence.

Frontend retry IDs and a backend in-memory acceptance cache reduce duplicate submissions while processes survive, but backend restart loses that cache. Runner delivery replay is an in-memory 2048-event ring with a fresh stream UUID per generation. Cursor drift requires canonical reload; it is not recoverable execution state.

### Durable library

Intent/checkpoint/results and app documents inside the same Session commit are atomic. Reopen makes surviving running tasks pending; code is supplied again by a registry, and scheduling starts on resume/progress APIs. Stored unsafe tool calls become interrupted, not re-executed; safe calls may rerun. Provider requests may repeat and incur cost. Nothing implies a remote PR/email/deploy either did or did not happen when its result receipt is absent.

U `13-recovery.ts:29-34` writes a memo **before** printing: this prevents a duplicate print, but a crash between memo and print can omit the print. It is not an exactly-once external-effect example. U `12-tasks.ts:28-35` relies on a fake bank honoring an idempotency key; production must obtain that guarantee from the actual external service. U `24-child-tasks` refunds a fake in-memory Set, not a proven payment compensation service.

### oqt branch

The branch combines Durable's transcript/task store with a separate protected permission ledger and replaceable executors. Model-facing tools explicitly declare `replay: "unsafe"` (`Q tools/oqt/gated-tool.ts:37`), so oqt deliberately does **not** promise automatic tool replay after a crash. Existing recovery tests cover model-issued calls; operator `!` bypasses that invocation journal. Keep library guarantees, integration behavior, and previously reported tests separate.

## 4. Replay/idempotency table

This table inventories audited operation classes and reachable shell/provider categories, **not every possible command an arbitrary shell can execute**. An exhaustive per-command replay guarantee is impossible without a closed tool/provider inventory. Uninventoried integrations remain **unknown/unsafe by default**.

| Operation class | Safe / unsafe / unknown | Failure window and required policy |
|---|---|---|
| Pure canonical history/catalog read | Safe (scoped) | Repeat query; preserve authorization and canonical identity. |
| File read/search | Safe for effects, not snapshot-identical | Contents can change between attempts; preserve exact provenance when required. |
| Model generation / summarization | Unknown externally; retryable by contract | Provider can receive request twice/cost twice. Pin model/request, don't promise exactly-once billing. |
| Prompt/steer/follow-up admission | Safe **only with persisted same conversation + requestId** | Oqto cache is volatile; pass stable requestId through Durable and retrieve receipt after reconnect. |
| View/watch delivery | Safe projection with correct snapshot boundary | Watch overflow replaces suffix by snapshot. Do not derive mandatory audit facts from every callback. |
| Canonical history ingest | Repeatable transaction when invoked, incomplete convergence policy | Native source IDs matter; current count guard misses equal-length edits/truncation. |
| Write file | Unsafe by default | A rerun can overwrite later edits; truncation/partial effect possible. Need expected version/content and atomic replacement receipt. |
| Edit file | Unsafe | Old-text match may fail, or match a later instance; use exact base/version and effect reconciliation. |
| Arbitrary bash / tests / builds | Unknown => unsafe | Commands may send mail, deploy, append, spawn descendants, mutate caches/services. No generic safe declaration. |
| Git commit / push / PR creation | Unsafe without external dedup | Effect may exist before receipt. Stable operation key and remote lookup needed; local ledger key alone insufficient. |
| Network sends / email / payment / deploy / scheduler activation | Unsafe without endpoint contract | Stable endpoint idempotency key or explicit unknown/manual reconciliation. No general endpoint dedup evidence found. |
| request_access / permission grant | Conditional | Exact scope/binding/expiry and CAS; never turn an import/fork into new authority. |
| Remember “always” decision | Unsafe current failure path | Rule writes before digest/revision validation (Appendix B F6). Rejected stale review must leave rules unchanged. |
| Revoke / abort / restart / stop subagent | Context-sensitive unsafe | Repeating later can stop new work; carry operation identity and inspect current ownership. |
| Fork/create conversation | Conditional | Same operation must rediscover created object before submitting; preserve lineage and external/native mappings. |
| Operator `!` / `!!` | Unsafe current crash semantics | No durable invocation receipt; effect may complete without client receiving result, and canceled queued work may execute. Never retry blindly. |
| Snapshot sync / local continuation / owner handoff | Unknown, not implemented | Consistent snapshot is safe to archive; waking two copies duplicates execution owners. Explicit fork/transfer protocol required. |
| Trusted providers / history adapters | Unknown, incomplete | Carrier history fixture strings are not real store authorization; require scoped service contract and receipts. |

## 5. SQLite, JSONL and the MacBook sync workflow

U README Storage explicitly offers **Memory, SQLite, JSONL and Cloudflare SQLite**. U `17-coding-tools.ts:76` and `19-json.ts:37-49` exercise Durable JSONL. SQLite is recommended for long-running agents because the JSONL implementation also indexes/retains full history in memory. Choosing JSONL does **not** mean implementing transactions from scratch; upstream already supplies sidecars and commit markers.

Important distinctions:

1. **Classic Pi JSONL != Durable JSONL.** Durable stores main/document/task records, not a drop-in classic session file. `pi-session-sync` cannot assume schema compatibility.
2. **Consistent SQLite backup is a valid portable artifact.** WAL must be included through proper backup/closed copy, not a blind live `.sqlite` copy. The same consistency requirement applies to Durable's multi-file JSONL directories.
3. **One process owns a store.** Neither backend supplies distributed locking/merge. Two local copies can each start pending tasks. A database is not the whole workspace, credentials, grants or process state.
4. **Power-loss claims need care.** SQLite uses WAL + synchronous NORMAL. JSONL fsync defaults false; the spec/implementation flush sidecars before main publication, but ordinary main-marker publication is not explicitly flushed even with fsync true. Both ordinary process-crash recovery and acknowledged-tail power-loss durability need separate acceptance tests. Appendix C provides exact references.

Recommended compatibility contract, **not implemented**:
- `/resume` presents classic and Durable sources with native ID, source machine, lineage, last synchronized revision and owning machine.
- Incremental classic import keyed by stable source entry IDs/ancestry, with an explicit source-to-Durable mapping. Preserve custom records/unknown kinds and original JSONL; never infer identity from text/count/order.
- History archive mode opens without scheduling tasks. Reading cannot implicitly call resume/wait APIs that activate scheduling.
- “Continue here” forks imported context into a fresh execution owner with fresh authority; “attach” controls original owner; “move” fences original owner before destination resume.
- Divergent source updates become explicit branches/conflicts, not overwritten histories. Define machine-home and workspace-path mapping without silently authorizing extra paths.
- Keep dirty repo files, untracked bytes, external operation receipts and scoped grants outside any claim that copying conversation storage moves the complete agent.

**Storage choice recommendation:** keep the upstream-supported SQLite default for live oqt while measuring long-session memory/startup and sync needs. Prototype Durable JSONL only if its operational benefits justify memory and multi-file sync costs. Do not build an export format or custom backend merely because SQLite was incorrectly assumed unportable.

## 6. Migration sequence and blocking gates

1. **Freeze contracts first.** Record public platform_id, native/external session identity, Durable storage identity, conversation ID, entry ID, task ID, requestId and event stream cursor separately. Preserve runner-only execution and oqto-log authority.
2. **Close oqt execution seams.** Bring operator and model calls under one fencing/resource/lifecycle contract, with cancellation, prompt revocation, uncertainty journal, session-keyed output, exact-decision remember ordering, bounded framing. Add failure-path regressions before widening exposure.
3. **Runner adapter in opt-in mode.** Admit durable commands with stable requestId behind existing Account/workspace/machine authorization; no frontend-to-harness bypass. Keep classic sessions working.
4. **Canonical projection.** Project committed immutable entries to oqto-log using stable source identities, durable cursor and idempotent admission. Snapshot/watch stream is not a lossless audit log. Test crash before/after both sides of the two-store boundary; no text/index/count matching.
5. **Reconnect and multi-client proof.** Snapshot + tail handoff, outbox retry across backend/runner restarts, stream drift and overshoot, simultaneous approvals, revoke audience changes, both current and legacy frontend paths, web/mobile/SSH attachment. Local fixture tests do not satisfy Account auth.
6. **Classic import and session sync.** Implement the compatibility contract above before users lose their existing archive workflow. Test repeated and updated imports, branches, compactions, binary attachments, path normalization and two-copy execution fencing.
7. **Automations last.** Webhook/scheduler → durable requestId → authorized sandbox → externally idempotent PR/deploy receipts. Fail closed where execution adapters are unavailable. Retain explicit interrupted/unknown states rather than blind retry.
8. **Delete proven redundant code only.** Gate by feature parity/failure injection, benchmark and a rollback path. No blanket “delete 8,000 lines” target.

Acceptance matrix must inject failures before effect, after effect/before receipt, after receipt/before publication, and during executor teardown; kill brain/host/runner/backend separately. Include power loss separately from SIGKILL. Include identical request retries, stale approvals, expired mounts, disconnected queued commands, cross-session output, oversized Unicode framing, equal-count JSONL edits, and missing source continuation files.

## 7. What this audit cannot establish

- Latest upstream compatibility: only the pin was examined.
- Authenticated runner/Oqto routing, mobile/browser/SSH deployment, production performance or security certification.
- No-duplicate real external effects (PR/email/deploy/payment), endpoint idempotency or a multi-node owner protocol.
- Faithful classic-session import/export, cross-machine sync, attachment portability, worker-side branch UI or trusted provider authorization.
- Byte-identical initial source content: initial proof captured git status, not a full filesystem snapshot. No source writes were made by the audit; status stayed identical before report creation. A hash manifest captured before report synthesis is checked again at completion. Concurrent-agent changes, if any, are not ours to revert.
- Upstream example exit success is not full conformance evidence. No fresh Rust/PTy/Oqto integration suite was run in this audit.

## 8. Example inventory / actual command outcomes

All paths below are U `test/examples/<name>.ts`; all command exit codes were 0.

- 00 conversation/atomic creation; 01 document/history commit; 02 forks; 03 owned conversation; 04 Chord state; 05 bounded watches.
- 06 harness/registry; 07 configuration/live settings; 08 handles/forks; 09 context/history heads; 10 registry reload; 11 extension state.
- 12 checkpoint tasks/idempotency assumption; 13 close/reopen recovery; 14 faux chat; **16 real model skipped**; 15 system sections.
- 17 coding tools on JSONL; 18 print mode/faux; 19 JSON events/default SQLite; 20 steer/follow-up/write inbox; 21 late join snapshot.
- 22 foreground child; 23 background reporter/restart; 24 child-task waits/abort; 25 background/manual/overflow compaction.
- 26 coding agent/env; 27 plan mode; 28 reviewer; 29 directory routing (not OS isolation); 30 override/wrap; 31 reload/restart.

Appendices preserve the detailed, independently inspected evidence and caveats. Where an appendix says it ran no tests, that refers to that inspection lane; the example run recorded above was performed by the coordinating lane.


---

# Appendix A — Mainline/observed Oqto evidence

# Pi Durable migration: Oqto read-only evidence audit

## Scope and provenance

Inspected `/home/wismut/byteowlz/oqto_refactor`; HEAD at audit: `701e42e62c96ff356964da7f4d3d5c57eba10ebf`. All citations below are repository-relative **observed working-tree** paths and 1-based lines. No repository edits, tracker operations, builds, or tests were performed. This is static evidence, not a runtime/crash-injection certification. “Replacement” means migration candidate, not an approved architectural decision. No proposed Pi Durable API or implementation was supplied, so compatibility with such an API is not established.

Working tree was dirty before inspection:
- Modified: `backend/Cargo.lock`, `backend/crates/oqto-runner/src/tui_rpc.rs`, `backend/crates/oqto-sandbox/src/config.rs`, `backend/crates/oqto/examples/sandbox.schema.json`, `docs/development/pi-tui-rpc-attachment.md`, `justfile`.
- Untracked: `backend/crates/oq-permissions/`, `backend/crates/oqto-runner/src/tui_rpc/control.rs`, `backend/crates/oqto-runner/src/tui_rpc/control/`, `backend/crates/oqto-sandbox/tests/guard_config.rs`, `docs/adr/0059-adaptive-capability-gates-and-replaceable-executors.md`, `docs/adr/0060-one-sandbox-config-approvals-are-the-guard-layer.md`, `oqt`, `tools/oqt/`.

The cited tracked files other than `tui_rpc.rs` were not listed as modified by `git status --short`; their observed contents therefore match HEAD at inspection. `git diff -- backend/crates/oqto-runner/src/tui_rpc.rs` explicitly shows HEAD is a **read-only** external TUI attachment, whereas the working tree adds `mod control`, exported control types, input leases, and capability negotiation. Do not attribute the working-tree input-control surface to HEAD. Untracked permission/control code is not release evidence.

## 1. Current crash and continuation contract

- Process stdout termination produces a translated canonical process-exit event with the last stderr context, publishes it, then marks the session `Stopping`: `backend/crates/oqto-runner/src/pi_manager.rs:4559-4589`. This is a failure notification, not evidence of automatic replay/re-execution of an interrupted tool.
- Runner creation seeds its in-memory message buffer from oqto-log: `backend/crates/oqto-runner/src/pi_manager.rs:1231-1249`. That buffer is a projection, not the durable authority.
- Native continuation is resolved on the machine owning the Pi files, not the control plane: `backend/crates/oqto-runner/src/session_resume.rs:1-6`. Resolution tries the public ID and, for imported `oqto-*` sessions, bound native IDs, searching managed and native-profile session directories: `session_resume.rs:21-30,34-59,67-90`.
- Missing continuation resolves to `None`, explicitly permitting a fresh session rather than silently continuing an unrelated one: `session_resume.rs:62-65,89-90`. Migration must decide whether this existing fresh-session fallback is acceptable for a durable run that promises continuation.
- Explicit server restart reads previous state and derives continuation/configuration before recreating/subscribing: `backend/crates/oqto/src/api/ws_multiplexed/agent.rs:842-923`. Preserve restart versus resume versus fresh-run semantics; they are not interchangeable.

## 2. History authority and persistence boundaries

**Observed durable read authority: oqto-log. Native Pi JSONL is continuation/source material, not the frontend's direct message authority.**

- Runner history catalog comes from oqto-log workspace listing: `backend/crates/oqto-runner/src/history_read.rs:116-128`. Message reads require one unambiguous public identity and project a 50-message canonical page: `history_read.rs:157-201`; tests explicitly cover canonical reads without Pi/harness-ID acceptance at `history_read.rs:330` onward.
- AgentEnd snapshots are persisted before downstream idle/read convergence: `backend/crates/oqto-runner/src/pi_manager.rs:4136-4204,4327-4328`. Persistence errors are logged at `pi_manager.rs:4282`; do not infer “idle implies successful durable commit” solely from intended ordering.
- Snapshot write is transactional, with snapshot-marker deduplication and per-source entry checks: `backend/crates/oqto-history/src/oqto_log/store.rs:372-455,495-508,626`. Its source-entry IDs include message fingerprint plus occurrence (`store.rs:495`), unlike native entry IDs. Preserve that distinction when replacing snapshot reconciliation.
- Native JSONL replacement accepts explicit source records: `store.rs:651-652,799-824`; replacement transaction begins at `store.rs:860`, uses native source entry identity at `store.rs:1037-1038`, commits at `store.rs:1206`.
- Quiescent-file ingest skips very fresh files and unchanged size/mtime cursors: `backend/crates/oqto-runner/src/daemon/jsonl_ingest.rs:186-209`. It resolves external/native identity to canonical session/workspace and replaces only when canonical message count is lower than parsed record count: `jsonl_ingest.rs:237-280`. Cursor advances afterward: `jsonl_ingest.rs:282-284`.
- **Important recovery limitation:** equal-length or shorter changed JSONL is not reconciled by that count guard. Source-ID-keyed replacement may be idempotent when invoked; that does not make the invocation policy a complete recovery algorithm. A durable migration should test equal-count edits, truncation, branch changes, and partial final lines rather than adopt count comparison as identity.

## 3. Identity domains that must survive migration

- Public/canonical ID helpers are distinct from native external identity: `backend/crates/oqto-history/src/oqto_log/store.rs:283-291`; binding resolution and write-resolution are explicit APIs: `backend/crates/oqto-history/src/oqto_log/bindings.rs:307,364,410,426,460`.
- The runner looks up Pi bindings by harness and reads `external_id`, not a guessed filename: `backend/crates/oqto-runner/src/session_resume.rs:49-57`. Imported binding continuation is regression-tested at `session_resume.rs:118-175`.
- Durable routing binding is resolved and reauthorized per command; personal ownership, remote-machine path, and shared-workspace admission are checked before executor selection: `backend/crates/oqto/src/api/ws_multiplexed/agent.rs:46-149`. Connection cache/caller cwd must not override this authority.
- Control rejects missing durable binding and `_system`, `pending-*`, or `tmp:*`: `agent.rs:65-79`. Session creation cannot mutate a bound workspace: `agent.rs:121-135`.
- Live delivery identity is another domain: fresh stream UUID, monotonically assigned sequence, event ID `stream_id:delivery_seq`: `backend/crates/oqto-runner/src/pi_manager.rs:549-555,605-614`. It is not a durable message/source-entry ID or an operation idempotency key.
- Fork translation explicitly resolves canonical entries to native source entry IDs and rejects synthetic history IDs: `backend/crates/oqto-runner/src/daemon/server.rs:3385-3410`. Keep public session, native session, canonical message, native entry, delivery cursor, client request, and fork operation identities separate.

## 4. Reconnect and frontend paths

### Runner/server event continuity

- Replay is **memory-only**, bounded to 2048 events: `backend/crates/oqto-runner/src/pi_manager.rs:520-526,549-555,614-616`. A process/runner recreation loses that buffer and creates a new stream UUID.
- Subscribe captures replay and installs the live subscriber under the same write lock: `pi_manager.rs:529-530,560-593`. Wrong-stream or below-floor cursor returns `Drift`; valid cursor returns only events above its sequence: `pi_manager.rs:569-585`. Future cursors currently get an empty resumed suffix (test `pi_manager.rs:713-724`), not rejection.
- Server reports drift as requiring durable timeline reload, and tracks seen event IDs: `backend/crates/oqto/src/api/ws_multiplexed.rs:2240-2294`. This is delivery dedup, not durable execution dedup.
- Relevant existing test definitions, not executed here: exact suffix/live continuation `pi_manager.rs:656`, stable event IDs on duplicate resume `:686`, wrong-stream `:729`, replay-floor drift `:742`.

### Current oqto-ui path

- Live platform uses session engine: `frontend/src/oqto-ui/platform/live-platform.ts:507`; live chat transport instantiates the pure transport: `frontend/src/oqto-ui/platform/live-chat-transport.ts:94`.
- Transport persists unacknowledged sends, retries them with the same request ID, and clears an ACK when a matching response arrives: `frontend/src/oqto-ui/engine/transport.ts:90-141,193-196`. It recreates subscribed sessions on socket open and schedules resync after reconnect: `transport.ts:165-191,219-265`; restores persisted outbox at `transport.ts:277` onward.
- **Potential convergence gap to verify:** `createSessionEngine.bind` registers connection observation but does not pass the transport's optional resync callback: `frontend/src/oqto-ui/engine/session-engine.ts:117-133`; transport invokes those callbacks at `transport.ts:184-190` and exposes registration at `transport.ts:377-383`. Do not assume scheduled transport resync reaches the page through this binding. Explicit `stream.resync_required` is separately projected: `frontend/src/oqto-ui/engine/projection.ts:297`.
- Drafts are disposable; turn-end/resync clears them in the engine: `session-engine.ts:105-113`. ChatPane discards draft/pending prompt and invalidates durable timeline queries on ended/resync updates: `frontend/src/oqto-ui/chat/ChatPane.tsx:239-258`. Connection updates alone return early (`:246`).
- Legacy coexistence is visible: `frontend/src/routes/AppShellRoute.tsx:3,103` imports/uses chat context; `frontend/lib/ws-manager.ts:452-469,1822-1899` has its own resync registration/snapshot path. A migration must inventory callers rather than treating either frontend transport as the only path.

## 5. Replay-safety classification

| Operation | Observed evidence | Migration constraint |
|---|---|---|
| Read canonical history | `history_read.rs:181-201` | Read-only/repeatable; preserve scope and public-ID admission. |
| Replay live canonical events | `pi_manager.rs:560-620` | Safe only as projection with stream/event-ID dedup; never re-dispatch tools from replay. |
| Retry prompt/steer/follow-up | `transport.ts:108-130`; `agent.rs:1025-1031,1098-1104,1171-1177` | Not crash-proof exactly-once. Acceptance markers are process-local, not an execution ledger. |
| Server acceptance dedup | `backend/crates/oqto/src/api/ws_multiplexed.rs:48,63-64,80-100` | One-hour in-memory cache cannot survive backend restart; check/mark must not be mistaken for atomic durable admission. |
| Snapshot/JSONL ingestion | `store.rs:387-455,495-508,860,1037-1038,1206`; `jsonl_ingest.rs:256-280` | Database dedup does not deduplicate external tool effects; preserve native source identities. |
| Fork | `daemon/server.rs:3415-3431,3468-3479`; `store.rs:662-683,700-731` | Existing in-flight guard and persistent operation lookup are useful seams. Require same operation identity on retry and inspect ambiguous native-fork/lineage-commit crash window. |
| Abort/restart | `session-engine.ts:140-145`; `agent.rs:842-923` | Abort is best-effort; restart may recreate process. Do not blindly replay either as a completed durable step. |
| Externally owned TUI input | working-tree `tui_rpc.rs:1-14,87-98,216-229` | Lease/connection authority is not a durable execution receipt; control additions are not HEAD evidence. |

## 6. Replacement candidates (not implementation claims)

1. **Pi process supervisor/command boundary:** `pi_manager.rs:4559-4617` and runner restart path `agent.rs:842-923`. Substitute a durable executor behind runner APIs, retaining routing authorization. Require durable command admission/result receipts before claiming crash-safe prompt/tool execution.
2. **Ephemeral replay ring:** `pi_manager.rs:520-620`. Could source reconnect continuity from durable event positions; retain atomic snapshot/tail boundary and explicit drift. Do not conflate durable log position with native session entry or message IDs.
3. **AgentEnd snapshot and file catch-up:** `pi_manager.rs:4136-4282`; `jsonl_ingest.rs:186-284`. Candidate for stable source-event projection rather than fingerprint/count convergence. Preserve oqto-log read contracts unless a separately approved authority migration changes them.
4. **Frontend outbox/ACK contract:** `transport.ts:90-141,193-196,277`; backend cache `ws_multiplexed.rs:48-100`. Replace volatile acceptance with durable operation receipts and explicit queued/accepted/running/completed/unknown states. Browser ACK or persisted chat message is not proof of exactly-once effects.
5. **External TUI adapter:** working-tree `tui_rpc.rs:33-55,87-98`. Native observations are explicitly private translator input, never canonical replay (`:5-6,64-65`). Read errors require fresh binding and canonical snapshot (`:87-89`); dropping adapter closes only its socket, not terminal-owned Pi. Retain lifecycle ownership and fresh authorization when changing executor technology.

## Required proof before declaring migration safe

These are proposed verification operations, **not performed audit results**:
- Kill Pi, runner, and backend separately before/after command acceptance and after an external side effect but before receipt persistence; recover with the same operation ID and demonstrate no duplicate effect (or explicit unknown/manual-reconciliation state).
- Reconnect within replay window, below replay floor, across stream UUID change, and with an ahead-of-tail cursor; compare stable canonical IDs and prove snapshot-to-tail continuity without text/index/count matching.
- Reload with an unacknowledged outbox send, including server TTL expiration/restart; demonstrate durable receipt lookup, not only in-memory cache dedup.
- Exercise both oqto-ui and legacy frontend consumers; prove reconnect invalidates durable pages even without a turn-end event.
- Replay history/source events twice and prove no prompt, tool, fork, restart, or external TUI input is executed by projection.
- Test imported public/native bindings, fork source-entry mapping and operation replay, revoked machine/workspace access, and missing continuation file; verify no identity or routing fallback selects another session/runner.
- Test equal-length JSONL changes/truncation and persistence failure before idle, rather than assuming source catch-up always converges.

Bottom line: current code has durable canonical history, identity bindings, bounded in-memory event replay, transactional history dedup and some operation-specific fork protection. It does **not** establish general crash-safe exactly-once agent/tool execution. The strongest migration seam is the authorized runner executor/receipt boundary while preserving canonical history and frontend identity contracts.


---

# Appendix B — oqt execution and permission findings

# Read-only audit: oqt / oq-permissions

Target: `/home/wismut/byteowlz/oqto-durable-support`
Revision: `52e492982d8745fe9e47285729bf67486bbf3988` (recent `!cmd` / `!!cmd` commit).
All file references below are relative to this target.

## Method and confidence

Read root and backend `AGENTS.md`, source, recent commit metadata, ADR-0053, and relevant test definitions. No target programs, builds, tests, sessions, sockets, providers, or shell effects were executed. Only this report was written. Existing git dirt (primarily migrated tracker files and deleted todo files) was left alone. No tracker mutations were made because this is expressly read-only.

“Source-proven” below means the control/data flow is visible in source, **not** dynamically reproduced. Existing tests are described as assertions present in files, not as tests passed in this audit. This is a focused audit, not an exhaustive security certification.

## Summary

The new shell path bypasses more than the approval prompt: it also bypasses admission/resource-expiry synchronization, invocation journaling, and authority readiness. Its purported serialization does not include the brain's actual tool execution. Its output is kept in an unkeyed client-global queue, rather than Session state. These are meaningful regressions relative to the guarded model-tool path.

The Rust ledger has substantial exact-operation, binding, CAS, expiry, and generation-fencing mechanisms. It is not itself an execution authority or an exactly-once history store. The recent operator-shell path does not participate in those mechanisms.

## Findings

### F1 — Medium: operator shell can use expired resource mounts indefinitely (source-proven)

Evidence:
- `tools/oqt/guard.ts:336-371`: `operatorShell` awaits initial `ready`, then executes directly against `supervisor.epoch`; there is no `syncResources()` or `authority.ready()` check.
- `tools/oqt/guard.ts:468-476`: normal brain preparation explicitly synchronizes resources before returning its execution epoch.
- `tools/oqt/guard.ts:594-603`: resource synchronization compares current ledger grants with the mounted spec and replaces hands when necessary.
- `backend/crates/oq-permissions/src/ledger.rs:545-567`: active resource enumeration filters expiration.
- `tools/oqt/hands-supervisor.ts:221-236`: `replace()` without a spec stops/reinstalls hands but leaves the last spec unchanged.

Scenario: approve an outside path or host network, let its lease expire while idle, then type `!` commands without intervening model calls. They use the old mount/network spec. Replacement after each shell retains it, so expiration need not take effect at all. The operator is local/trusted, so this is not unauthenticated privilege escalation; it defeats the advertised time-bounded grant semantics and can disclose expired-resource output to the next model prompt. It also extends an expiring resource's use to the shell path's 120-second budget, not the documented 30-second tool budget.

Needed proof/fix: expire a grant, issue only operator shells, assert outside path/network inaccessible and no stale output delivered; use a common authority/readiness/resource synchronization admission path without adding approvals for the human's own command.

### F2 — Medium: shell is not serialized with actual brain calls; revoke is delayed behind shell (source-proven)

Evidence:
- `tools/oqt/guard.ts:339-380`: operator shell holds `#serial` across the entire exec and replacement.
- `tools/oqt/guard.ts:458-529`: brain tool handler does not run its call lifecycle under `#serial`; `prepare` and `drain` may execute concurrently with operator shell. In particular `drain` calls `supervisor.replace()` directly at line 515.
- `tools/oqt/gated-tool.ts:50-100`: model execution occurs outside the guard, between prepare/check and drain/check.
- `tools/oqt/guard.ts:608-619`: brain-loss handling and revoke queue on the same serial chain held by the operator shell.

Scenario: client A has a running approved model command and client B sends `!cmd`. Both can run against the same generation; either completion can retire the other's executor. Conversely, a running `!sleep 120` delays `/permissions revoke` and brain-loss processing until the shell exits, rather than promptly freezing and stopping effects. Multiple clients are an explicitly supported operating mode.

Needed regression: overlap operator/model shells and assert chosen ordering; revoke during operator shell and assert bounded prompt descendant teardown. Serialize invocation lifetimes, not just some state-transition methods, and provide an interrupt path that does not queue behind the work it must stop.

### F3 — Medium: canceled/disconnected queued operator shells still execute (source-proven)

Evidence:
- `tools/oqt/guard-wire.ts:77-80`: disconnect aborts per-call controllers.
- `tools/oqt/guard-wire.ts:111-134`: queued handlers still get run as earlier handlers finish; the queue is not discarded on close.
- `tools/oqt/guard.ts:697-699,748-752`: operator handler discards the supplied signal (`_signal` in the serve callback; the operate method has no signal parameter).
- `tools/oqt/guard.ts:808-816`: shell accepts command and delegates without cancellation.
- `tools/oqt/guard.ts:339-359`: queued shell uses `BACKGROUND_CONTEXT`, not the caller's cancellation.

Scenario: send a slow shell followed by a write-effect shell from another operator client, then cancel/close that client while it waits. The queued command can execute later after its caller has gone. This is local operator-intent/lifecycle failure, not a remote attacker route. A retried command can consequently duplicate effects.

Needed proof: canceled-before-start shell never writes a marker; disconnect during exec promptly drains descendants or explicitly reports retained execution under a durable job identity. Check abort state at dequeue/start and propagate cancellation to execution.

### F4 — Medium: `!` output crosses Session boundaries and has no pending-output recovery/sync (source-proven)

Evidence:
- `tools/oqt/plugin/src/tui.ts:18-26`: one process-global `string[]` under `Symbol.for("oqt.shell-context")`; no Session key.
- `tools/oqt/plugin/src/tui.ts:337-373`: shell knows its Session, but pushes only formatted command/output into that shared array.
- `tools/oqt/source_patches.py:27-36`: next `#submitPrompt` unconditionally drains the global array into whatever controller is selected.
- `tools/oqt/session-commands.ts:42-54,63-91`: `/new` and `/resume` switch Sessions without transferring/clearing a keyed shell-output record.

Scenario: run `!cat sensitive-project-result` in Session A, `/new` or `/resume B`, then send a normal message in B. A's output is prepended to B's prompt/history. Same workspace is the documented isolation scope, but this is still an unintended disclosure and corrupts per-Session provenance. A second client attached to A cannot see pending output, and closing the first client loses it. The queue is spliced before submission is known successful, so a failed submission also risks losing it.

Needed proof: A→B switch, reconnect before submission, two-client prompt submission, and submission failure; assert exact Session ownership and intentional consumption. Decide whether pending shell output is durable or explicitly ephemeral, and expose that truth to the user.

### F5 — Medium: operator effects are not journaled; crash loses uncertainty/history (source-proven)

Evidence:
- `tools/oqt/guard.ts:336-371`: operator exec never calls authority prepare/begin/finish and has no command invocation ID or durable result record.
- `backend/crates/oq-permissions/examples/permission_carrier.rs:234-247`: normal coding admission journals before effects via `authority.begin`.
- `backend/crates/oq-permissions/src/ledger.rs:494-523`: recovery discovers only live invocation rows, and revokes grants only when such orphaned rows exist.
- `tools/oqt/plugin/src/tui.ts:369-373`: context is queued only after shell returns successfully.

Scenario: `!cmd` writes a file or performs an approved-network effect, then host/client dies before the response is received. Reopen cannot report that shell as interrupted or expose a durable effect disposition; if it was the only invocation, ledger recovery has no row causing grant invalidation. Blind manual retry can duplicate an effect. `!!` privacy does not require omitting an opaque operation ID and interrupted status.

Needed proof: kill host mid-operator-command, reopen, assert an interrupted identity/status and no implicit replay. Record effect disposition outside model context even for `!!`.

### F6 — Medium: remembered approval mutates persistent authority before digest/revision validation (source-proven)

Evidence:
- `tools/oqt/guard.ts:769-783`: `remember:true` finds by ID and writes the repository rule.
- `tools/oqt/guard.ts:784-788`: only afterwards calls `authority.decide` with reviewed digest/revision.
- `backend/crates/oq-permissions/src/ledger.rs:362-374`: actual stale/unreviewed digest, revision, state, expiry validation occurs inside decide.

Scenario: an operator request with a valid currently pending ID but stale/wrong reviewed digest/revision and `remember:true` persists the “always allow” category/prefix, then receives a failed decision. Future Sessions can auto-allow it despite that failure. Concurrent operators can encounter this class of stale review; no arbitrary remote actor access is claimed. File mutation is not rolled back when decide fails.

Needed proof: submit wrong digest/revision plus remember; require both a rejected decision and unchanged rules bytes. Coordinate persistent remember with successful exact decision, avoiding a new race between validation and persistence.

### F7 — Low/Medium reliability: advertised 64-KiB output can break the operator connection after an effect (source-proven)

Evidence:
- `tools/oqt/guard.ts:342-355`: output keeps 65,536 JavaScript code units.
- `tools/oqt/guard-wire.ts:43-46`: entire JSON response must fit 65,536 **bytes**, including envelope/escaping.
- `tools/oqt/guard-wire.ts:116-128`: oversized successful-response write throws and destroys socket.

A command emitting 65,536 ASCII characters exceeds the response frame once JSON overhead is added. Quotes, backslashes, control characters, and non-ASCII make the mismatch worse. The effect already happened; the user gets connection failure instead of output, potentially losing permission subscription too. This is not a strict 64-KiB last-output implementation.

Needed proof: ASCII boundary, JSON-escaping-heavy output and multibyte Unicode. Bound encoded response bytes or stream/chunk a separately bounded result; retain truncation metadata.

### F8 — Low security/availability: inbound “bounded” guard framing does not bound complete frames (source-proven)

Evidence: `tools/oqt/guard-wire.ts:20-40` parses/removes every newline-terminated line before checking only the residual buffer's length. It never checks the completed line's byte length before `JSON.parse`.

A socket-reachable client, including code in a confined brain with its own tool socket, can send a complete oversized frame and force allocation/parsing in the unconfined guard. This is a same-user/compromised-brain DoS surface, not an unauthenticated network endpoint. Chunk timing affects whether an oversized frame is rejected, which a framing limit should not permit.

Needed proof: oversized terminated frame in one delivery and across multiple deliveries, plus Unicode byte limits; reject before parsing.

## Additional replay/failure gaps

1. **Drain failure erases the live effect journal anyway.** `tools/oqt/gated-tool.ts:96-97,116-123` marks drain attempted before awaiting it, then always calls finish even if drain throws. `permission_carrier.rs:261-264` ends/deletes the live journal row; `ledger.rs:484-490` implements that deletion. `guard.ts:513-517` does not freeze authority through `authority.transition` for ordinary drain. Therefore an unconfirmed executor teardown can be reduced to a generic uncertainty error with no retained interrupted journal, and recovery may have nothing to fence. This is a source-visible failure-path gap, not a reproduced teardown failure. Add fault injection for stop timeout/replace failure and ensure authority remains frozen and the invocation inspectable.
2. **No exactly-once receipt in the ledger.** `ledger.rs:467-490` prevents re-entry only while an invocation row exists; `end` deletes it. `admit` can return an approved permit again for the same unchanged request (`ledger.rs:212-273`). The carrier removes its in-memory permit at finish. Thus `prepare → finish → prepare` of a previously approved unchanged ID is not intrinsically a consumed-once operation. This is not necessarily a bug in the crate's contract: the trusted Durable caller owns task identity/replay, with `replay:"unsafe"` at `gated-tool.ts:37`. It is a gap if consumers assume the permission ledger alone deduplicates completed effects. Add a carrier-level regression documenting intended semantics and a crash-at-effect/commit-boundary Durable trace.

## Actual tests versus claims

No tests were run in this audit; the following are **test definitions**, not fresh pass evidence.

- `tools/oqt/bang_tui_test.py:17-43` has one happy-path real-TUI scenario: writes/reads a workspace file, runs silent output, attempts owner directory listing, asserts no model call/approval, then checks visible output included and silent output excluded in the next provider request. It does **not** cover expiration, concurrent brain work, revoke during operator exec, cancellation/disconnect, host crash, two clients, Session switching, large/Unicode output, or durable reload of shell history. Owner-mask evidence checks absent filenames, not every hard deny/mask class. `justfile:174-175` defines the bang gate and line 181 includes it in the combined UI gates. A gate definition/commit claim is not a captured successful run.
- `tools/oqt/multiclient_tui_test.py:29-73` covers one shared model turn, one approval winner, stale decision failure, exact single file effect, shared turn visibility, first-client departure and final host retirement. It contains no `!`/`!!`, pending shell context, or operator/model execution overlap.
- `tools/oqt/recovery_tui_test.py:36-83,109-150` covers brain/host kills during **model-issued approved** shell and fresh approval after recovery. It does not cover operator shell. In the first case, `loops` and `executed` at lines 74/79 are computed but not asserted; uncertainty/fresh approval and heartbeat settling are stronger direct evidence than the blanket “no replay” print statement. An append-only exact execution marker would strengthen that claim.
- `tools/oqt/resources.test.mjs:27-103` asserts outside mount before/after approval, read-only behavior and explicit revoke removal; lines 107-163 assert network/write grants. These are meaningful real confined-tool tests but do not exercise operator-shell expiry.
- `tools/oqt/gated-coding.test.mjs:46-60` rejects brain access to decide/revoke/ack/subscribe, but the newly added `shell` method is not in that rejection list. The source switch does reject it (`guard.ts:537-538` default); add it to the regression rather than treating missing test as a proven bypass.
- `backend/crates/oq-permissions/tests/lifecycle.rs` includes exact arguments (135), distinct once operation (172), grant scope (193), reopen (256), competing CAS decisions (280), wrong operator/digest/revision (318), revocation (429), backward-clock expiry (452), cross-binding permits (466), cross-ledger replay (637), orphan recovery (683), and clean recovery (719). These test the authority abstraction, not operator exec or real history-provider auth.
- `backend/crates/oq-permissions/tests/ceiling.rs:300,350,434` covers resource expiry/revocation enumeration, explicit policy replacement, and generic-vs-prefixed shell grants. Again, no proof that every execution entry point synchronizes mounts.

## Boundaries and things not to misreport

- Operator boundary is local OS ownership, not Account auth. Guard sockets are private and brain tool dispatch excludes approval/operator methods. Lack of Account auth is an explicitly declared production gate, not an unauthenticated RCE finding.
- Guard/host are trusted and unconfined; brain gets provider network and access tokens by design. Network grants expose localhost by design. Those facts need deployment care but are not new vulnerabilities by themselves.
- `tools/oqt/host.ts:48,143-144` binds the workspace profile's entire Session directory read-write into each brain. The README explicitly acknowledges sibling-Session visibility. Do not claim per-Session OS history isolation; the brain-confinement test's title (“no ... other sessions”, `brain-confinement.test.mjs:19`) is broader than the production bind scope.
- `permission_carrier.rs:275-286` history search returns fixed fixture strings. The Rust crate does not authorize actual hstry/oqto-log data. ADR-0053:53 explicitly leaves real history translation, provider integration and authenticated multi-client audience controls as production gates. Synthetic history tests cannot establish real history isolation or sync.
- Upstream Durable source is patched/pinned externally; this audit inspected the owned integration and exact patch definitions, not an independent end-to-end verification of the entire upstream history/sync engine.
- Stale-model-tool output is deliberately buffered and checked before delivery (`gated-tool.ts:53-100`), and permission decisions are strongly bound in the Rust layer. Preserve these strengths while bringing operator-shell lifecycle under a common execution contract.

## Priority acceptance work

1. Common invocation lifetime/resource fencing for operator and model paths; prompt revocation and cancellation.
2. Session-keyed shell context with explicit durability, consumption and multi-client semantics.
3. Operator invocation interruption journal and uncertain-stop fault injection.
4. Exact-decision/persistent-remember atomicity.
5. Encoded-byte output/framing limits and boundary tests.

Nothing here establishes a tested production authentication/history contract. The existing local proof is valuable, but it does not justify extending its recovery, serialization or grant-expiry claims to the recent `!` path without the regressions above.


---

# Appendix C — Upstream storage/task specification caveats

# Upstream read-only audit supplement

## Scope and confidence

Read in full, without executing examples or authenticated requests:
- `/home/wismut/.local/share/oqt/pi-source/packages/durable/docs/spec.md` (5123 lines)
- `/home/wismut/.local/share/oqt/pi-source/packages/durable/docs/pico-v5-handoff.md` (917 lines)
- `/home/wismut/.local/share/oqt/pi-source/packages/durable/docs/pico-v5-chord-usage.md` (387 lines)
- `/home/wismut/.local/share/oqt/pi-source/packages/ai/README.md` (1949 lines; including internal README cross-reference sections).

These are documentation-contract findings, not independently verified implementation/test results. No repository edits. Below, S = `/home/wismut/.local/share/oqt/pi-source/packages/durable/docs/spec.md`, H = `/home/wismut/.local/share/oqt/pi-source/packages/durable/docs/pico-v5-handoff.md`, C = `/home/wismut/.local/share/oqt/pi-source/packages/durable/docs/pico-v5-chord-usage.md`, A = `/home/wismut/.local/share/oqt/pi-source/packages/ai/README.md`. All references are exact file line ranges using these absolute-path aliases.

## 1. Storage ownership and transaction authority

- **Session is the sole mutation/persistence authority**, atomically committing entries, complete task replacements, and document changes. Only committed state is observable; external effects stay outside the mutation transaction (S:41–79). The application owns the open Session; Chord facets own disposal of acquired states, not the Session or document (C:126–132).
- Session holds one mutation line through asynchronous callback, preparation, storage settlement, adoption, and publication enqueue. Commit observers capture immutable state synchronously; user watch callbacks run off-line. Cancellation after commit admission does not undo or interrupt storage settlement (S:1588–1620).
- **Ownership boundary:** commit arguments are borrowed until `Storage.commit` settles; retained containers must be detached; memory recursively copies, JSONL/SQLite detach via serialization/decoded indexes, and every storage read returns detached JSON (S:1630–1637). Conversely, Session snapshots/tracker revisions may share containers: immutability is trusted, not enforced by freezing (S:1344–1362).
- Drafts are revocable, assigned values copied and strict-JSON checked. Prepare computes the candidate before storage admission; success adopts by pointer swap; failure publishes nothing. Preparation/validation/checkpoint failures roll back normally rather than poisoning the Session (S:1364–1432).
- Storage enforces atomicity, ID ownership, immutable conversation/entry creation, document consistency and detached values, but trusts Session semantic validation. IDs come from one globally increasing namespace across reopen (S:4711–4725). Session—not an additional caller-facing storage mutex—serializes commits (S:4760–4770).
- Definitions own checkpoint policy. Creation/version changes require bases; current-only bases permit reclamation, rewindable bases never authorize deleting addressable history; absent checkpoints can yield unbounded replay tails (S:1434–1482). Fork copies use committed pre-batch stored values/versions, independent new incarnations; no copied tasks/task documents (S:1521–1568).
- Chord state/views/events own **no second persistence authority**. State acquisition atomically snapshots/subscribes; remote successful commit does not imply callbacks have completed; shutdown detaches clients/disposes services before closing Harness (C:47–52, C:161–185; S:4345–4367).
- pi-ai is a provider/request layer, **not** a durable task store: Context serialization example makes the application save/restore JSON itself (A:1603–1640). CredentialStore persistence is separately application-injected; its serialized `modify` path owns OAuth refresh concurrency (A:428–442). Dynamic catalog storage is also separate, defaulting to memory (A:1304–1328).

## 2. JSONL transactional semantics and durability caveat

- Layout is **not just one append-only transcript**: `main.jsonl` carries table writes, document records and commit markers; `doc-<id>.jsonl` carries a document incarnation; `task-<id>.jsonl` carries live task replacements (S:4914–4927).
- Every commit: append complete prepared affected sidecar records → append complete main marker listing them → publish in-memory candidate only after marker write succeeds. No standalone sidecar fast path (S:4929–4936; H:70–77).
- **`fsync` defaults false.** Default guarantees ordinary process-crash consistency, not power/host/kernel/filesystem-failure survival. With true: append sidecars, flush each affected sidecar, then append main marker. **Ordinary publication does not explicitly flush main.jsonl, even with fsync true: acknowledged tail commits may disappear.** It provides sidecar-before-marker ordering, not a blanket acknowledged-commit power-loss durability guarantee. Main-only commits have no sidecar flush (S:4935–4947; H:79–86).
- Before destructive reclamation with fsync true, flush main once to stabilize the authorizing marker; failure defers reclamation without undoing/failing already-published state. Nonempty temporary replacements are flushed before rename (S:4942–4947; H:94–107).
- Recovery removes torn final lines and unconfirmed sidecar tails; only confirmed records apply. Missing required confirmed data is corruption/open failure, except a later latest base or retirement can prove an old physical record unnecessary. Uncertain append failure poisons open backend (S:4949–4957). H explicitly includes uncertain publication append/flush failure and distinguishes pre-I/O serialization/preparation failures (H:70–90).
- Reclamation follows committed base/retirement authorization, deletes empty sidecars or replaces via temp+rename, invalidating cached descriptors to avoid appending to unlinked inodes. **Initial implementation does not compact main.jsonl** (S:4959–4964).
- Any uncertain storage commit failure fails the Session; reopening may find it committed or not. Retry admission after reopen with the same requestId, not a blind fresh identity (S:4824–4832). Read-invalid-request/caller-cancel exceptions are narrow; commit/mint failures never exempt (S:4857–4863).

## 3. Task replay: guarantees versus non-guarantees

- Open changes surviving running tasks to pending with checkpoint/abort mark preserved; waiting stays waiting; completing/waiting are reconciled. No handler executes during open; scheduling requires resume or a progress API. Missing/incompatible definitions block work rather than silently terminalize it (S:629–663, S:1898–1915).
- Durable reservation precedes execution. Runtime commits gate against invocation lifetime/abort mark and atomically replace task checkpoint/outcome with entries/documents/child creation; full checkpoints, not implicit local stack replay (S:1794–1814, S:1860–1897).
- **Effect sandwich is not exactly-once external execution:** commit intent → external effect → commit outcome. On recovery at intent, effect may already have happened; author must retry idempotently, poll a stable external handle, or record interruption (S:1917–1933). Memos are durable first-writer-wins values, not universal effect deduplication.
- Tool default is unsafe (S:2866–2869). Intent persists final arguments and resolved replay policy; recovery skips beforeTool and reuses arguments. Randomness, timestamps and mutable config-derived inputs requiring stability must first be durably captured (S:3170–3179).
- Recovered execute reruns **only if both stored intent policy and current resolved declaration are safe**. Current unsafe/missing can veto stored safe; current safe never upgrades stored unsafe. Otherwise interrupted result uses committed partial output/details/diagnostics (S:3306–3313, S:3903–3907). Safe replay is a tool author's assertion, not proof of idempotency. Environment is rebuilt with current cwd, so it need not match first attempt (S:2993–3001).
- Nested calls deduplicate admission by caller-local key, reuse committed results, and reject tool/argument mismatch. Default positional keys demand same rerun ordering; explicit keys provide stable identity. A crashed nested call still follows its own replay policy: **explicitly not exactly-once execution**, external effects still require idempotency (S:2890–2942).
- Replay-unsafe callers' nested/created tasks normally carry abandonOnRestart; scheduling after reopen marks abandonment, not inspection-only open. Missing cleanup definitions stay blocked for restart marks; explicit abort can orphan them, leaving external effects uncleaned (S:2075–2113, S:2900–2905).
- Submission requestId deduplicates admission within one conversation, before writes; cross-type reuse rejects (S:2300–2303). Creation and first submit are separate commits: host must durably rediscover created conversation before retrying submit after a crash (S:668–680). Provider tool call IDs are not Session-unique and must not substitute for durable request IDs (S:3220–3230).
- Generation recovery resends committed messages with pinned model/thinking/stream options but reruns beforeRequest; provider request/effect may repeat (S:3786–3799). Retry re-prepares, so later settings/agent changes can apply (S:3833–3837). Deferred handle/poll deadlines persist (S:3678–3688, S:3833–3841). Compaction recovery likewise resends pinned request; an interrupted uncommitted response has no counted usage (S:4082–4098, S:4145–4151).
- Committed progress survives; current uncommitted throttle window can be lost. Committed generation partials become aborted entries before restart/retry (S:3548–3555, S:3786–3799). Raw pi-ai `partial` is mutable shared live data, not an event-time snapshot; compact frame encoder/reducer has its own ordering validation and requires separate terminal settlement (A:703–705, A:726–759).
- Structured concurrency: terminal receipt guarantees ordinary owned work drained; completing outcomes run no more task code, retain outcome, and defer task-doc retirement/waiters until final commit. Background subtrees are excluded and may continue (S:2047–2051, S:2156–2212). Close is shutdown, **not durable abort**; it joins all invocations and writes no task outcome. Noncooperative code can indefinitely delay close; old/new Harness must never concurrently own same Session (S:1898–1915, S:3519–3525).

## 4. Observation and replay caveats for integration

- Watches buffer at most 100 pending committed frames; overflow replaces undelivered suffix with full-state root replacement. Convergent state observation is **not an audit journal**; persist immutable facts and scan them if every transition matters (S:4281–4295; C:318–339).
- Experimental event stream similarly overflows to snapshot and reconnects from a new snapshot with **no event replay**; exact submission outcomes use Submission.wait (S:4522–4532). Chord delivery sequence is in-memory stream sequencing, not persisted Session commit sequence (S:4235–4242; C:19–22).
- Failed/aborted runs leave queued followups/writes in inbox; waits can remain unsettled until a later submission boundary or withdrawal. Idle alone does not guarantee queued passive-write placement (S:902–909, S:2357–2367).
- pi-ai normalization/system replay supports context transfer, not task recovery. Providers may collapse positional system messages and transform cross-provider thinking. pi-ai permits abort continuation, whereas Pico excludes aborted/error/deferred assistants from future provider requests: distinguish layers (A:1139–1171, A:1522–1532, A:1570–1601; S:294–298).

## 5. Documentation inconsistencies / evidence limits

- Spec still says “Pico5 is not implemented yet” (S:310–313), while handoff status says packages 1–23 implemented (H:10–14) and final conformance says done with named test files (H:897–917). Treat initial wording as stale, not evidence of runtime absence.
- H contains superseded staged APIs (`ConversationConfig`, old registry batch/dispose, pi.post-tools) later explicitly replaced in packages 19/21 (H:291–343, H:614–664, H:825–847). Use normative S for current contract, not an isolated early handoff milestone.
- H:282–285 says prepare views before Storage admission and never rebuild from committed publications; normative S:4345–4367 and later H:587–590 describe advancing mounts from subscribeCommits publications. This is a real handoff/spec inconsistency; do not infer current implementation from that early bullet.
- S:4935–4947 is materially more precise than the final JSONL footgun shorthand (S:5104–5107): **fsync true still lacks ordinary main-marker flush**. Any audit summary claiming full acknowledged power-loss durability from that option is overstated.
- Named conformance/lifecycle/crash tests in H are documented acceptance/status evidence only. This read-only supplement did not inspect or run their implementations, and made no authenticated provider calls.


## Final non-destructive check

Before report creation, `git status --porcelain=v1` matched the initial capture exactly. A SHA-256 manifest of pre-existing modified/untracked files, captured before report synthesis, was checked after synthesis: **zero existing files changed**. The only added repository artifact is this report. Tracker metadata and separately committed coordination-board messages are intentional administrative changes. The pre-existing tree was dirty, so a clean-tree claim would be false.
