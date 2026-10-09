# ADR-0055: Latest qualified stable Pi candidates

Status: Policy accepted by user on 2026-10-05; automation and release integration in progress (`oqto-ywm7`).

## Context

The permanent Pi1.0.0 freeze no longer meets the user's preference to keep up with upstream. Official1.0.3 is available. Discovery and binary startup alone cannot establish extension compatibility, durable session correctness, authorization or packaged rollback. The release candidate's Node runtime is qualified separately from its still-standalone default packaging path.

## Decision

Track the latest **qualified** official stable Pi. Preserve exact shipped pins, checksums, npm closure, runtime receipts and coherent rollback identity from ADR-0054. This supersedes only that ADR's permanent1.0.0 version requirement, not its deployment acceptance gates.

Daily/manual automation discovers the publisher stable channel and prepares an immutable source snapshot in a new private directory. It refuses prereleases, malformed metadata, downgrades, non-publisher assets, weak closure integrity, changed baseline pins and existing output paths. Major candidates require explicit reviewed admission. Node remains separately checksum-qualified; Pi updates never silently upgrade Node or derive an independent Oqto CPU floor.

The source worktree, accepted pin and user config are not modified. Candidate preparation is not qualification. A qualified bump proposal must carry version/base-commit keyed evidence and exact pins; it must not auto-merge, publish, install or promote live links. Failed or incomplete gates retain the known-good accepted baseline.

## Implemented first lane

`scripts/release/pi-update.py` discovers and prepares an isolated candidate with exact npm lock and official Linux standalone archive checksums. The qualified Node runtime supplied by the operator must match the accepted Node receipt and binary version; a receipt is provenance metadata, not independent proof against a malicious local operator replacing its binary. The candidate stager independently acquires and checksum-verifies the official Node archive.

`.github/workflows/pi-updates.yml` serializes daily/manual discovery, candidate preparation, locked-runtime staging and native RPC/session tests. It has read-only repository permissions and retains bounded evidence for14 days. Its schedule becomes effective only after an authorized merge to the default branch; it is not enabled by a local commit.

**No automatic PR or accepted-pin promotion is implemented yet.** Required integration blockers are extension SDK/type qualification, individual+combined extension catalog, controls/selection CAS/reconnect/no-Chat pollution, runner guards/strict doctor, full Node packaging/clean install/update/failed activation rollback/config preservation, authenticated full-shell service/sandbox/provider acceptance and physical CPU/native helper coverage. Synthetic provider and QEMU proofs remain separately labelled. This first lane cannot issue a ready bump PR when any of these is absent.

Existing standalone update/install entrypoints are not used by this workflow. Their narrow compatibility gate is not qualification for the Node/npm package, and must not be substituted for the missing promotion contract.

## Verification

- Offline regression tests cover exact stable selection, current/no-op, major admission, malformed publisher metadata, downgrade, asset/version identity, duplicate checksums, partial lock disagreement, weak/untrusted closure entries, unique external pin updates and destination preservation.
- Native preparation/staging/RPC evidence must name exact versions, base commit, lock hash, publisher integrity and standalone checksums.
- Qualification failures never update source pins or user homes. Future proposal integration needs duplicate/concurrent candidate, partial-update recovery, binding of evidence to candidate files and rollback regressions before acquiring repository-write authority.
