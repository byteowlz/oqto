# Local runner lifecycle belongs to systemd via socket-activated template units

> **Amended by ADR-0020 (2026-06-25):** systemd is **one** Placement Supervisor backend, not *the* local supervisor. It remains the supervisor for the systemd-Linux single-node/multi-VM placement (the socket-activation model below stands there), but the core is supervisor-agnostic: rootless podman (primary), Kubernetes, Docker, and `local-process` are peer backends behind the same contract. Read "the local placement's supervisor is systemd" below as scoped to the systemd-Linux backend.

The backend used to detect dead runners and revive them through usermgr/sudoers (`ensure_runner_healthy`), a privileged path that repeatedly produced bugs (visudo fixes, oqto-m94p deploy-restart machinery). We decided the placement's supervisor owns runner lifecycle — the backend never supervises anything it routes to — and for the local Linux placement that supervisor is systemd: system-level template units (`oqto-runner@{user}.service`, `User=%i`) with socket activation (`oqto-runner@{user}.socket` at `/run/oqto/runner-sockets/{user}/oqto-runner.sock`, `SocketGroup=oqto`, mode 0660). Connecting to the socket starts the runner; idle runners may exit and respawn on demand.

## Considered options

1. Status quo: user-level units + `enable-linger` + backend revival — keeps the privileged supervision surface and the `/run/user/{uid}` vs `/run/oqto/runner-sockets/{user}` path divergence.
2. System template units, always-on — no linger, central control, but deploy still needs privileged restart choreography and idle users keep resident runners.
3. System template units + socket activation (chosen) — deletes backend liveness logic, the usermgr revival path, linger provisioning, and deploy restart choreography (stop old services; next connection starts the new binary). The socket unit is the single declared path contract.

## Consequences

- The runner must accept an inherited listener (`LISTEN_FDS`); the shipped-but-nonfunctional `deploy/systemd/oqto-runner.socket` becomes real instead of config-without-implementation.
- `oqtoctl doctor` needs a check for "socket activates but service crash-loops" (surfaces as connection-refused, not a failed unit).
- Runner revival is not session recovery: agent processes that died with a runner are gone; recovery is owned by the persistence-hardening work (oqto-zpvs), not lifecycle.
- macOS/personal dev mode is exempt: the backend supervises a single same-uid child runner (or launchd is used); non-systemd placements are remote dial-in runners whose supervisor is k8s/their own init.
