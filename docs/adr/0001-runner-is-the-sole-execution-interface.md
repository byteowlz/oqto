# Runner is the sole execution interface

The backend historically branched on runtime modes (`local`, `runner`, `container`) in 13+ places in `SessionService`, owning container orchestration and direct process spawn alongside runner routing. We decided the runner is the only way the platform executes agent work: every placement — local host, remote machine, container, pod — runs a runner inside it, and the backend only ever speaks the runner protocol. "Container mode" and "local mode" cease to exist as backend concepts; isolation (bwrap, container, microVM) becomes runner-side per-session policy, which aligns with the isolation-tiers direction (20260609).

## Consequences

- `RuntimeMode` branches in the backend are dead code to be removed; `SessionService` sheds container runtime orchestration.
- Attachment is hybrid: local runners are reached via the declared socket-path convention; remote runners dial in and register with the backend (identity, capabilities, transport). The backend is a registry/router, never a supervisor of remote placements.
- Personal (single-user) deployments still run a runner; the backend may supervise that one child process as a convenience, but it attaches through the same protocol.
