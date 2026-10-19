# Local classic import and native conversation navigation

## Status

Implemented and locally qualified, 2026-10-18: `oqto-g7rk.3` and
`oqto-g7rk.5`. Source-only oqt; no installation, deployment, hosted history
migration, remote synchronization or mobile qualification. Refines ADR-0063/0064.

## Context

Classic Pi JSONL is a source archive, not a native task store or an authority
source. Selecting a native conversation also changes transcript observation,
model configuration and the target of input and operator output. Changing just
an input pointer would mix branches, including across two attached clients.

## Decision

### Import is a protected, immutable bootstrap

- An explicit origin namespaces the classic UUID. The destination catalog namespace,
  source identity, exact UTF-8 digest and selected source leaf determine an opaque
  catalog Session ID. This ID is neither the classic UUID nor an Oqto platform ID.
- Local source reading is bounded, read-only, no-follow at the file, strict UTF-8,
  and verifies descriptor/path identity and nanosecond metadata before/after.
  Discovery uses an explicit root and finite traversal limits. Selection pins the
  discovered digest; a changed file requires inspection and reselection.
- Only bounded peer-owned begin/chunk/commit uploads cross the operator transport.
  The original source path never becomes a host execution path. Imported cwd,
  model names, thinking preferences and extension records do not select runtime
  policy, credentials, grants or workspace scope.
- The worker alone writes native SQLite through Durable APIs. It retains the exact
  source bytes, identities, parent links, inert records and native mappings, and
  separately materializes the selected context through native typed message/tool
  entries, without task creation. A model-empty checkpoint owns the context head;
  mappings record actual native materialization IDs by source ID, and the watermark
  is the final written entry. Hidden custom context stays TUI-hidden; original
  extension renderer parity is not claimed. Unsupported selected semantics,
  including incomplete tool exchanges, fail before mutation. No archived task,
  submission, extension or tool call is scheduled.
- Host-owned protected publication receipts outside the brain-writable Session
  directory govern visibility and admission. A reserved target is hidden and
  effects/input are denied until bootstrap commits, its worker closes and the host
  publishes. Missing or malformed publication evidence fails closed, including
  after host restart and deletion of a writable pending marker.
- Exact retry reuses the snapshot. Source append publishes a new immutable
  snapshot, with the last actually published predecessor when present; it never
  mutates a destination that may already have local continuation. Rewrite,
  truncation and changed-payload source-ID reuse fail closed.

Archive ancestry is not represented as a fictitious native task/fork graph.
Native branching at an inert archive-only entry is rejected with source-leaf
selection guidance; selecting another classic point uses explicit `--leaf`.
Native continuation and its subsequent native forks retain normal Durable entry
and conversation identities.

### Navigation is one worker-owned transition

A Session-scoped selected-conversation document records the native ConversationId.
Startup binds that handle before accepting input. The router changes controller,
transcript and model observation coherently; it serializes mutations, refuses
navigation during live native work, and fences stale client targets. Old watch
callbacks cannot overwrite the new branch. A partial failed transition cannot
leave admission open against a different durable target.

Remote input leases do not authorize navigation: `select`/`fork` require local
control, while graph queries remain observation-only. Local navigation reclaims
the input lease and drains its recorded queued submissions before dispatch.
The authority regression uses the actual worker service ID to prevent drift.

Forks use native visibility/ancestry validation and native fork document policies;
they do not copy live tasks, grants or result bindings. Public Session, native
conversation, source entry, native entry and request identities remain separate.

### Operator results bind the conversation before effects

Receipt v2 extends ADR-0064. The observed transcript conversation is explicit in
operator shell intent. Guard admission pins that destination before asynchronous
work, rejecting stale selection. A two-phase guard selection fence excludes both
queued/live operator effects and navigation until the worker transition commits
or aborts. It is not a grant.

A bounded Session-scoped native receipt index records invocation ID, digest and
explicit ConversationId. It is not cloned into forks. Listing filters the exact
Session/conversation; passive result admission resolves the recorded conversation,
not the current selection. Recovery reconciles sealed acknowledgments across all
branches, including when another branch is selected. The protected 128-invocation
capacity remains global to the Session and cannot be reset by branching.

`!!` remains an opaque envelope with no command/output in the brain or native
history. Passive writes do not start inference. No uncertain external effect is
replayed, and external exactly-once execution is not claimed.

## Verification boundary

Native SQLite, actual bounded transport and composed confined-hands tests cover
storage, rejection, publication, identities, branch result routing and recovery.
Integrated TUI, actual provider-request capture, two-client navigation/stale picker
rejection and reopen passed before issue closure. Identity-addressed queued-input
cleanup resolves the recorded submission's original conversation, independently
of the current view; it cannot cancel passive receipt writes or borrow another
attachment's input authority. See the completion evidence report in
`docs/development/oqt-import-forks-completion-2026-10-18.md`. Process-crash evidence is not power-loss
qualification. Hosted canonical Chat history remains `oqto-log`; this local
composition neither writes Pi JSONL nor introduces a permission-store timeline.
