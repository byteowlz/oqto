# Session announce v1

A Pi extension tells its personal Runner "I am on this native session file",
so a new or switched Session is listed immediately instead of on the Runner's
five-minute discovery sweep.

An announce is a **hint, never authority**. It names one file; the Runner admits
exactly that file through the checks it already applies to discovery (owner,
symlink, canonical path, native root, Workspace root, header identity, pin) and
then invalidates its catalog. Nothing in this message grants access, control,
approval or history authority, and a listed Session is listed exactly as the
sweep would have listed it.

## Transport

- Unix domain `SOCK_STREAM`, in the user's private socket directory.
- One line in, one line out, then close. Lines are UTF-8 JSON, terminated by `\n`.
- Maximum announce and ack size: 8192 bytes. Larger input is refused unread.
- Socket path: `$OQTO_SESSION_ANNOUNCE_SOCKET`, else
  `<socketDirectory()>/pi-session-announce.sock`, where `socketDirectory()` is
  `XDG_RUNTIME_DIR` on Linux when it is an owned `0700` directory, and
  `os.tmpdir()` otherwise (macOS temp dirs are already per-user).
- When the Runner's configured socket root differs from that default directory,
  the Runner publishes `<socketDirectory()>/oqto-session-announce.path`: one
  line, the absolute socket path, mode `0600`. A client may read it only if it
  is a regular file owned by the same uid with mode `0600`.
- The Runner answers regardless of outcome. The extension never blocks Pi: a
  missing or slow Runner is not an error, and the sweep still lists the Session.

## Announce

```json
{
  "v": 1,
  "kind": "session_announce",
  "reason": "start",
  "session_file": "/home/owner/.pi/agent/sessions/--home-owner-work-app--/2026-11-05T05-40-30-123Z_ses_abc.jsonl",
  "session_id": "ses_abc",
  "cwd": "/home/owner/work/app",
  "pid": 4242,
  "live_socket": "/home/owner/.local/state/oqto/run/pi-tui-rpc-4242.sock",
  "previous_session_file": null
}
```

| field | required | meaning |
|---|---|---|
| `v` | yes | `1`. Other versions are refused as `wrong_version`. |
| `kind` | yes | `"session_announce"`. |
| `reason` | yes | `start`, `new`, `switch`, `fork`, `resume`. Informational: every reason means "validate this file now". |
| `session_file` | yes | Absolute, already canonical native JSONL path. |
| `session_id` | yes | Native session id from that file's header (`[A-Za-z0-9_-]{1,256}`). |
| `cwd` | yes | Absolute, already canonical working directory Pi reported. |
| `pid` | yes | Pi process id. Informational; the Runner does not probe liveness. |
| `live_socket` | no | pi-tui-rpc socket for that Session. Hint only: it need not exist yet, and when present it must be absolute and inside the Runner's socket root. |
| `previous_session_file` | no | Fork lineage. Absolute or `null`; the Runner never reads the parent. |

Unknown fields are refused. Relative paths, `.`/`..` segments, trailing slashes
and NUL bytes are refused as `invalid_path`.

## Ack

```json
{ "v": 1, "ok": true, "result": "listed", "detail": "" }
```

| result | `ok` | meaning |
|---|---|---|
| `listed` | true | Admitted now; the Session is listed. |
| `duplicate` | true | Already admitted; nothing changed. |
| `denied` | false | The file exists but failed admission (root, owner, symlink, Workspace, header identity, pin). |
| `invalid` | false | The announce itself was refused before any file was read; `detail` is the stable code. |

`invalid` codes: `empty`, `not_utf8`, `malformed`, `wrong_kind`, `wrong_version`,
`invalid_path`, `invalid_id`, `invalid_pid`, `too_large`.

`ok: false` is terminal: do not retry the same announce. Retry only transport
failures, a bounded number of times, without blocking Pi.

## Runner behaviour

1. Refuse a peer whose uid is not the Runner's, whatever the socket permissions say.
2. Read one bounded line within two seconds, decode and validate the wire shape.
3. Refuse a `session_file` outside the native session root, a canonical `cwd`
   outside the approved Workspaces, and a `live_socket` outside the socket root.
4. Read the native header through the same bounded reader discovery uses
   (owner, symlink, canonical path, identity, Workspace, uniqueness, pins). A
   header that is still being written is re-read for up to ~300 ms.
5. Refuse a header whose native id or Workspace differs from the announce.
6. Admit through the existing single-file writer, invalidate the catalog, ack.
7. Log every refusal; accept nothing from the message itself.

## Fixtures

`fixtures.json` holds five valid announces with their acks and eleven invalid
cases with their stable codes. `admission.cases` lists the refusals that need a
real filesystem or socket peer and are covered by Runner process tests instead:
out-of-root file, Workspace outside approved roots, live socket outside the
socket root, header identity mismatch, symlinked file, foreign-uid peer.
