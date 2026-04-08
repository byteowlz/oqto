# Pi Session Identity Contract

Status: draft (tracked by `oqto-c4x3.1`)

## Problem

Pi session identity is currently ambiguous across multiple identifiers:

- Pi native session ID (JSONL/runtime)
- Oqto runner session ID
- hstry conversation UUID
- readable IDs / titles

This ambiguity causes duplicate conversations, fork lineage drift, and stale/partial loads when sessions are started or continued outside Oqto.

## Goals

1. Deterministic identity resolution for every Pi session operation.
2. Zero duplication of Pi conversations in hstry.
3. Stable fork/tree ancestry across Pi and Oqto entry points.
4. No heuristic title/readable-id merge logic in primary paths.

## Canonical Identity Model

For `source_id = "pi"`, every conversation has these identities:

- `pi_session_id` (authoritative)
  - Source: Pi JSONL session header / Pi runtime.
  - Immutable for the lifetime of the Pi session.
- `conversation_id` (hstry internal UUID)
  - Storage primary key only.
- `external_id`
  - Must equal `pi_session_id` for Pi conversations.
- `runner_session_id` (ephemeral)
  - Runtime process handle bound to the active Oqto session process.
- `readable_id` (UX-only)
  - Human-friendly slug; never a primary key.

### Fork lineage fields (Pi conversations)

- `root_pi_session_id`
- `parent_pi_session_id` (nullable)
- `fork_origin_message_id` (nullable; if available)

Lineage must be keyed by Pi IDs, not readable IDs.

## Invariants

1. For `source_id = "pi"`, `external_id == pi_session_id`.
2. A Pi session maps to exactly one hstry conversation (`1:1` by `external_id`).
3. `readable_id` and title never determine authoritative identity.
4. `runner_session_id` never determines durable identity.
5. All session attach/open/continue operations resolve to `pi_session_id` first.
6. Fork ancestry references Pi IDs only.

## Resolution Algorithm

`resolve_pi_identity(input) -> PiIdentityResolution`

`input` may contain one or more of:

- `pi_session_id`
- `runner_session_id`
- `conversation_id`
- `readable_id`

Resolution order:

1. If `pi_session_id` provided, lookup hstry by `source_id="pi" && external_id=pi_session_id`.
2. Else if `runner_session_id` provided and active, map active runner session -> `pi_session_id`.
3. Else if `conversation_id` provided, load conversation and verify `source_id="pi"` then read `external_id` as `pi_session_id`.
4. Else if `readable_id` provided, resolve to conversation ID (must be unique in scope), then step (3).
5. If unresolved, attempt JSONL discovery by native Pi session files (deterministic scan), then bind/import.

Return:

```ts
{
  pi_session_id: string;
  conversation_id?: string;
  runner_session_id?: string;
  readable_id?: string;
  source: "hstry" | "runner" | "jsonl";
}
```

## API/Code Rules

- Workspace chat/history APIs must accept/resolve to `pi_session_id` internally.
- No direct business logic keyed by `readable_id`.
- Any endpoint accepting `readable_id` must resolve once at boundary and then use `pi_session_id`.
- Canonical message fetch paths for Pi must use the resolved identity from this contract.

## Migration Plan (existing data)

### Phase 1: Audit

1. Enumerate `source_id="pi"` conversations.
2. Build groups by `external_id`, `platform_id`, `readable_id`.
3. Detect duplicates where multiple `conversation_id` map to same `pi_session_id` candidate.

### Phase 2: Normalize

1. Set `external_id = pi_session_id` for all valid Pi conversations.
2. Backfill missing `pi_session_id` from JSONL/session metadata where possible.
3. Mark unresolved records for manual repair queue.

### Phase 3: De-duplicate

For each duplicate group of same `pi_session_id`:

1. Choose canonical survivor conversation (highest message completeness + newest metadata).
2. Reproject authoritative JSONL into survivor.
3. Tombstone/soft-delete duplicate conversations.
4. Repoint lineage references to survivor.

### Phase 4: Enforce

1. Add uniqueness enforcement for Pi identity (`source_id="pi"` + `external_id`).
2. Route all Pi history/session resolve paths through `resolve_pi_identity`.
3. Add regression tests.

## Test Matrix (must pass)

1. Start session in Pi CLI -> open in Oqto -> same `pi_session_id` mapping.
2. Continue outside Oqto -> reopen -> no new conversation created.
3. Create fork in Pi -> Oqto tree shows correct `parent_pi_session_id`.
4. Create fork in Oqto -> continue in Pi -> tree remains consistent.
5. Rename/readable-id change -> identity remains stable.
6. Reindex from JSONL after corruption -> same identity and lineage restored.

## Non-Goals

- Replacing canonical message model.
- Making `conversation_id` user-facing.
- Using title heuristics for identity reconciliation.
