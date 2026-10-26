# Runner-owned Pi classic native history paging

Personal Pi classic history is served by the Runner, alongside—not instead of—
`oqto-log` history in Server mode. Clients do not read files, select native heads
or reconstruct branches. Personal catalog identity and source pins remain in
`oqto-log`; this reader is not another persistence authority or importer.
Pi-Durable/oq/oqt storage integration is deliberately out of scope.

## Why the old reader failed

`personal_catalog::live_history` read the entire native file into `NativeGraph`,
retained every payload and then projected one page. Its 64 MiB allocation/parse
bound rejected large live Sessions even when the Runner had an exact runtime
head. Falling back to an unsynchronized canonical store produced unavailable
history. The fix is bounded indexed paging, not a larger whole-file allocation.
The canonical importer's existing graph parser and Server path are unchanged.

## Reader and cache

`oqto-history::oqto_log::store::native_graph::reader::NativeFileIndex` scans a
captured file length into bounded ID/parent/offset/digest metadata. It checks
complete record syntax and UTF-8, unique IDs, already-present parent links and
known `AgentMessage` metadata types. As in Pi's session manager, an entry whose
parent was never written starts its branch (Pi treats orphans as roots and stops
the path there); real files contain such gaps. A parent written later in the file,
or an entry naming itself, is a link Pi would follow and is refused, never cut.
The canonical importer applies the same rule. Arbitrary parent containers are rejected
without allocating a JSON tree. Unknown payloads are not materialized as Values. Partial final records are
not facts and cannot provide a selected head. A cold request still scans metadata
across the file; this is not an unvalidated EOF slice or a zero-I/O cold open.

The caller supplies the actual observed runtime leaf, including an explicitly
empty head. Live probes require `leafId` to be present: missing evidence is not
silently converted into an empty history. The index follows only that ancestor path. It seeks and decodes only
the requested visible message payloads, reusing `Entry::branch_message` and the
existing projector for IDs, parent links, timestamps, parts, hidden aborted/bash
messages, authored JSON and client-ID suffix handling.

`personal_catalog::history` keeps four in-memory indexes, keyed by path,
native/workspace identity and file device/inode/size/mtime/ctime (nanoseconds
included). Head switches reuse metadata but select a fresh ancestor path.
Append, rewrite, truncate and replacement invalidate the index; changed sources
currently get a new bounded metadata scan, not speculative incremental reuse.
There is no persistent index, prefetch job or new background service.

## Bounds and safety

- File snapshot: at most 2 GiB; at most 100,000 complete entries.
- Header: 16 KiB; individual record: 16 MiB.
- Page: normally 50 visible messages; at most 8 MiB of selected record bytes.
  Pages shorten at the byte bound without dropping messages. A single oversized
  visible record fails explicitly instead of returning a falsely empty page.
- Projected reply: at most 16 MiB. The generic index interface also caps requested
  page count at 200; the personal route requests 50.
- Cooperative index scan deadline: ten seconds, checked between records. This is
  not preemption of a blocked kernel filesystem operation.
- Two blocking page workers maximum. A started worker owns its semaphore permit
  even if the HTTP caller cancels, so cancellation cannot accumulate workers.

Existing signed admission, grants, public/native mapping and peer/socket checks
remain required. The personal caller checks the native source/header/pin before
and after the read. Workers open only owner-owned regular files with no-follow,
close-on-exec and nonblocking flags; fence descriptor and published-file identity
and metadata around reads; and verify selected record digests against the index.
Cache hits confer no authority. No native-file writes, Pi launch/reload, input
lease, mutation replay, root relaxation or source repair is introduced.

## Cursor behavior

Cursors remain opaque strings in the existing history response. Indexed cursors
use `native:v2:<visible-offset>:<public-message-anchor>`. The anchor must still
be on the selected ancestor path at that offset and is qualified by public
Session identity. Same-branch growth can continue older paging; a branch switch
or another public Session cannot silently mix pages. Legacy numeric native
cursors must refresh the newest page. No client schema/code change is required.
When the Pi disappears, native cursors still fail and the client refreshes;
never-synchronized archived history is not solved or guessed from file order.

## Verification

`oqto-24hw` owns this change. The signed HTTPS large-source regression returned
503 on the old reader, then 200 after the fix, without a prior native import.
A >64 MiB synthetic source has 120 selected messages, a sibling branch and large
later off-branch messages. Its observe/status/history/detach flow matches the
phone: three pages match the existing small-graph projection, retain unique IDs,
and acquire no input leases or writes, even while canonical ingest is deferred.

Reader tests cover projection parity, hidden/structural messages, authored JSON,
empty/missing/partial heads, malformed parents, duplicate IDs, byte/page/file/
entry bounds, stale selected payloads, same-branch cursor growth and cross-branch/
Session cursor denial. Runner cache tests exercise same-length rewrite, append,
truncate, replacement, wrong identity and no-follow rejection. An instrumented
read test proves an indexed page reads only its selected record, not older or
off-branch payloads.

The opt-in release probe uses a generated 328,184,810-byte file: 5,300 metadata
entries, about 1.1 s cold indexing and 2 ms for six indexed 50-message pages on
the development Mac. Cold means no metadata index; the probe does not flush the
OS file cache. These are local synthetic measurements, not cold-storage, phone
or network latency guarantees. Run it explicitly with the pinned toolchain:

```sh
just test-personal-native-history-bench
```

`just test-personal-host` runs formatting, strict all-target history/runner
Clippy, both library suites and all disposable signed-HTTPS process cases.
Deployment, real-host large-session recovery and physical phone acceptance are
separate gates; source tests do not authorize restarting services.
