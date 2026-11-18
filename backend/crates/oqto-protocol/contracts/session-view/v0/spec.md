# oqto.session-view v0

Status: **draft for review, revision 2.2 (2026-11-15)**. The owner answered the first
revision's open questions and asked for client caching; revision 2.1 added paged entry
reads, the store marker and caching rules. Revision 2.2 adds generative UI parts as
`oqto.ui` entries (owner requirement for the terminal client's first slice, ADR-0087
decision 8). Implemented by the in-process reference mapping used to capture the golden
frames (`scripts/contracts/session-view/`), the byoagnt kit, and the Rust
`oqto-session-view` and `oqto-session-client` crates. Do not ship against it before it is
accepted. Tracking issue: `oqto-1b8f.6.3`. Decision records: ADR-0074, ADR-0080, ADR-0083,
ADR-0087.

## What this is

One contract for showing and steering one Session that runs on Pi Durable, the same for
every client: web, phone, desktop, terminal, and other agent hosts.

It is a set of **frame shapes and rules**, not a wire. A binding carries the frames over a
concrete transport. The contract is the same over every binding.

Pinned upstream: `@earendil-works/pi-durable` 1.5.2 and `@earendil-works/chord` 1.5.2.

## What it is not

- It grants nothing. Frames carry no caller identity. The binding authenticates the client
  and the host decides what that client may see and do.
- It has no approval operation. Permission prompts and owner decisions are a separate
  contract (`oqto-1b8f.6.2`). No frame here approves anything, and a receipt never means
  "approved".
- It is not a history store. It shows a window over the newest entries and lets a client
  read older or missed entries page by page.

## Terms

- **Session**: one durable conversation, named by an opaque id the host gave out. Never a
  path, a store name or an upstream id.
- **Host**: the process that owns the Session and speaks this contract.
- **Attachment**: one client's live observation of one Session.
- **View**: the value an attachment keeps current. It is upstream's `ConversationView`.
- **Receipt**: the durable status of one request the client made.

## Bindings

A binding must give:

1. Ordered, reliable delivery in both directions for the life of a connection.
2. One frame per message, encoded as a JSON object in UTF-8.
3. Client authentication before the first frame. The host stamps identity; clients never
   send it.
4. An end-of-connection signal. When a connection ends, all its attachments end silently.

Planned bindings: WebSocket for local and web clients, and the signed personal transport
for paired devices. Neither is specified here.

## Frames

`frames.schema.json` is the normative shape of every frame. Unknown fields and unknown
frame types are refused, not ignored.

Client to host: `attach`, `detach`, `submit`, `write`, `abort`, `receipt_query`,
`read_entries`.
Host to client: `attached`, `detached`, `view`, `receipt`, `entries`, `ended`, `error`.

Every client frame has an `id`, unique within the connection; reusing one closes the
connection. The host answers each one
with exactly one frame carrying the same `id`: the matching reply, or `error`. Frames the
host pushes on its own (`view`, `ended`, and receipt updates) have no `id`.

A request that is not a JSON object, has no valid `id`, or reuses an `id` already used on
the connection cannot be answered unambiguously: the host closes the connection, and that
connection's attachments end silently. A request with a valid, unused `id` is always
answered, with `error` when it is malformed.

## Attach and the view

`attach` names the Session, the contract version `v`, and an optional window.

The host replies `attached`, which names the Session's `store`, and then sends `view`
frames for that attachment:

- `seq` starts at 1 and rises by exactly 1. A client that sees a gap or a repeat must drop
  the attachment and attach again.
- Frame 1 is the base: its first op is `["r", value]`, the whole view.
- Every later frame holds ops that change the value the client holds. Apply them in order.
- Applying must be all or nothing per frame. If any op is refused (see Ops), the client
  must drop the attachment and attach again. It must never skip an op.

The view value has `conversation`, `entries` (the window), `before` (the id of the entry
just before the window, absent when the window starts the transcript) and `docs`.

**Window.** `min` defaults to 50 and `max` to `2 * min`. The window starts with the newest
`min` entries and takes appends. When it would hold more than `max`, the host drops the
oldest entries in one splice, down to `min`. Limits are in `operations.json`.

