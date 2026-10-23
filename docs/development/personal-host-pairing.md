# Personal host pairing wire

Implementation: `oqto-personal-wire` (host-neutral client/CLI types), `oqto-personal` (local owner authorization), and Runner semantic actions. Decisions: [ADR-0062](../adr/0062-backendless-personal-host-device-pairing.md), [native graph convergence](../adr/0063-native-session-graph-convergence.md). Runnable CLI/HTTPS Files, PTY and existing-Pi attachment/control have disposable development proofs. Mobile wiring, frozen native qualification and deployment acceptance remain open under `oqto-8x3p.10.7`; development tests are not phone acceptance.

## QR v1

A UTF-8 JSON object, at most 2048 bytes, with exactly these fields:

- `version`: integer `1`.
- `purpose`: `oqto.personal-host.v1`; a CrossLatch QR is not an Oqto QR.
- `host_id`: canonical UUID persisted by the local authority, stable across reopen. This is not an Account, Principal, certificate or device identity.
- `url`: canonical root HTTPS origin, including trailing `/`. No credentials, query, fragment or application path. At most 1024 bytes after URL canonicalization.
- `pin`: lowercase SHA-256 hex of the configured server's DER leaf certificate, not a CA or PEM hash.
- `token`: lowercase 64-hex one-use secret. Only its hash is persisted.
- `expires_at`: Unix seconds. Server tickets last five minutes; client parsing allows up to 60 seconds of behind-clock skew. Server expiry and one-use consumption remain authoritative.

The QR deliberately contains no grants, host filesystem paths or private keys. Owner-selected session/Files/terminal grants live in the ticket store and cannot be widened by enrollment input. QR JSON, SVG and terminal renderings are sensitive intentional output, never chat/analytics/log receipts. The QR type has no Debug or unchecked Deserialize implementation. Rendering is an optional `qr` feature, without the image stack or SQLite in the wire crate.

## Enrollment

`decode_enrollment` bounds input to 4096 bytes and rejects unknown fields, including client-supplied grants. The exact Ed25519 enrollment shape and signing bytes are provided by immutable `xlatch-wire-auth`; Oqto uses the compiled `oqto.personal.pair.v1` context, not `xlatch.pair.v1`. `PairingAuthority::enroll_json` performs bounded decoding, possession proof and transactional ticket consumption, returning a new device identity with only the ticket's grants. Device identity and host identity must not be interchanged.

Transport must verify the pinned TLS peer **before sending the ticket**, and verify TLS handshake signatures. `matches_leaf` alone is not a TLS verifier, and QR creation from a DER byte slice does not establish certificate validity or endpoint reachability. The enrollment-only adapter serves `POST /personal/v1/enroll`, returning `{version:1,host_id,device_id}` on success. Clients must correlate the host identity with the QR. It does not expose grant issuance or execution routes. Refusals return a generic 401 without proof/token/error details; oversized bodies return 413. An uncertain enrollment outcome must not cause automatic replay or privilege widening. Signed RPC uses the separate `oqto.personal.rpc.v1` context and exact payload string; signatures still do not grant capabilities.

## TLS trust and lifecycle

The optional wire `tls` feature builds an exact-leaf, owner-pinned TLS 1.3 client. It verifies CertificateVerify through rustls's crypto provider: a copied public certificate without its private key fails. There is no CA fallback, accept-any mode, early data or resumption. Pin trust is explicit and **does not assert public PKIX CA, DNS-name or certificate-lifetime validation**; the validated QR origin and exact pinned leaf identify the owner-selected endpoint. Certificate renewal/rotation requires explicit trust handling, not fallback to an arbitrary certificate.

The enrollment server accepts a caller-owned listener/config and shutdown future, so it can be composed into the Runner rather than supervising sessions separately. It bounds active connections to 64, TLS handshakes and HTTP headers to five seconds, enrollment bodies to 4096 bytes, and HTTP connection lifetime to 30 seconds. Those are enrollment bounds, not a long-lived session/PTTY lifecycle. No service is installed or started by constructing the adapter.

## Runner composition and owner operations

The source one-command flow is `oqto-runner pair`: optional terminal selection,
equivalent flags and same-process hot-enable through a private owner controller.
See [pairing CLI](runner-pairing-cli.md) for readiness, output and acceptance
boundaries. In particular, the installed Studio's isolated native-source binding
is still a blocker; the source feature is not evidence of a live Mac update.
The explicit composition flags below remain the lower-level operator interface.

