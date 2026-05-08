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

## Migration path

1. Introduce neutral timeline DTOs and schema governance.
2. Extend oqto-log schema for context snapshots and raw native events.
3. Move remaining ChatMessageProto-specific projection responsibilities behind timeline/projection boundaries.
4. Build a checkpointed oqto-log to hstry projection job.
5. Migrate frontend tree/chat rendering to timeline views.
6. Remove legacy fallback paths once invariant and golden tests cover reconnect, retry, fork, and import scenarios.