**Resume.** A client that kept entries can attach with `from` set to its newest entry id
plus one. If at most `max` entries are at or after `from`, the window starts there and
`before` equals the client's newest entry: nothing was missed. Otherwise the window starts
at the newest `min` entries and `before` is a later entry: the client has a gap and must
not join the two ranges. Reading older entries is not part of v0.

**Whole or nothing.** A client sees the whole Session view or nothing (`denied`). v0 has
no partial or filtered views. A later version that adds them must announce the filter in
`attached`, and clients must then keep caches per filter.

**Entries never change.** A committed entry id names the same content for the life of the
store; this is upstream's guarantee. Compaction, resets and context edits are new entries;
forks are new conversations. Only replacing the store breaks it, and that changes `store`.

**Stable fields.** Most of the view belongs to upstream and may change with the upstream
version. Clients may rely on the paths in `operations.json` `stable_view_subset` and must
tolerate unknown fields, unknown entry kinds and unknown documents everywhere else. Every
listed path occurs in at least one captured scenario.

**Idle.** A Session is working exactly when `docs["pi.live"].run` is present. Queued inputs
are in `docs["pi.inbox"].items`. Clients must not guess idleness from text or timing.

`retired: true` on a view frame is passed through from upstream and means the observed
value was retired. No captured scenario produces it.

## Reading entries

`read_entries` reads committed entries by id range, one page at a time, in `ascending` or
`descending` (default) order. `min` and `max` are inclusive entry ids, both optional.
`limit` is 1 to 200 (default 50). The reply `entries` holds `items` in the requested order
and, when more remain, `next`: an opaque cursor. Send it back unchanged, with the same
bounds and order, to get the next page. No `next` means the range is exhausted. A host may
return fewer items than `limit` to stay within its frame limit.

It maps onto upstream's `Conversation.entries()`. Ids have holes, so clients never compute
counts or ranges from ids beyond passing exclusive bounds as `min = a + 1`, `max = b - 1`.

## Caching

Clients cache so the Session appears at once. The rules:

1. **Key.** A cache belongs to one host, Session and `store`. If `attached` names a
   different `store`, drop the cache: entry ids may now mean different content.
2. **Fast start.** Show the cache first, then attach with `window.from` set to the newest
   cached id plus one. If the base's `before` equals the newest cached id, nothing was
   missed.
3. **Gaps.** If `before` is past the newest cached id, the entries in between are missing.
   Keep the cache, show the gap, and fill it with `read_entries` (`min` = newest cached + 1,
   `max` = `before`, ascending). Never join the two ranges without filling the gap.
4. **Earlier entries.** Read with `max` = oldest held − 1, descending, page by page.
5. **Instant send.** A client may show a sent input at once under its `request_id` until
   the entry named by its receipt arrives. Unsettled requests may be cached and resent
   unchanged after a reconnect.
6. **Purge.** On `ended` with `denied`, or `error` `denied` or `unknown_session` for a
   Session, delete its cache. Store caches encrypted at rest where the platform allows: a
   cache holds the Session's private data.

The `fill-gap` and `load-earlier` scenarios show rules 3 and 4 against the real harness.

## Ops

Ops are upstream Chord's decoded op tuples with inline paths. Path interning and arity
omission are not used in v0. `operations.json` lists the seven verbs.

Rules a client must implement exactly, with `ops-vectors.json` as the test:

- A path is an array of string keys and non-negative integer indices.
- Reserved segments `__proto__`, `constructor` and `prototype` are refused.
- A string segment into an array is refused. An index past the array end is refused. `s`
  at exactly the array end appends.
- `d` on an array removes the element and shifts the rest. `d` on a missing object key
  does nothing.
- `a` and `t` need an existing string. `t` counts UTF-16 code units.
- `p` clamps its index to the array length. `m` needs a permutation of the same length.
- Refusals fall into three classes: `invalid_op`, `unsafe_path`, `unresolvable_path`.

The vectors were produced by running upstream's own `apply`.

## Inputs

`submit` carries `input.kind` and `input.text`.

| kind | when idle | when working |
| --- | --- | --- |
| `prompt` | starts a run | refused with `busy` |
| `steer` | refused with `idle` | queued; delivered during the current run |
| `follow_up` | starts a run | queued; runs after the current run |

`busy` and `idle` refusals have no effect. The same `request_id` can be reused for the
retry, for example as a `follow_up`.

