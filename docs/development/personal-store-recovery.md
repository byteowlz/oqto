# Personal pairing store recovery

A saved stopped listener is not permission to clear its database or create another
host identity. Inspect `oqto pair --status` and the Runner error first. The private
owner controller now retains startup error causes and allows recovery of exactly
the saved plan; changed scopes still fail closed.

## Legacy filesystem marker after a Mac reboot

Old v1 markers persisted `st_dev`, a mount-instance ID that can change after a
macOS reboot. New v2 markers use the descriptor-bound volume UUID plus inode on
macOS. A mismatched v1 marker requires explicit host-owner approval, not automatic
adoption or database replacement.

1. Confirm the selected Runner is the intended dedicated host user and its saved
   listener is stopped. Privately back up the store and current binaries/config
   coherently; never publish those backups or pairing tickets.
2. Inspect the private `store-owner.json` under the state path reported by status.
   Pin its reviewed SHA256. Do not hand-edit it or copy a different SQLite inode
   beneath its marker. Wrong inode/foreign store/symlink failures require separate
   operator investigation, not this migration.
3. With a qualified binary supporting ADR-0073, run:

   ```sh
   oqto pair --repair-owner-marker REVIEWED_MARKER_SHA256
   oqto pair --recover-listener
   oqto pair --status
   ```

   Use `--runner-home PATH` if deliberately selecting another existing profile.
   These commands never request a pairing ticket. The repair requires a stopped
   saved listener and an unheld authority lock. It verifies the exact old marker,
   recognized application-owned database and same inode; a private backup is
   retained and SQLite is never opened for writes by the migration.
4. Confirm the saved plan and original host identity are unchanged. Only then run
   normal `oqto pair` to explicitly select/confirm grants and issue one new ticket.

For a reviewed rollback while the listener is stopped:

```sh
oqto pair --restore-owner-marker REVIEWED_BACKUP_SHA256
```

This restores the original private legacy marker inode without changing SQLite;
it preserves the v2 marker too. Restoring an obsolete mount ID deliberately
restores the former refusal. Binary/marker rollback must be coherent; never force
it over a running authority. Backups are not permission to reset keys or grants.

## Qualification and observed activation

The regression is reproduced on old source with the exact `personal listener
stopped; inspect runner logs` error. Linux and native macOS gates cover the full
repair/rollback process, wrong identities, unchanged database bytes, preserved
host/TLS identity, same-PID listener recovery after transient bind failure,
unchanged scope and zero-ticket recovery. No native Pi is launched by recovery.

The approved Mac development activation on 2026-10-22 changed only the two managed
pairing binaries and ownership marker; protected config/native settings stayed
unchanged. Database bytes during migration and its inode were preserved. Both
listener status and authenticated mTLS health/capabilities passed afterward;
ungranted enumeration remained denied. This is not real phone enrollment,
terminal-confirmed control or a public release. No ticket was issued by activation.
