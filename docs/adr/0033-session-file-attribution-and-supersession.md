# Session file attribution, changesets, and supersession

## Status

Accepted (2026-08-04). Tracked by `oqto-717n`. Builds on the Session, work directory, and Workbench language in `CONTEXT.md`. Consumes the runner-mediated watch contract from [ADR-0032](0032-frontend-state-ownership-and-workbench-stack.md) and serves the persistent Files surface from [ADR-0031](0031-session-centric-workbench-shell.md).

## Context

A work directory holds one shared filesystem, and many Sessions edit it over time. Showing "this file changed" as one flag on the current tree therefore answers only a momentary question. As soon as later Sessions touch the same paths, that flag stops describing what any earlier Session did, and reopening a past Session yields either a stale claim or nothing at all.

The question a person actually asks when returning to a finished Session is not "is this file dirty" but "what did this Session change, and did that survive." Those are different questions against different states, and a single mutable marker cannot answer both.

The durable material already exists and needs no new capture path. `oqto_log_parts` stores `kind = 'tool_call'` rows with `tool_name` and `json_payload`, and `oqto_log_raw_envelopes` retains the full native payload with `payload_sha256`. Edit content, written file content, and their ordering are therefore already recoverable per Session from `oqto-log`.

The filesystem cannot supply the actor. A runner serves one Workspace ([ADR-0019](0019-container-per-workspace-placement.md)), a Workspace holds many work directories, and one work directory hosts many Sessions sharing one filesystem. A watch event therefore proves *that* a path changed and never *who* changed it, and no amount of watcher scoping fixes this: concurrent Sessions in one work directory are indistinguishable at the inode level. Changes also arrive from outside every Session — terminal commands, builds, `git checkout`, App sidecars.

## Decision

### 1. Three projections, not one changed flag

File change information is exposed as three separate projections with different lifetimes and different sources of truth:

| Projection | Question | Lifetime | Source |
|---|---|---|---|
| File Activity | what changed in this work directory just now, and who claimed it? | transient | runner watcher stream (ADR-0032) joined to in-flight tool calls |
| Session Changeset | what did this Session change? | durable | `oqto-log` tool-call parts and raw envelopes |
| Attribution | who last changed what is here now? | derived from current content | changesets replayed over current state |

No component may collapse these into a shared mutable "changed" field. File Activity decorates the live tree and is allowed to disappear on reload; a Session Changeset must not, because it is history.

These are typed, owned projections consumed through the ADR-0032 state model. They are not a publish/subscribe event bus, and they introduce no `window` events, module globals, or DOM-query coordination.

### 1a. The watcher proves change; only a tool call names the actor

The two inputs to File Activity carry different authority and are never conflated:

- the **watcher** is authoritative that a path changed, and carries no actor;
- a **tool call** is authoritative for the actor and the intended path, and covers only harness-mediated writes.

A change is attributed to a Session only when that Session's own tool call names the path. Attribution is never inferred from timing proximity, event order, or "the Session the user is currently looking at" — the same reconstruction-by-order mistake already forbidden for chat history in `docs/frontend/oqto-ui-interface-checklist.md`.

A watched change that no tool call claims is therefore **unattributed**, and is presented as a work-directory change rather than assigned to any Session. Unattributed changes are a first-class result, not a gap to paper over: a build artifact, a terminal edit, or a concurrent Session's write must never be shown as the focused Session's work.

### 2. A Session Changeset is anchored to its own base

Each Session Changeset records the paths that Session changed, with diffs anchored to the content that existed when that Session edited it — never against current file content. A past Session's diff is a historical fact and must render identically no matter how many later Sessions edited the same file.

Base anchoring resolves in this order:

1. where the work directory is a Git repository, anchor to commits, because Git is already the authority for content history;
2. otherwise anchor to the content hashes `oqto-log` already stores for the edit.

### 3. Supersession, not rewritten history

When a later Session changes what an earlier Session wrote, the earlier changeset is never edited or invalidated. Instead the relation between Sessions carries a **supersession** marker: the earlier change remains historically true and is additionally shown as superseded by the identified later Session.

This is what makes a past Session useful rather than merely archived — it answers "did my change survive," which neither a changeset alone nor the current file content can answer.

Supersession ships per path. Region-level supersession is deliberately deferred; see consequences.

### 4. Attribution is Session-scoped and filtered by selected scope

Files are work-directory-scoped while Sessions are the actors, so every attribution record names the Session that produced it, and views filter by the selected scope. A Session must never present another Session's changes as its own, and concurrent Sessions in one work directory must not leak into each other's Files view. This is the cross-Session leak already listed as a release blocker in `docs/frontend/oqto-ui-interface-checklist.md`.

## Rejected alternatives

- **Stacked diffs.** The obvious mental model, and the reason this ADR exists. A stack assumes a linear, ordered, rebased sequence of changes; Sessions run concurrently and interleave, so their true relation is a directed acyclic graph. Presenting them as a stack would either misstate ordering or require rebasing history that already happened. Supersession keeps the honest DAG relation while still answering the question stacked diffs were reached for.
- **One mutable `changed` flag on the file tree.** Cheap, and the current shell's approach. It cannot survive later Sessions and silently degrades into a lie for past Sessions.
- **Diffing a past Session against current content.** Makes historical diffs mutate as unrelated Sessions work, so the same Session shows different history depending on when it is opened.
- **A dedicated file-change capture path.** Unnecessary duplication: `oqto-log` already holds tool-call payloads and raw envelopes, and a second capture path would create a competing authority for the same facts.
- **A Pi event extension as the change source.** It would report only Pi's own writes, making every bridged harness silently blind ([ADR-0006](0006-pi-first-class-harness-others-bridged.md)) while re-deriving actor facts the canonical protocol already persists. Pre-write interception is a Gate and sandbox concern, not an event feed.
- **Attributing watched changes to the focused Session.** The cheap join, and wrong: it manufactures false authorship for terminal edits, build output, and concurrent Sessions.
- **Git as a hard requirement.** Would exclude non-repository work directories, which are ordinary in Oqto.

## Consequences

- Past Sessions stay meaningful: a finished Session can be reopened and still show what it changed and whether that survived.
- `oqto-log` gains a second read model. It remains the durable authority; changesets are projected from it and never written around it.
- Per-path supersession is coarse. It reports that a later Session changed the same file, not that it replaced the same lines, so it can over-report supersession for large files. Region-level supersession requires mapping line ranges across intervening edits and is deferred until per-path attribution proves insufficient in real use.
- File Activity may disappear on reload without data loss, because the durable answer is rebuildable from the Session's changeset.
- Live attribution is only as complete as harness tool calls. Agent writes made through a shell command rather than a file tool arrive unattributed until that Session's changeset is projected, which is accepted: an honest "changed, actor unknown" beats a confident wrong author.
- Non-repository work directories get correct but weaker anchoring than Git-backed ones, and the two must be visibly distinguishable rather than silently mixed.