`write` appends a passive entry with a client-chosen `kind` and optional `data`. It never
starts a run and the model does not see it. Kinds starting with `pi.` are reserved, and
`oqto.ui` is written by the host only (see Generative UI parts).

`abort` stops the current work and withdraws queued inputs. Inputs that were placed or
queued settle `unanswered` with reason `aborted`. The reply arrives when the Session is
idle. Abort is not rollback: entries already written stay.

## Generative UI parts (revision 2.2)

A part is interactive UI an agent shows through a tool or its persona (ADR-0074): an
OpenUI Lang program in the Oqto profile with a required plain-text fallback. It travels
as an ordinary passive entry, so it is cached, paged and never changes once committed.

- **Kind and writer.** The entry kind is `oqto.ui`. Only the host writes it, for a tool or
  the persona. A client `write` of kind `oqto.ui` is refused with `invalid`, so no client
  can show a part that looks like the agent's.
- **Data.** `frames.schema.json` `definitions.ui_part`: `part` is the
  `oqto.openui.part.v1` envelope (`partId`, `profile` id and version, `body`, `fallback`,
  optional `answer` with a JSON Schema and `timeoutSeconds`); `state` is `shown`,
  `answered`, `expired` or `withdrawn`; optional `origin` names the tool `call` or the
  `entry` the part belongs to. The body is at most `max_ui_body_bytes` UTF-8 bytes and the
  fallback at most `max_ui_fallback_chars` characters (`operations.json` `limits`).
- **States.** A later state of a part is a new entry with the same `partId`. Clients show
  the newest state per `partId` and reconcile only by `partId`, never by text or position.
- **Rendering.** Clients render the profile's core components natively and the fallback
  otherwise: unknown components, a profile version they do not know, a body over the size
  or nesting bounds, or a body they cannot parse. Failing closed means showing the
  fallback, never a partial render.
- **Authority.** Rendering a part grants nothing. OpenUI actions never call tools, URLs or
  MCP servers. Answering a part is an `oqto.interactions` `answer` keyed by `partId` and
  validated by the host against `part.answer.schema` before anything reaches the model; it
  is not part of v0. A v0 client shows that the part waits for an answer it cannot give.

The `ui-part` scenario shows a tool writing a part, the host withdrawing it as a second
entry with the same `partId`, and a client's forged part refused.

## Receipts

Every `submit`, `write` and `abort` carries a client-chosen `request_id`, unique within
the Session. It is the idempotency key.

- The same `request_id` with the same content returns the current receipt and has no
  second effect. A client that lost its connection resends the same request.
- The same `request_id` with different content is refused with `request_conflict`.
  "Same content" means equal canonical JSON (keys sorted, no whitespace, UTF-8) of the
  request kind and payload: `[input.kind, input.text]` for a submit, `["write", entry]` for
  a write, `["abort", null]` for an abort.
- A request refused with `busy` or `idle` leaves no record; its `request_id` may be reused.
- Request records, including the content behind `request_conflict`, last as long as the
  Session and survive host restarts.
- `receipt_query` returns the current receipt without doing anything.

Statuses: `queued` → `placed` → `done` or `unanswered`. Steps may be skipped; none go
backwards. `done` and `unanswered` are final.

- `queued`: admitted, waiting in the inbox.
- `placed`: in the transcript (`entry`) and owned by a run.
- `done`: for an input, answered (`answer` is the entry that answers it). For a write,
  appended (`entry`). For an abort, the Session is idle.
- `unanswered`: final without an answer. `reason` is upstream's reason, for example
  `aborted` or `model_error`.

The reply to a request is its receipt at that moment. The same first receipt is pushed to
every other connection attached to the Session, so a second device sees the message from
the moment it is admitted. After that the host pushes a `receipt` frame each time the
status changes, to the requesting connection (attached or not) and to every connection
attached to the Session. A resend of a known request is answered only to its sender. After
a reconnect the client uses `receipt_query` for requests it still holds.

A host tracks every request's status itself, whether or not any client is attached: a
request admitted before every client detached still settles, and `receipt_query` reports it.

On a connection attached to the Session, a receipt that names `entry` or `answer` is sent
only after a `view` frame or `entries` reply on that connection delivered that entry, or
when the entry is at or below the `before` of that connection's attachment (committed, and
readable with `read_entries`). A client can always find the entry.