Use the dedicated host Principal with `single_user=true` and Linux-user provisioning disabled. Start with an explicitly reachable HTTPS origin and no implicit grants, for example:

```sh
oqto-runner --personal-listen 0.0.0.0:39444 \
  --personal-origin https://your-tailnet-host:39444 \
  --personal-workspace-root /absolute/canonical/work-directory \
  --personal-no-pair
```

Without another explicit endpoint this is standalone personal mode. Add the existing `--socket PATH` or `--listen-tls ADDRESS` plus its unchanged mTLS options to cohost personal HTTPS in the **same** Runner. There is one Pi manager and one serialized history-ingest task; pairing never authorizes the other transport. Do not run a second Runner over the same history just to add pairing.

The owner uses `oqto-runner personal pair --sessions observe` (or `control`), independently adding `--read-root`, `--write-root` or `--full-user-terminal`. `personal devices` lists enrollments; `personal revoke DEVICE_ID` revokes one. Custom server state requires matching `--state` on owner commands. JSON/SVG QR output remains an intentional secret-bearing owner output, never a diagnostic receipt. Unknown configuration, device identities and stable TLS identity must survive upgrade; corrupt identity is an error, not automatic key rotation.

Existing-Pi session support requires explicit canonical Workspace roots, an exact oqto-log public/native identity, a matching owned JSONL header and a matching live native peer. Current `pi-tui-rpc` must report `get_state.leafId`; missing head evidence is unavailable, not an inferred last entry. Attachment alone never requests a lease. Control additionally needs the independent Device control grant and a fresh terminal confirmation on that connection. Release/detach, cancellation, idle timeout or revoke close only the Runner-owned connection—not terminal-owned Pi. After dispatch timeout the outcome is uncertain: never automatically resend input.

Session handles are Device/generation-bound, capped at eight per Device and128 overall (including in-flight work), with a60-second idle deadline. Snapshot mode drains private native events without claiming their replay. Chat reads serve oqto-log only and fence page reads against head revision changes. A single background writer obtains a fresh native head itself, then merges immutable entry/parent facts; independent Device nudge arrival order cannot select an old Branch. Legacy provenance and sibling paths remain durable. History may be temporarily unavailable during initial synchronization. This is canonical-history polling plus native activity state, **not token-delta streaming**.

The composed listener uses a45-second connection lifetime to allow30-second terminal confirmation plus transport overhead, with one HTTP request per connection; enrollment-only remains30 seconds. TLS/header/connection-count bounds otherwise remain unchanged.

## Explicit native source contract

Use these **additional owner startup flags** when the existing Runner's service
HOME/TMPDIR differ from the terminal-owned Pi source:

```sh
--personal-native-session-root /absolute/canonical/native-user/.pi/agent/sessions \
--personal-native-socket-root /absolute/canonical/native-user-tmp \
--personal-workspace-root /absolute/canonical/approved-work-directory
```

Keep the existing service UID, HOME, XDG environment, config, mTLS options and
personal state path unchanged. No automatic exposure or grant creation occurs.
Without the new source flag the existing source/socket defaults are unchanged.
With it, the native root, socket root and Workspace roots must exist, be
canonical absolute directories and belong to the Runner's effective UID.
The socket-root flag defaults only to canonicalized service TMPDIR; select the
terminal's actual owned socket directory explicitly for split-TMPDIR setups.
The new roots are CLI-only, resolved before workers/listeners; `history_read.home`
and `runner.pi_sessions_dir` do not implicitly authorize this personal source.
No client wire change is required.

The one existing ingest task retains its default HOME-based legacy sweep and
adds bounded header-only discovery of the selected native source at startup and
on its five-minute sweep (legacy nudges also request discovery). Discovery reads
exactly the first newline-terminated header, at most16KiB, with no read-ahead
into bodies. It examines at most4096 immediate source directories and32768
entries total; a bound violation defers the whole discovery pass. Symlinks,
foreign-owned files/directories, malformed/partial headers, ambiguous native
files, wrong encoded directories/filename IDs and outside-root/noncanonical cwd
are rejected. Work directories follow the catalog's depth-four limit, excluding
hidden, `node_modules`, `target` and `vendor` children. This is not all-profile
bulk import or a promise of instantaneous catalog coverage. Each pass inventories
retained default-source filenames once. Initial shard probing and read-only
preflight are polled beside the native queue, not awaited before servicing it.
There is at most one header commit per scheduling turn, on the existing writer;
an in-flight commit must finish. Legacy work remains serialized behind discovery
and is retained while authority is unavailable. Native-only nudges do not request
another discovery pass.

