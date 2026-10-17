# Personal host pairing wire

Implementation: `oqto-personal-wire` (host-neutral client/CLI types) and `oqto-personal` (local owner authorization store). Decision: [ADR-0062](../adr/0062-backendless-personal-host-device-pairing.md). This is not yet a runnable CLI/network service or phone acceptance; listener/client TLS, Files, PTY and session integration remain under `oqto-8x3p.10.7`.

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

Transport must verify the pinned TLS peer **before sending the ticket**, and verify TLS handshake signatures. `matches_leaf` alone is not a TLS verifier, and QR creation from a DER byte slice does not establish certificate validity or endpoint reachability. No HTTP endpoint is published by this component yet; consumers must not guess one. Signed RPC uses the separate `oqto.personal.rpc.v1` context and exact payload string; signatures still do not grant capabilities.

## Gates

`just test-personal-authority` checks both packages, QR rendering, proof/store integration, formatting and strict all-target Clippy. These are component tests, not a camera scanner test, pinned TLS service proof, native Pi control or file/terminal operation acceptance. Bootstrap crash/recovery and existing-device upgrade qualification remain required before deployment.