A receipt is derived from the Session's committed state, never from streamed text.

## Errors and endings

`error` answers one request. `code` is a closed set, listed with meanings in
`operations.json`. `message` is for people and must not be parsed.

`ended` closes one attachment: `host_closed`, `failed` or `denied`. After `failed` the
client must treat everything after the last frame as unknown and check receipts with
`receipt_query` once it can attach again.

## Limits

All limits are in `operations.json`. Byte limits count UTF-8 bytes and are normative; the
schema's `maxLength` values count characters and are only an outer bound. A request over a
limit is refused with `too_large` before anything is done. A host must never send a frame larger than the `max_frame_bytes`
it announced in `attached`.

## Versions

`attach.v` is the contract version. A host that does not speak it answers
`unsupported_version` with the versions it does speak. `attached.upstream` names the Pi
Durable version the host runs, so a client can tell which upstream shapes to expect
outside the stable fields.

A new upstream version needs a new capture. If the captured frames change in the stable
fields or the op rules, that is a new contract version.

## Conformance

The bundle is the test material:

- `fixtures.json`: frames the schema must accept and frames it must refuse.
- `ops-vectors.json`: op batches with their result or refusal class.
- `scenarios/*.json`: real frame sequences captured from Pi Durable 1.5.2 through the
  reference mapping (steps of a second connection carry `"conn"`; a `close` step records
  the host closing a connection), with the hash of the view after every `view` frame and the Session's
  full transcript at the end (`full_transcript`), against which every delivered entry is
  checked.

A **client** conforms when it applies every vector as recorded, replays every scenario to
the recorded hashes and final value, refuses the invalid fixtures, and, given the cached
entries of the first attachment and the pages of `fill-gap` and `load-earlier`, ends up
holding exactly the full transcript.

A **host** conforms when, driven by a scenario's client frames against the same
deterministic model, it produces frames that satisfy the schema, the sequence rules above
and the scenario's `expect` block. Byte-equal frames are not required: upstream ids and
op grouping may differ.

`scripts/lint/session_view_contract.py` checks the bundle against itself with an
independent op applier, including that delivered entries never differ from the
transcript, that receipts follow the entries they name, and that pages stay inside their
requests. It proves the bundle is consistent. It does not prove that any
host or client conforms.

Canonical JSON for hashes: object keys sorted, no whitespace, UTF-8, integers only.

## Not in v0

Listed in `operations.json` `not_in_v0`. The important ones: permission prompts and
decisions, the Session catalog and creation, forks, context edits (planned as an
owner-only operation, since changing what the model sees is an act of authority, not
chat), model selection, partial views, and withdrawing a single queued input.

## Decisions of 2026-11-14

The first revision's open questions, answered by the owner ("clients need to be able to
cache so everything feels as snappy as possible"):

1. **Ops only.** Native clients apply ops; there is no snapshot mode. The vectors are the
   test. A client that cannot apply a frame re-attaches and gets a fresh base.
2. **Whole or nothing.** No redaction in v0 (see Attach and the view).
3. **Retention.** Request records last as long as the Session and survive restarts.
4. **Delivery.** Receipt updates go to every connection attached to the Session.
5. **Ordering.** A receipt follows the delivery of the entries it names.

Also settled from the byoagnt kit's feedback: what "same content" means, that byte limits
are normative, that a request without a usable id closes the connection, and (revision 2.1)
receipts for entries below the window, the first receipt reaching other connections,
receipts to a detached requester, and that a reused frame id closes the connection. The
`two-clients` scenario captures delivery across two connections.

Revision 2.2 (2026-11-15) adds `oqto.ui` entries, the `ui_part` schema definition, the
`ui_parts` section and the two limits in `operations.json`, three invalid fixtures and one
valid one, and the `ui-part` scenario. Nothing in the frames or op rules changed.

## Planned for v1

- **Session catalog.** List the Sessions a client may see (id, created, title or first
  line), create one, mark one active, and tell attached clients when the active Session
  changes. The durable-kit's HTTP routes and `kit.moved` entry are an accepted interim.

## Open questions

1. **Cursor lifetime.** Upstream cursors are scan positions and survive a host restart in
   practice; v0 does not promise it. A client that gets `invalid` for a cursor restarts the
   read from its own bounds.
