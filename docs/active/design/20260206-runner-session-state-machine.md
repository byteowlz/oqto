# Runner Session State Machine

This document defines the session lifecycle state machine enforced by the
runner for Pi sessions. The runner owns process lifecycle and is the source
of truth for state transitions. The frontend mirrors state for UX only.

## States

- `starting`: Session process is booting.
- `idle`: Session is ready and waiting for input.
- `streaming`: Session is actively emitting a response.
- `compacting`: Session is compacting its context.
- `aborting`: Session is cancelling a running turn.
- `stopping`: Session is shutting down.

## Command Guards

- `prompt`: allowed in `starting` or `idle`.
- `follow_up`: allowed in `starting`, `idle`, or `streaming`.
- `steer`: allowed in `starting`, `idle`, or `streaming`.
- `compact`: allowed only in `idle`.
- `abort`: allowed in any state; transitions to `aborting` when active.
- `new_session` / `switch_session`: allowed only in `idle`.
- `get_state`, `get_messages`, `get_stats`, `get_commands`: always allowed.

## State Transitions

- `starting` -> `idle` after first successful Pi event.
- `idle` -> `streaming` when a prompt is dispatched.
- `streaming` -> `idle` on `agent_end` or `stream.done`.
- `streaming` -> `aborting` when `abort` is dispatched.
- `aborting` -> `idle` on `agent_end`.
- `idle` -> `compacting` when compaction starts.
- `compacting` -> `idle` when compaction ends.
- Any -> `stopping` on process exit.

## Notes

- `steer` in `idle` is routed as `prompt`.
- `follow_up` in `idle` is routed as `prompt`.
- State changes are driven by runner-observed events; the frontend should not
  infer state transitions based on user intent alone.