Native identity reads and canonical catalogs share two process-lifetime SQLite
reader connections/workers, regardless of shard count, HOME or overlapping
validators. Each is established exactly once on a caller-independent initializer
thread/runtime, without an establishment timeout that could discard a live worker.
The two initializer threads exit after handoff. Caller cancellation/runtime shutdown
returns the same connection synchronously; it never releases capacity for a new
worker. No pool/recycling/replacement path exists. Initialization or irrecoverable
worker failure fails closed for that slot until process restart (not an automatic
restart request); shard/attachment errors can recover on the same readers. Each
connection attaches at most one exact `mode=ro` shard to an empty in-memory main
database, then detaches it; queued SQL and cancelled attachments must be cleaned
successfully before another shard is used. Statement caching is disabled. Canonical metadata enumeration has one process-wide blocking-scan
permit; header/default-filename snapshots have a separate single-scanner permit
and five-second caller deadline. Permits remain inside blocking jobs across
cancellation, so retries cannot fan out scanner threads. Filename filtering is
also off the writer, using a reverse-name prefix index that retains exact suffix
semantics, including underscores. Default metadata errors refuse discovery rather
than masquerading as an empty source. These bounds cover identity-reader resources,
not all existing Runner/socket/store resources; the existing serialized writer
still owns destination migration and pin writes.

