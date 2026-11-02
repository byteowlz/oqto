# Session media: native, managed and workspace previews (oqto-j08e)

Status: source implementation and isolated qualification; **not deployed**. Native
phone rendering/physical acceptance remains separate. Raw live Pi/TUI RPC frames
are not rewritten; this feature handles history/display media after admission.

## One renderer/cache, explicit original authorities

`oqto-media` owns deterministic image descriptors, wire projection, JPEG derivation
and the disposable cache. It does not authenticate a caller or choose an authority.

| Surface | Original authority | Admission/revalidation |
| --- | --- | --- |
| Personal/native history | The admitted Pi JSONL's conversation `message.content` | SessionObserve, configured roots, public/native mapping, native source pin, no-follow descriptor and unchanged source before/after; delivery revocation fence |
| Managed durable history | Selected oqto-log **message** JSON `content`/`parts` | Exact public-to-internal/native binding, controller-derived Workspace ceiling before reading payloads, unchanged selected record/binding before/after derivation; backend target/membership rechecks |
| Workspace raster preview | The admitted workspace file | Existing workspace proxy authorization; capability-relative no-follow open, source byte hash, stable descriptor and reopened path before publication |

Raw extension/control envelopes and private message extras are not image sources,
even when a caller knows their hash. Original Pi JSONL, original files and canonical
message payloads are never rewritten by projection, preview generation or cache
lookup. The hash identifies bytes; it does not grant access.

History replaces explicit inline image blocks with:

```json
{"type":"image","media_ref":{"sha256":"ORIGINAL_HASH","mime_type":"image/png","width":1200,"height":900,"byte_size":1048576}}
```

Managed refs also carry their opaque public `session_id`; an authored foreign
Session field cannot override the response's admitted context. User-image parts
have a typed optional `media_ref` in the projected part DTO. Tool input/output
images use the same descriptor. Unsupported/oversized inline blocks are omitted or
represented as unavailable rather than transferred as original base64.

Only identical image refs within a tool display payload are deduplicated. Distinct
messages survive even if text or image bytes are identical. Presentation and
normalization no longer remove assistant rows by visible-content fingerprints.
Image IDs/alt/caption strings are retained when projecting an explicit image block.

## Retrieval and presentation

Personal signed action (unchanged wire):

```json
{"operation":"session_media","session_id":"PUBLIC_ID","sha256":"ORIGINAL_HASH","variant":"thumb"}
```

Requires SessionObserve, not Files/Terminal grants or pairing alone. The explicit
PersonalSource binding used by split-home setups is currently required.

Managed cookie-authenticated binary endpoint:

```text
GET /api/chat-history/PUBLIC_ID/media/ORIGINAL_HASH?variant=thumb
GET /api/chat-history/PUBLIC_ID/media/ORIGINAL_HASH?variant=full
```

The backend uses Runner `GetWorkspaceSessionMedia`, not backend-home filesystem
reads. Its mandatory Workspace ceiling is full-principal owned history for a
personal target, within the admitted shared root, or the exact grant-admitted
remote workdir. The client cannot supply a source path or choose this ceiling.
The Runner rejects scope mismatches before scanning content or using the cache.
Native/external IDs are not aliases for a public ID. Ambiguous bindings are refused.

Shared history/media reads now use explicit read-only target resolution. Membership
is checked before placement/cached endpoint access and again after queued media
work. Viewer reads do not confer prompt/start/mutation permission; the existing
drive resolver still requires it. Remote machine metadata is not an execution grant.

Workbench history renders lazy thumbnails with one-click original links. Workspace
raster previews use `/api/workspace/files/thumbnail?path=…&workspace_path=…&size=768`;
the unchanged `/file` URL opens the original. SVG and non-raster files retain their
original path rather than entering a raster decoder. Styling reuses existing image
and preview classes; no new visual direction was selected.

Thumb JSON includes `source_sha256`, encoded-byte `sha256`, `variant`, MIME,
dimensions, `byte_size`, and `data_base64`. Managed HTTP exposes the decoded image
bytes with private/revalidation and nosniff headers. Full always returns exact
original bytes, not a recompression. Verify encoded bytes against response `sha256`;
do not overwrite the original cache entry with its thumbnail.

Headless native peer download:

```sh
oqto-runner personal peer media --identity /private/peer.json \
  --session-id PUBLIC_ID --sha256 ORIGINAL_HASH --variant thumb \
  --output /private/new-thumbnail.jpg
```

