# 0025: Chat reads are read-only; Pi JSONL syncs via a background ingest task with stat cursors

Date: 2026-07-18
Status: accepted

## Context

oqto-log is the durable read authority for chat history, but Pi owns the JSONL
session files and can be driven outside Oqto (bare `pi` continuing a session).
To catch such out-of-band changes, the runner previously spawned a
"repair-if-richer" task on every authoritative chat read: parse the full JSONL,
compare message counts, and replace the oqto-log session if the JSONL looked
richer.

This made reads write. Consequences observed in production:

- Chat opens for split/empty-keeper sessions re-parsed a 48MB JSONL and
  attempted a 16k-message replace on every open, failing with
  `database is locked` (concurrent repairs + long FTS-rebuild transactions)
  and retrying forever.
- Runner RSS ballooned to 1.45GB; request latency rose to ~6s while repair
  storms ran.
- The comparison itself (projected count vs JSONL entry count) reconciled
  across different granularities, so it could trigger indefinitely.

## Decision

1. **Chat reads never write.** The authoritative messages path serves oqto-log
   only and, at most, sends a fire-and-forget nudge to the ingest task.
2. **One serialized ingest task per runner** (`daemon/jsonl_ingest.rs`)
   reconciles Pi JSONL into oqto-log:
   - An **ingest cursor** per session file (size + mtime, stored in the
     oqto-log index database) turns change detection into one `stat()`.
   - A periodic **sweep** (5 min) walks all session files as a safety net;
     chat opens **nudge** a specific session for prompt pickup.
   - Files modified within a **quiescence window** (30s) are skipped: live
     sessions are fed to oqto-log by the event pipeline, and half-written
     files must not be parsed.
   - When the JSONL is strictly richer than oqto-log, the session is
     converged with `replace_session_with_pi_jsonl_records` (keyed by source
     entry ids, idempotent). No append-then-maybe-replace: append of
     re-ingested content duplicates messages.
3. **Explicit repair remains explicit**: the `RepairWorkspaceChatHistory`
   runner op and `/api/chat-history/backfill` stay as user-invoked bulk
   operations.

## Consequences

- Chat opens are O(1) reads (session index, ADR-cross-ref: commit `2ad0b7c6`)
  with no write amplification; lock storms from concurrent per-open repairs
  are structurally impossible (single writer).
- Bare-pi continuations appear in Oqto after file quiescence plus nudge/sweep
  latency — typically the next chat open or within one sweep interval — and
  no longer depend on someone opening the chat to trigger sync.
- Losing the cursor table only costs one re-scan; cursors live beside the
  best-effort session index in `index.sqlite`.
- Count-based richer-detection is still a heuristic pending stable
  source-entry-id diffing; it is now bounded to at most one replace attempt
  per file change instead of one per read.
