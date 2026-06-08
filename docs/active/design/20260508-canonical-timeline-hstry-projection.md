# Canonical timeline and hstry projection architecture

Status: accepted for implementation
Issue: oqto-awh9

## Decision

Oqto's long-term history authority is the canonical timeline stored in oqto-log. hstry remains a compatibility, interoperability, and search projection, but it is not the source of truth for Oqto session timelines.

This follows the Bitter Lesson for agent history: retain rich, lossless, stable data and derive narrow views from it. Do not make the durable store match today's chat UI or one harness's DTOs.

## Authority model

- oqto-log/timeline is authoritative for runner-mode Oqto sessions.
- hstry is a derived projection for cross-tool search, legacy clients, and migration interoperability.
- Pi JSONL remains harness-owned raw metadata and can be imported/replayed, but Oqto must not create or mutate Pi JSONL.
- Frontend chat messages are a view projected from timeline turns and parts.

## Timeline v1 shape

The Rust DTOs live in `backend/crates/oqto-protocol/src/timeline.rs`.

Core objects:

- `TimelineDocument`: versioned container for one session.
- `TimelineSession`: preserves both `platform_id` and `external_id`.
- `TimelineBranch`: first-class branch node for forks.
- `TimelineTurn`: first-class turn node with parent linkage and monotonic `turn_version`.
- `TimelinePart`: atomic content. Tool calls and tool results are distinct lifecycle entries.
- `RawNativeEvent`: byte-preserving native event retention for deterministic re-projection.
- `AgentContextSnapshot`: captures model/provider, included turns, files, prompt hash, and usage at generation time.

## Projection rules

Native harness events are appended or imported into timeline using deterministic projectors:

1. Preserve native event payloads in `RawNativeEvent` before lossy normalization.
2. Resolve stable identities: `platform_id` for Oqto control, `external_id` for harness/import identity.
3. Assign turns to a branch and parent turn; increment `turn_version` monotonically per session.
4. Split tool lifecycle data into `ToolCall` and `ToolResult` parts linked by `tool_call_id`.
5. Attach `AgentContextSnapshot` when the agent turn begins or at the nearest available context boundary.
6. Commit turns append-only. Repairs may replace a projection only when the raw source is strictly richer and the operation is idempotent.

## hstry projection

The oqto-log to hstry projection is intentionally lossy and rebuildable:

- text/thinking/file/tool parts are converted to hstry `Part` values for search and legacy rendering;
- branch and context metadata are summarized in metadata fields where useful;
- raw native event payloads are not required in hstry because they remain in oqto-log;
- projection records should carry source hashes/checkpoints so they can be re-run safely.

If hstry and oqto-log disagree for runner-mode Oqto reads, oqto-log wins. hstry may be used as a temporary migration fallback only when oqto-log lacks an imported timeline and the fallback is explicitly documented at the call site.

## Invariants

- No durable row uses `pending-*` or `tmp:*` IDs.
- Every committed turn belongs to exactly one session and branch.
- `(session_id, turn_version)` is unique and stable.
- Tool results must reference a tool call by `tool_call_id` when the source provides one.
- Raw native events must remain sufficient to re-run the projector and reproduce the same timeline graph.
- hstry projections must be rebuildable from oqto-log without consulting frontend state.

## Migration and shim removal plan

The migration is intentionally incremental: every phase has an observable gate and a rollback point. Until the final cleanup phase, new timeline code must be additive and must not remove the ability to render existing sessions.

### Phase 0: foundations and invariants

Status: complete for v1 DTO/projection foundations.

Scope:

- Introduce neutral timeline DTOs and projection DTOs in `oqto-protocol`.
- Extend oqto-log schema for timeline metadata, raw native event retention, and context snapshots.
- Move read-only oqto-log projection logic into `oqto-history` so runner/server crates only adapt to it.
- Add golden and invariant tests for Pi import, event assembly, failed turns, tool lifecycle, raw refs, deep links, and temporary ID rejection.

Gate:

- `cargo test -p oqto-history` passes.
- `cargo clippy -p oqto-history -- -D warnings` passes.
- No production code can persist `pending-*` or `tmp:*` IDs to timeline rows.

Rollback:

- Keep current hstry/frontend read paths active. Timeline projection can be disabled because no user-facing read path depends exclusively on it yet.

### Phase 1: dual-project writes at durable boundaries

Scope:

- On runner `AgentEnd`/terminal turn boundaries, write or update oqto-log timeline turns append-only from canonical event snapshots.
- Keep existing hstry persistence active as a compatibility projection during this phase.
- Store enough source metadata to make duplicate `AgentEnd` replays idempotent: session IDs, turn IDs, raw source refs, turn version, and projection hash.
- Emit structured diagnostics for projection status: `timeline.projected`, `timeline.skipped_duplicate`, `timeline.invariant_failed`, and `hstry.projected`.

Gate:

- For new runner sessions, oqto-log turn count converges with current hstry-visible message groups after agent idle.
- Duplicate terminal events do not create duplicate turns.
- Existing reconnect, retry, abort, compaction, and tool-result flows pass regression tests.

Rollback:

- Stop invoking timeline writes; hstry remains the read/write path for visible history. Any partial oqto-log rows are ignored until reindex/backfill.

### Phase 2: backfill and validation for existing stores

Scope:

- Import Pi JSONL and existing hstry metadata into oqto-log without mutating Pi JSONL.
- Backfill both `platform_id` and `external_id` for sessions; never replace one with the other.
- Validate raw refs, branch parent integrity, stable `(session_id, turn_version)`, and tool call/result links.
- Produce a per-workspace migration report with counts for imported sessions, skipped sessions, invariant failures, and projection hash mismatches.

Gate:

- `oqto runner migrate-oqto-log --mode validate` or equivalent validation reports zero blocking invariant failures for the workspace before read cutover.
- Re-running bootstrap/backfill is idempotent.
- Search results from `oqto search timeline ... --json` include deep links for imported sessions.

Rollback:

- Keep hstry as the user-visible history source for workspaces whose validation fails. Failed workspaces can be repaired and re-run without deleting hstry.

### Phase 3: switch backend reads to timeline-first

Scope:

- Change session message/tree APIs to read from oqto-log timeline projections first for runner-mode sessions.
- Allow hstry fallback only when oqto-log has no imported timeline for a session and the call site logs an explicit `timeline.fallback.hstry` diagnostic.
- Add API support for active branch reads, branch listing, and deep-link lookup by timeline turn/message/part IDs.
- Keep existing flattened chat response shape as a compatibility projection while adding tree-aware response metadata.

Gate:

- Reloading a runner session after idle produces the same visible chat content from oqto-log as from hstry projection.
- Branch metadata (`parent_id`, `branch_id`, active branch head) survives API round trips.
- hstry outage does not break timeline reads for migrated runner sessions.

Rollback:

- Toggle backend read priority back to hstry-first. Because writes remain dual-projected, no timeline data needs to be discarded.

### Phase 4: switch frontend rendering to timeline/tree views

Scope:

- Promote the feature-flagged timeline tree preview into the default renderer for migrated sessions.
- Continue offering a flattened active-branch chat view as a projection of the tree, not a separate data model.
- Preserve existing chat UX while exposing branch/deep-link affordances incrementally.
- Remove assumptions that message order alone defines conversation structure; use timeline parent/branch metadata instead.

Gate:

- The default UI renders existing linear sessions identically to the legacy chat list.
- Forked/retried sessions show stable branch structure after reload.
- Deep links from search/projection open the correct session and turn.

Rollback:

- Disable the frontend rollout flag and render the flattened compatibility projection. Backend timeline reads can remain enabled if their flattened output is stable.

### Phase 5: make hstry projection-only

Scope:

- Stop treating hstry as authoritative for runner-mode timeline reads or search.
- Keep hstry writes only as an optional, rebuildable export for legacy tools and interop.
- Prefer native oqto-log search for Oqto and agent-mediated search.
- Make projection checkpoints explicit so hstry can be rebuilt from oqto-log after schema changes.

Gate:

- `oqto search timeline` covers the Oqto search paths previously dependent on hstry for runner sessions.
- hstry can be deleted/rebuilt in a test workspace without losing Oqto-visible timeline history.
- Legacy clients that still read hstry are documented as consuming a derived export.

Rollback:

- Re-enable hstry projection jobs or legacy read fallback for affected clients. Do not promote hstry back to canonical authority.

### Phase 6: remove compatibility shims

Scope:

- Delete runner/server shim modules that only re-export timeline projectors once all call sites import `oqto-history` directly.
- Remove `ChatMessageProto`-specific history projection paths from durable runner-mode flows.
- Delete hstry-first history reads for runner sessions.
- Remove frontend timeline preview flag once the tree renderer is default and covered by tests.
- Update older design docs that describe hstry or Pi JSONL as authoritative for runner sessions.

Gate:

- No production code path writes full-window hstry snapshots as the source of truth for runner sessions.
- No production code path reads runner-session history from hstry unless explicitly labeled export/legacy.
- End-to-end deploy validation covers new session, reload, retry/error, fork, search deep link, and imported Pi JSONL scenarios.

Rollback:

- This phase is not rollback-friendly. Only start it after at least one release has shipped with timeline-first reads and hstry projection-only behavior enabled.

## Metrics and diagnostics

Track these during rollout:

- `timeline_projection_success_total` and `timeline_projection_failure_total` by harness and failure reason.
- duplicate terminal replay count and dedupe count.
- hstry fallback read count; this should trend to zero before shim removal.
- timeline validation failures by invariant.
- hstry projection lag/checkpoint age when the export job is enabled.
- search deep-link resolution failures.
- frontend branch-render fallback count.

Operational rule: any increase in fallback reads, invariant failures, or unresolved deep links blocks progression to the next phase.
