# Shared Session file saves and Terminal adapters

Implemented source; deployment and physical acceptance are separate.

## Baseline-checked writes

The Server files channel has an explicit `write_checked` command with
workspace-relative path, base64 content and expected SHA-256. A supporting
user-plane advertises `checked_write_supported` on read results. Direct planes
without an implementation return unsupported; there is no legacy unconditional
write fallback.

RunnerUserPlane calls the new typed `WriteFileChecked` RPC using
`request_once`: no recovery, autostart, reconnect queue or retry. The ordinary
Files transport admission includes the new variant; remote Files peers still
resolve against their opened ScopedFiles descriptors. Caller root/path strings
never enlarge those grants.

Both personal FileWrite and this RPC reuse `checked_file::replace`. It checks
the loaded digest before preparation and immediately before atomic replacement,
syncs the temporary file and directory, and returns the resulting digest.
No-clobber creation remains available only through the existing personal API.
The mobile editor caps text saves at 256 KiB; existing personal wire bounds remain
unchanged. JSON is not parsed, normalized or rewritten.

Final-component reads use safe rustix openat with NOFOLLOW beneath the already
opened parent. The negative symlink regression first failed when relying on
cap-std custom O_NOFOLLOW (cap-std resolves links earlier); the fixed test passes.
Parent traversal/outside grants and stale digests are refused. These checks do not
serialize unrelated native writers across the final baseline check and rename.

## Session Terminal

`SessionTerminalOpen { session_id, rows, cols }` is admitted only with the
existing Session observation and full-user Terminal grants. The host resolves
the exact authorized Session/cwd, never a phone-supplied workspace string, and
opens or reattaches that device's live Session PTY. No Pi attachment/control lease
is acquired; no Pi is started or reloaded. Terminal is full-user execution, not
cwd containment.

`session_terminal_supported` is a separate capability, default false for older
Hosts. Raw PTY support must not imply this Session-cwd operation. Existing
Terminal read/write/resize/close ownership and revocation checks remain in force.
Personal client Detach only drops its live attachment; the remote shell may live.
Output loss and unconfirmed input are surfaced, never silently replayed.

## Verification and limits

- Runner units cover stale digests, exact authored bytes, final symlinks and
  descriptor-scoped checked writes.
- Signed process proof covers independent grants, authoritative Session cwd,
  same-PTY reuse and zero Pi execution. Another actual daemon RPC proof covers
  checked replacement, digest receipt, stale baseline and traversal refusal.
- Mobile shared controllers test both adapters, read-only/revoked grants,
  conflicts, lost acknowledgement reconciliation, cwd pins, background/no replay,
  malformed/oversized text and legacy Terminal capability refusal.
- Existing Server lib test compilation was checked; a Files-only test filter
  matched zero tests. Do not present that as real WebSocket-route acceptance.
- Static quality findings remain unwaived: one new dispatch arm and thin typed
  metering/adapter forwarding are reported by the structural gate. They do not
  represent duplicate save implementations. Full report and compiler/native test
  receipts are retained privately; this is not an all-gates-green claim.

Roll out the runner before the supporting Server. Older Servers remain
preview-only with an explicit safe-save capability explanation. Do not change
pairing grants merely to make a client feature available.
