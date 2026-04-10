# oqto-log regression and chaos matrix

Date: 2026-04-10

This matrix defines correctness checks for message integrity after oqto-log cutover.

## Core invariants

- No message disappearance after reconnect/session switch.
- No duplicates after retry/reconnect/bootstrap import reruns.
- Timeline order stable by `turn_version`.
- Tree links stable via `parent_turn_id` and `branch_id`.

## Scenario matrix

1. Reconnect mid-stream
   - Start prompt, disconnect client, reconnect before AgentEnd.
   - Expected: stream overlay may drop, durable timeline converges without duplicate turns.

2. Retry cycle with duplicate AgentEnd placeholders
   - Trigger retry with recoverable error.
   - Expected: only committed snapshot turns in oqto-log, no duplicate user/assistant pairs.

3. Session switch race
   - Switch sessions rapidly while prompts complete.
   - Expected: per-session projector reads never cross-contaminate.

4. Fork/tree navigation
   - Create child/fork branch and navigate ancestor/child.
   - Expected: tree projection returns correct parent graph and branch heads.

5. Bootstrap import idempotency
   - Run `oqto runner migrate-oqto-log --mode bootstrap` twice.
   - Expected: second run writes no duplicate turns/messages; checkpoints advance only when needed.

6. Validation gate behavior
   - Corrupt one workspace db and run `--mode validate`.
   - Expected: non-zero mismatch count; deploy health gate fails and rolls back.

7. Restart recovery
   - Restart runner during active usage.
   - Expected: post-restart reads from oqto-log projector return stable timeline.

8. External continuation (Pi JSONL updated outside oqto)
   - Append messages via external Pi run; execute bootstrap again.
   - Expected: new turns imported, old turns unchanged.

## Commands

```bash
# Bootstrap and validate
oqto runner migrate-oqto-log --mode bootstrap
oqto runner migrate-oqto-log --mode validate

# Operational diagnostics
oqto runner migrate-oqto-log --mode diagnostics
oqto runner migrate-oqto-log --mode reindex
```
