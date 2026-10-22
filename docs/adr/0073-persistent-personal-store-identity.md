# Persistent personal-store identity and explicit listener recovery

## Status

Implemented and under native qualification, `oqto-8x3p.10.7.8`. Extends ADR-0062
and ADR-0069's preserve-first local owner workflow. Not phone/control acceptance.

## Context

A Mac reboot changed `st_dev` while the personal SQLite inode remained unchanged.
The v1 private owner marker stored a mount-instance device number as if it were a
persistent volume identity. Reopening therefore refused the existing store.
The listener startup error was hidden by its readiness channel, and a saved plan
with a stopped task permanently refused subsequent owner Enable requests.

## Decision

New stores use a v2 marker: purpose, persistent filesystem identity and inode.
On macOS, the filesystem identity is the volume UUID returned by descriptor-bound
`fgetattrlist(ATTR_VOL_INFO | ATTR_VOL_UUID)`, not the volatile `st_dev`.
Unsupported/unavailable/zero volume identity fails closed. Other Unix platforms
retain the existing device-number/inode fence; this decision does not claim their
filesystem remount behavior has been qualified. Same-process inode leases keep
using `st_dev`/inode, where mount-instance identity is appropriate.

A valid v1 marker whose current device/inode still matches remains readable.
A mismatched v1 device is NEVER silently adopted. The host owner must explicitly
run `oqto pair --repair-owner-marker <expected-marker-SHA256>` for the selected
Runner's saved, stopped listener. The library owns this operation:

- Validate existing private directories, ancestors, owner/modes and no-follow
  regular files; acquire the authority singleton lock.
- Pin exact old marker bytes by SHA256; require known v1 purpose, matching inode
  and recognized initialized application-owned SQLite header. Unknown marker,
  changed inode, foreign database and active authority are rejected.
- Preserve a private hard-linked backup without overwriting unknown entries.
  Resume a pre-publication interruption only if the backup is the exact original
  marker inode. Sync backup, stage and directory publication.
- Publish the v2 marker atomically after rechecking current marker and database
  identity. This explicit owner migration binds the currently approved volume;
  no inference about an unrecorded pre-reboot volume UUID is made.
- Never open SQLite for writes, migrate its schema, rotate keys, alter grants,
  clear tickets or generate a pairing token. The database bytes/inode are retained.

`oqto pair --recover-listener` requests the exact persisted plan and emits status,
not a ticket. A normal newly confirmed pairing operation may also recover that
same plan before its separate, once-only issuance request. A stopped task is fully
retired before reopening the authority; the controller serializes recovery and
rejects changed Workspaces/native source/origin/address before retirement. Stored
startup failures retain the child error chain in owner diagnostics/logs.

## Rollback and verification

A private old marker backup remains beside the store. The explicit
`oqto pair --restore-owner-marker <backup-SHA256>` operation validates the current
v2 identity, singleton lock and exact private legacy backup, preserves the v2
marker too, and atomically restores the original legacy marker inode without
opening SQLite for writes. It is only available for the saved stopped listener.
Binary/marker rollback must
be coherent and occur only while the authority is stopped; restoring the exact
old marker on a changed mount intentionally restores the previous refusal, not
false success. Do not copy a backup database inode into place under either marker.
Use the supported host-owner path, not manual JSON/SQLite edits.

Regression tests cover persistent identity versus changed mount ID, foreign
identity/inode, digest-pinned migration with byte-identical database/host/TLS
identity, active-authority exclusion and explicit non-pairing CLI modes. A real
fixture process starts with a saved listener blocked by an occupied port, then
recovers on the same PID after release; old source produces the user's exact
`personal listener stopped` error. Another real process fixture repairs legacy
metadata and recovers without ticket issuance. Native Mac qualification exercises
the actual volume UUID syscall and installed binaries before live activation.
