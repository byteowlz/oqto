# Runner ownership/socket prevention checklist

Incident: oqto-jft8 / oqto-pvk7 / oqto-5m0v. Source work only; live retirement/deployment is separately coordinated.

Before editing: oqto-log remains durable Chat authority; Pi remains sole native JSONL writer. No DB rows, session IDs, platform/external mapping or reconciliation are changed. Native events, Session binding and reconnect semantics remain unchanged. Changes concern launch identity, one canonical Unix endpoint and ownership of recovery-created processes only.

Required prevention and proof:
- Daemon/client use one pure default resolver; no unsubstituted runtime placeholder. Explicit recovery --socket equals dialed endpoint.
- Wrong-UID launch against another user's HOME fails BEFORE imports, endpoint workers, directory creation or DB access. Valid multi-user administrative HOME remains supported; do not implement process-wide async seteuid.
- Recovery admission is serialized/bounded; readiness failure kills/reaps only the child it launched, never an adopted managed runner.
- Socket exclusivity/cleanup must protect live listeners and inode identity; never unlink a live lock inode or another incarnation's endpoint.
- Doctor/deploy checks actual effective identity and history root/shard/DB/WAL/SHM ownership/access. Snapshot repair re-inventories sidecars after backup; readonly SQLite is not filesystem-side-effect-free. No automatic broad chown, hidden shard omission or chmod777.
- Disposable HOME/XDG fixtures, regression tests, scoped format/lint/test gates and zero new warnings. Actual authenticated session-list and reconnect/switch/reload are separate live acceptance, not inferred from backend health200.

Live evidence: old root PID2671205 uses HOME=/home/wismut and XDG_RUNTIME_DIR=/run/user/1000, runs deleted September4 release, and holds user-history SQLite/WAL/SHM files. No direct child agents found. Legacy shutdown source unconditionally removes the configured socket, which now belongs to the managed user runner. Do NOT blindly terminate it and break the newer listener. Identify connected peers and coordinate a protected/quiet retirement. Child inventory MUST cover all `/proc/PID/task/*/children` or system-wide PPIDs; main-thread `task/PID/children` alone is insufficient for Tokio daemons. No attribution of specific historical files is claimed without creator evidence.

Approved retirement completed 2026-10-05T14:24:48Z after a disposable hardlink/restore listener proof. Only pidfd-bound SIGTERM was sent to the verified legacy PID. A private alias preserved the current managed socket through the legacy unlink; restored inode1079820, mode0770 and ownership unchanged. Managed PID2792033 unchanged; Ping/Pong+capabilities pass;204 shards/0 ownership/access failures. Alias removed after legacy exit. Protected audit `/root/oqto-recovery-backups/20261005T142448Z-legacy-runner-retirement.json`. This mitigates the live stale-root writer. The source guards described below are implemented; deployment/strict-doctor integration remains separate and is not accepted by this mitigation.

## Implemented source guards and evidence

- `runtime_guard.rs`: actual effective UID versus private HOME/XDG/configured Pi/memory/history roots, before config probes/imports/endpoint workers; readonly metadata audit of shard/DB/WAL/SHM ownership and type. No SQLite connection, sidecar creation or automatic permission repair. Administrative runners can use administrative-owned private state; this is filesystem admission, not a substitute for Account/Workspace/Session authorization.
- One daemon/client default endpoint; reject relative and unresolved-brace socket paths. Native default CLI proof has no literal `{runtime_dir}` directory.
- `socket_lease.rs`: persistent no-follow lifetime lock inode, fail closed on active/non-socket endpoints; remove only a proven refused stale socket. Shutdown/error cleanup compares bound inode identity. Existing runtime-directory modes are preserved; inherited systemd listener remains activation-manager owned.
- `recovery_child.rs` + client: serialized/throttled admission, explicit `--socket`,15-second elapsed readiness deadline (not just fast failed attempts). Failure/cancellation kills only the unadopted owned process group, including config smoke helpers, and reaps its direct child. Adoption releases kill ownership, so a backend/reaper exit cannot terminate a durable runner. An orphan helper zombie can await init reaping; no running helper is accepted.
- Native private-HOME/XDG tests use synthetic Pi, stripped credentials, no provider/model calls.12 concurrent actual SDK callers spawn exactly one native runner; it survives SDK process exit. Actual SDK failed-readiness proof stops/reaps a live child. Native duplicate/adoption/inode cleanup, user-namespace real UID rejection before Pi/config workers, metadata no-sidecar and inherited-listener tests pass.
- Gates:134 Rust tests (including explicitly exercised namespace proof and subprocess SDK probes),1 Python gate-root/signature regression; `just fmt`, `just lint-runner-runtime-guards` (all-target Clippy `-D warnings`), crate-boundary/history-authority and diff checks pass. New production guard modules have0 ast-grep findings.
- `oqto-xcf3`: fixed change-check repository-root resolution and content/profile fingerprinting. Disposable regression fails before the fix for bad root and repeated edits, then passes; unchanged quick work skips, full profile cannot borrow a quick stamp.

## Scoped policy debt and remaining acceptance

The broader branch ast-grep gate is NOT green:47 inherited findings across147 previously committed files. Scoped acceptance rationale for this patch: retain unchanged `add_memory`/`update_memory` argument-count suppressions and `get_stdout_receiver` wire-error suppression (3 findings in touched files), rather than bundle an unrelated public memory API/wire-error redesign into incident prevention. No new suppression/finding or compiler/Clippy warning is excused. The other44 findings belong to untouched inherited work. This is bounded source-patch debt, NOT release/main/deployment acceptance; broad gate cleanup stays an integration blocker.

Runtime-root checks cannot police arbitrary privileged programs or old binaries. A failed strict owner check is diagnostic, not permission to chown automatically. Install-contract doctor integration, package deployment, real multi-user/container namespace qualification and authenticated Web/mobile reload/reconnect remain required before claiming permanent live prevention.
