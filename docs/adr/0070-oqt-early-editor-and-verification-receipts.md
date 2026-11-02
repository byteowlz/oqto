# oqt early editor and runtime verification receipts

## Status

Implemented locally for standalone oqt, 2026-10-22; `oqto-hsyy`. Refines
ADR-0066 launch verification and ADR-0065 input admission. Source and staged
runtime only; activation is a separate operator decision.

## Context

Interactive oqt showed nothing until full runtime verification (≈3.6 s warm,
≈12 s with a cold page cache over ~29k entries), Node module load, host
start/attach and Session attachment had all completed. Upstream Pi has since
removed the experimental client/server stack oqt's transport adapter is pinned
to; its maintained interactive TUI accepts typing during late startup but
creates its Session runtime first and does not queue submissions.

## Decision

### Presentation owns startup; transport stays an adapter

`presentation-client.ts` mounts the native Pi editor (`CustomEditor`, same
component and keybindings) before host preparation, discovery or attachment,
then attaches the prepared Session behind it. The pinned experimental TUI is
reached only through a small reviewed adapter (`early_presentation_patches.py`):
`createPending`, `attachPrepared`, `failStartup`. Durable storage, the guard and
Session identities are unchanged. If upstream ships a Durable client/server
presentation, this adapter is deleted rather than maintained in parallel.

The first frame is a lightweight `StartupShell` (the same `CustomEditor`,
keybindings, theme and viewport layout). The client/server stack loads behind
it in measured stages, one per event-loop turn after a one-frame settle, so
keystrokes keep echoing while synchronous module evaluation runs; the launcher
loads smoke, demo and classic-import modules only on those paths. The full TUI
then adopts the shell's draft and the same queue in one synchronous step, so no
input is lost or duplicated. Attaching the lane no longer forces the chat screen
over an already open plugin dialog (a pre-existing race that the live renderer
made frequent: a pending approval vanished on a second client).

### One queued startup message, exactly-once dispatch, never replayed

`presentation-startup.ts` holds at most one submitted startup message. It is
released only after a complete binding — runtime, Session, conversation, model
and attached host process/operator endpoint — and dispatched once through the
normal prompt/steer admission with the attached view's router token, so a
branch selected by another client meanwhile rejects it. Esc withdraws it
before dispatch. Rejected, cancelled or startup-failed text returns to the
editor; a dispatch whose admission is unknown is reported as possibly sent and
is never retried automatically. Commands wait for readiness. Startup failure
keeps the editor and text, shows the host-log cause, and exits non-zero.

### Verification receipts

After full content verification, a receipt records a SHA-256 digest of every
runtime entry's (path, mode, uid, nlink, dev, inode, size, mtime, ctime),
including the manifest and root. It is written only if that digest is identical
before and after hashing, atomically, 0600, under the private per-runtime state
directory. A later launch accepts a receipt only if it is owned, single-link,
schema-valid, for the same artifact, at most 7 days old (≤5 min clock skew),
and the current stat digest matches; otherwise it re-verifies all content.

Any write, rename, replacement, chmod, link or utimes call changes ctime or
inode, which unprivileged processes cannot set. Receipts therefore detect
filesystem modification, not media corruption, root/clock manipulation or raw
device writes; weekly full verification bounds the media-corruption window.
Like ADR-0066, this is not a boundary against an unconfined same-UID process.

## Verification

12 queue tests; 5 receipt tests (fast path skips hashing; same-size content
change, restored mtime, mode, extra entry and manifest rewrite force full
verification; unsafe/stale/future/foreign/corrupt/hardlinked receipts and
non-private directories are ignored; concurrent change during hashing yields no
receipt). Real source PTY with the actual host lock held: typing, queue,
automatic single dispatch after attach, Esc cancel without dispatch, exit while
blocked, and startup failure with zero provider calls. Existing coding PTY
suite passes (13/15; demo `tui_test` and `classic_import` fail independently,
tracked as `oqto-y8xn` and `oqto-gkdr.1`).

Installed staged candidate, normal priority, disposable owner/state, n=5:
normal launch (receipt, warm host) first frame p50 0.81 s, typing visible
0.82 s; fresh state (full verification, cold host) 4.9 s. The previously active
runtime measured 8.05 s to visible typing with the same harness (n=3). A
message typed during startup was queued before attachment in 5/5 samples and
reached the provider exactly once.
