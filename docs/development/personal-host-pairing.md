# Personal host pairing wire

Implementation: `oqto-personal-wire` (host-neutral client/CLI types) and `oqto-personal` (local owner authorization store). Decision: [ADR-0062](../adr/0062-backendless-personal-host-device-pairing.md). An enrollment-only HTTPS adapter and optional exact-pin TLS client are implemented for Runner composition and fixtures. No runnable CLI pairing flow or phone acceptance exists yet; Runner mounting, Files, PTY and session integration remain under `oqto-8x3p.10.7`.

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

## Gates

`just test-personal-authority` checks both packages, QR rendering, proof/store integration, formatting and strict all-target Clippy. These include real loopback pinned-TLS enrollment and wrong-pin/copied-certificate rejection. They are not a camera scanner test, running-host/phone acceptance, native Pi control or file/terminal operation acceptance. Bootstrap crash/recovery and existing-device upgrade qualification remain required before deployment.
