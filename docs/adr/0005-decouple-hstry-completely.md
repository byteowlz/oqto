# hstry is fully decoupled from oqto

oqto-log is the sole history authority for oqto sessions (20260410 ADR); runner writes already go only to oqto-log. We decided to remove hstry from oqto entirely rather than keep it as a derived read model: the planned oqto-log -> hstry projection pipeline (oqto-awh9.12) is cancelled, the 32K-LOC `history/hstry/` gRPC read surface gets deleted, `hstry.service` leaves setup/deploy, and the canonical WS channel is renamed from `"hstry"` to `"history"`. hstry survives as an optional standalone product — an aggregate store for users who want to search sessions across other agents (Claude Code, codex, etc.) — but oqto neither writes to it, reads from it, nor provisions it.

This supersedes the projection-only endgame in `docs/active/design/20260508-canonical-timeline-hstry-projection.md` (phases 5-6); the cutover intent of oqto-2hyk and oqto-5stn stands.

## Cutover conditions (before deletion)

1. oqto-log search shipped (FTS, runner-mediated) — the last capability hstry uniquely provides (oqto-2hyk).
2. History read performance from oqto-log acceptable (oqto-hsde resolved).
3. Regression matrix (20260410) and migration validator green; deploy-time importer converges remaining data (oqto-t3jv).
4. Rollback path: previous release; old hstry DB files remain on disk as cold backup, served by nothing.

## Consequences

- oqto-4g08 (hstry adapter provisioning drift) dissolves instead of being fixed.
- New oqto features must never target hstry; any future "search my non-oqto agent sessions inside oqto" feature would integrate hstry as an external optional tool, like any other harness-adjacent tool.