All shard directories are enumerated strictly, without flattening errors or
skipping unreadable/missing shard databases. Inventory classifies directory entries
before metadata lookup: known non-directory, non-symlink entries (including the
index's retiring WAL/SHM files) are not shard authority. Directory and symlink
metadata failures still refuse the whole inventory. A deterministic disposable
regression retires regular sidecars after enumeration through the actual binding
check and discovery APIs, while retaining missing-shard refusal and shard bytes.
No ownership or schema answers are
cached: preflight requests retain no identity facts, and commit re-enumerates and
reads current global facts again. Newly added/upgraded shards participate without
a remove-before-reopen window. Initial/read/verification failure aborts and
poisons the whole batch; it cannot resume after silently losing a shard. Complete
identity scans have a five-second caller deadline, reader acquisition (availability
plus idle ping) two seconds and SQLite busy waits100ms. A blocked establishment keeps its fixed slot;
caller timeout cannot create a replacement. These are refusal bounds, not a disk/host latency SLA.

Header-only sessions get distinct deterministic public IDs through oqto-log's
strict identity/binding API, even when fresh and empty. Existing public IDs,
owners, legacy turns, Branches and Fork provenance remain intact. Conflicting
or dangling binding facts fail closed, never trigger importer conflict repair.
Retained April/July migration-prefix stores remain catalog/identity-readable even
without their original JSONL. Readers select only columns/tables present in that
schema, retaining all old identity facts and any existing Fork lineage; absent
columns represent facts that did not exist yet. List/attach do not migrate a
shard. Only a destination writer may upgrade it before committing a new pin.
The canonical Session stores an immutable source pin (canonical source root and
file paths, device/inode identity, native ID and cwd). A same-size/mtime copy in
another root or replacement inode cannot inherit that binding. Such a change
requires explicit owner-reviewed reconciliation, not silent rebinding or a
symlink workaround. Filesystem identity pins are not a defense against arbitrary
unsandboxed execution by the same Principal.

Header-only discovery records identities without titles or activity. Each
listed Session therefore also carries display-only metadata
(`personal_catalog.rs`): `display_title` (canonical title, else the live Pi's
name, else the latest native `session_info` name without its readable-id suffix,
else the first prompt), `activity_at_ms` (native file modification time),
`workspace_root` (longest matching designated root) and `live`/`streaming`. The
list is ordered live first, then by recent activity. Only the 400 most recently
modified files are read, 64 KiB from each end and complete lines only, cached by
file identity and size/mtime. Liveness sends one read-only `get_state` per
pi-tui-rpc socket with attachment's socket and peer checks, cached for three
seconds; a Pi claimed by two sockets or outside the Session root is not live.
None of this is identity, history or control authority: canonical `title` and
the existing fields are unchanged, and older clients ignore the additions.
History for a Session never synchronized from a live Pi still returns `503`.

Native-source pins (`extensions_json.personal_native_source`) record the Session file's
path, inode and persistent filesystem identity: the volume UUID on macOS, where
`st_dev` is a mount-instance ID that changes after a reboot. Validation accepts that
pin, or a version 1 pin (with `dev`) whose device number still matches. Pins
written before a remount stop matching; `oqto-runner personal repair-native-pins
--home <service HOME>` reports them, and with `--apply` re-pins (compare-and-swap,
safe while the Runner runs) only those whose path, inode, native ID, Workspace and
session-root path and inode all still match. Replaced files are left for the owner.
Discovery never re-pins an existing Session on its own.

List/attach/history resolve against the original service HOME. Catalog reads
are read-only; writer identity commits and legacy ingestion invalidate the
30-second catalog cache, including in-flight cache-fill fencing. Explicit native
sources distinguish Checking, Discovering, Ready and Failed internally. Checking
catalog/history requests and failed-authority session operations return the
existing `503 unavailable`; normal Discovering may return the existing partial
catalog without claiming completeness. Known faults immediately fence cached
lists and new/native control actions; detach/release cleanup remains available.
A complete retry runs every five seconds, keeps failed readiness until successful,
and must match the current fault generation before clearing failure. It also
invalidates any pre-failure catalog. Capability/enrollment shapes are unchanged
for strict installed clients: transport liveness or supported-capability booleans
are not Session-authority readiness. An initial existing-pin attachment can still
prove its own complete fresh authority while the independent initial probe waits.
The cache contains canonical rows only, not path authority: attachment revalidates the
configured root identities, header and durable pin before socket discovery.
The additional source never uses the legacy size/mtime cursor or count-based
replacement path. A nudge cannot import an external body or fall back to ambient
HOME. Only an admitted snapshot nudges graph ingestion; on that same serialized
writer a fresh socket `get_state.leafId` selects the native graph. Root/pin and
grant checks run before and after graph parsing. Missing head, partial tail,
changed immutable entry facts or revoked authority remain unavailable; no last
entry guess, managed adoption, second Pi or JSONL rewrite/restore is allowed.

Pi candidate selection during config loading is filesystem-only. Explicit
managed create/model-discovery launch paths retain version/RPC qualification,
failing closed on the selected candidate; existing-Pi personal paths execute
neither qualification nor a Pi process. Disposable executable sentinels test
full startup, owner commands, mTLS-denied managed launch, native history/control,
uncertain input and restart. This is source qualification, not activation or
real phone/native-session acceptance.

## Gates

`just test-personal-authority` checks both packages, QR rendering, proof/store integration, formatting and strict all-target Clippy. These include real loopback pinned-TLS enrollment and wrong-pin/copied-certificate rejection. They are not camera-scanner or phone acceptance. `just test-personal-host` runs formatting and strict all-target Clippy **before** history/library/process tests; new fixtures cannot bypass lint. It qualifies the deployed/default Runner features, not optional Iroh. `cargo test -p oqto-runner --test personal_host` uses actual disposable Runner processes, private HOME/TMPDIR, pinned HTTPS and CLI enrollment to prove independent Files/PTTY grants and Unix+HTTPS cohosted existing-session history/control/revocation. Its native peer is synthetic; real terminal, frozen Linux/macOS and mobile acceptance remain separately required. The split-source process fixture retains actual April/July migration prefixes without matching default JSONL, verifies cold catalog/discovery and unchanged complete shard bytes across list/attach/restart, and retains old owner/history/Fork/source provenance. A second process fixture adds3821 synthetic header-only sources across eight workspaces and asserts queued native head convergence before discovery completes; it uses no private sources and is not a real-host timing guarantee. A resource/recovery process fixture adds192 distinct retained shards outside the approved roots, injects initial and later SQLite failures, overlaps eight validators, asserts503 refusal/recovery and unchanged strict capabilities, and samples only its disposable child's threads/FDs. It uses two Tokio test workers without changing OS limits or real runtime settings. History tests cover12 overlapping validators, initial/post-upgrade reopen failure, poisoned-batch refusal, fresh commits after preflight, WAL visibility and cancelled read cleanup. A fresh-process cancellation regression stalls actual SQLite progress callbacks during establishment and idle acquisition, overlaps validators/catalogs across four synthetic homes, aborts fourteen callers and shuts down their runtime while workers remain blocked, then requires sixteen successful reads on the same two physical worker threads. ConnectionState-owned captures measure physical-owner overlap, not pool size; test-only hooks add stalls, not production lifetime enforcement. The native session library fixture repeats all original assertions with the initial inventory held pending and requires both heads to converge before releasing it. The library fixtures also cover bootstrap process deaths, identity persistence and canonical equal-count head switching without rewriting native files.
