# ADR: oqto-log authoritative history store

Date: 2026-04-10  
Status: Accepted (phase 1 foundation)

## Context

The current Pi-RPC -> runner -> hstry/frontend history path can produce disappearing, duplicated, and misordered messages due to split authority and merge heuristics.

## Decision

Adopt `oqto-log` as the single durable authority for chat history projections.

### Core invariants

1. **Single durable authority**
   - Timeline/tree reads are projected from oqto-log only.
   - No secondary authority merge in frontend or API paths.

2. **Deterministic canonical IDs**
   - Persist canonical `turn_id` and `message_id` derived deterministically.
   - Preserve source IDs (Pi/JSONL) as hints/lineage only.

3. **Append-only turn DAG**
   - Turns are immutable records linked by `parent_turn_id`.
   - Branching is explicit via `branch_id`.

4. **Version monotonicity**
   - `turn_version` is monotonically increasing per session.

5. **Idempotent import contract**
   - Source ingest de-duplicates by `(source_kind, source_session_id, source_entry_id)` when present.
   - Import progress is tracked in checkpoints.

6. **Storage placement**
   - `oqto-log` lives in Linux user home.
   - **One database per workspace**:
     - `~/.local/share/oqto/oqto-log/<workspace_hash>/oqto-log.sqlite`
   - Shared workspaces use the shared workspace Linux user's home.

## Compatibility / migration policy

- Bootstrap migration from Pi JSONL is mandatory during install/update/deploy.
- Migration must be replay-safe and resumable via checkpoints.
- Cutover to oqto-log reads is blocked if migration validation fails.

## Consequences

- Significant reduction in history ambiguity and merge race surfaces.
- Clear operational contract for deploy-time backfill and validation.
- Enables first-class tree/fork/compaction projection from one source.
