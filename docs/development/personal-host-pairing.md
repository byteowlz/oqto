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

## Gates

`just test-personal-authority` checks both packages, QR rendering, proof/store integration, formatting and strict all-target Clippy. These include real loopback pinned-TLS enrollment and wrong-pin/copied-certificate rejection. They are not camera-scanner or phone acceptance. `just test-personal-host` runs formatting and strict all-target Clippy **before** history/library/process tests; new fixtures cannot bypass lint. It qualifies the deployed/default Runner features, not optional Iroh. `cargo test -p oqto-runner --test personal_host` uses actual disposable Runner processes, private HOME/TMPDIR, pinned HTTPS and CLI enrollment to prove independent Files/PTTY grants and Unix+HTTPS cohosted existing-session history/control/revocation. Its native peer is synthetic; real terminal, frozen Linux/macOS and mobile acceptance remain separately required. The library fixtures also cover bootstrap process deaths, identity persistence and canonical equal-count head switching without rewriting native files.