Creates a new mode-0600 file without overwrite; stdout is metadata, not base64.
Only full media gets the separate 12MiB peer response allowance. Managed/headless
clients can use the typed Runner operation or authenticated HTTP endpoint.

A Session image need not have a workspace file. Opening its original is supported;
revealing it in an OS file explorer requires a real file or explicit download.
Locality never grants access or automatically selects original quality.

## Cache configuration

Read once at service startup; changes require a restart, none performed here.
Runner-spawned file services inherit these environment settings.

| Setting | Default |
| --- | --- |
| `OQTO_MEDIA_CACHE_ENABLED` | `true`, only `true`/`false` accepted |
| `OQTO_MEDIA_CACHE_DIR` | OS cache directory + `oqto/media`, absolute dedicated path |
| `OQTO_MEDIA_CACHE_MAX_BYTES` | `268435456` (256MiB), positive |
| `OQTO_MEDIA_CACHE_IDLE_TTL_SECS` | `604800` (7 days), positive |

Linux: `$XDG_CACHE_HOME/oqto/media` or `~/.cache/oqto/media`. macOS:
`~/Library/Caches/oqto/media`, owned by the Runner/service user, not necessarily the
native Session owner. Invalid settings/unsafe global cache root refuse startup.
Disabled mode ignores existing cache contents and creates no directory; it does
not silently purge files.

File-service compatibility: an explicitly configured `thumbnail_cache_dir` uses a
new `media-v1` subdirectory (relative paths resolve against the service cwd), leaving
old files alone. Existing `max_thumbnail_age` controls image idle expiry unless the
new TTL environment setting is present; zero disables that service's image cache.
The existing video frame pipeline is unchanged, not migrated into this image cache.

0700 directories, 0600 entries; descriptor-relative no-follow operations,
nonblocking cross-process locks, atomic rename, interrupted-temp cleanup and
encoded hash/JPEG dimension validation. Version/size are part of each key. Least
recently used entries are evicted by actual JSON size, with an additional4096-entry
bound. Expiry/cleanup occurs on access, not in a background janitor; clock rollback
cannot extend retention. Unexpected files are preserved and caching fails safely.
Runtime cache I/O faults derive previews rather than widening authority.

Cache hits still read/hash the admitted original and revalidate its binding; they
save JPEG decode/resize/encode, not the original-source scan. No originals or full
history are cached. S3 is not required. Re-derivation needs no thumbnail migration,
but a Session transfer must include its authoritative original data, not just refs.
Revocation cannot erase bytes already downloaded by an authorized client.

## Bounds and proof

Session originals: 8MiB, 40 million pixels, each edge <=16384; PNG/JPEG/GIF/WebP.
Workspace preview input: 32MiB. Native source file: 512MiB, record: 16MiB, nesting96.
Managed content scan: 512MiB, record16MiB, at most1024 shards. Each source adapter
has two workers; decoder allocation limit192MiB. Preview sizes1–768 use versioned
`thumb-jpeg-fit{edge}-budget128k-v1`, trying quality65 at the requested edge, then
55 at two-thirds and45 at half, accepting <=128KiB JPEG. Bump the version for
incompatible encoder/policy changes. Previews are static, metadata-free JPEG.

`just test-personal-media` covers shared cache/renderer, workspace image HTTP
contract, native signed HTTPS/CLI, actual managed Runner transport, canonical source
scoping and backend permission/response tests. Managed fixture: three1MiB tool
screenshots plus a1MiB user image produce <16KiB history JSON; originals remain
byte-exact. Tests cover foreign/native-ID aliases, Workspace ceilings on warm
cache, private/control-record exclusion, record replacement, scoped file escape,
nonmember/revoked viewer, viewer read-vs-drive, cache corruption/expiry/LRU/version,
concurrent publication, and invalid startup settings.

Frontend tests cover page normalization, stable message preservation, thumbnail
URLs and original links. `just gen-types` now executes actual ts-rs export tests
and refuses a zero-test success. Do not hand-edit generated types.

Qualification also exposed invalid FTS update/delete triggers in the existing
ordinary contentful FTS5 table. Forward migration
`20261101001_message_fts_mutation.sql` fixes derived-index maintenance without
rewriting payloads; regression covers update, NULL, restore and cascading deletion.
No live databases were migrated. Deployment needs a schema-aware backup/rollback
plan and coordinated backend/Runner/files/frontend versions; older packaged tars
predate this work.

Still separate: raw live Pi/TUI RPC frame projection, typed native-source refusal
(`oqto-v07c`), native phone rendering/physical acceptance and installed rollout.
Synthetic fixtures do not establish a deployed product.
