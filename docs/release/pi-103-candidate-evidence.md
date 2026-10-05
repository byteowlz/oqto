# Pi1.0.3 isolated candidate: first qualification lane

Date: 2026-10-05. Issue: `oqto-ywm7`. Source commit: `5c9c2be3` on `feat/pi-qualified-stable-tracking`.

## Evidence

Private candidate: `/home/wismut/byteowlz/oqto-release-artifacts/pi-stable-103-5c9c2be3`.

- Discovery selected official stable1.0.3, rejecting floating/prerelease versions and downgrades. Source accepted pin remains1.0.0.
- Immutable source snapshot updated only candidate Pi/Linux archive pins and runtime package/lock. No live installation, source pin change or user config write.
- Official Node24.14.1 archive passed pinned SHA256 verification. npm installed121 packages with lifecycle scripts disabled, private HOME/cache and stripped environment. Each locked fetched package has SHA512 integrity.
- SDK publisher integrity: `sha512-t2lb0dw4y/jr5a2PRo6eTHGTZOPB3/YAMVyhhYFC1W3Hl5xE+462I/gMWjF4gCLuhGipNEfuNqONFmdqLFz4SQ==`.
- Candidate runtime lock SHA256: `5ba0b5b3a2306aa4b9c5418bf3930f4028eb3033f097c2ee5c47cdb41e990008`.
- Native exact-version RPC/session qualification passed. Durable evidence: `rpc-evidence/` and `oqto-pi103-rpc.log`.
-25 individual extensions plus combined catalog/session-start passed against exact1.0.3. Fixture source is frozen `a9a87c3d2ba0d97573941b2834fcc8497d8d83b5`, with `node_modules` bound to the candidate runtime (not a global/old SDK).
- Selection controls passed with/without capabilities, durable answers and zero model/chat/history messages. Synthetic-provider Selection tool callback and durable record passed.
- The two Selection proof files in the private fixture changed **only** their hardcoded expected binary version from1.0.0 to1.0.3. No implementation or source repository edits; original assertions remain enforced at the new exact version. This overlay is not a published extension SDK pin change.
-12 Python offline/stager tests pass, touched Python Ruff format/check pass, touched shell syntax and git diff checks pass.

The scheduled/manual workflow source is committed, but has not been merged, pushed, enabled or run on GitHub. Local commands exercised preparation/staging/RPC and additional extension/control proofs; this is not a claimed hosted-workflow execution.

## Remaining gates

No qualified automatic bump PR or accepted-pin promotion yet. The read-only workflow currently automates discovery/preparation/staging/native RPC, not all local extension proofs above. It must still incorporate those proofs and SDK type compilation, structured-fix collection2.3.1 qualification, coherent default Node packaging/acquisition, runner guards/strict doctor, packaged clean install/update/failed activation rollback/config preservation, authenticated full-shell service/sandbox/real-provider acceptance, physical CPU/native helper coverage and evidence-bound/idempotent proposal behavior.

The frozen2.2.1 collection used here is the existing release candidate pin; it does not incorporate the separately fixed structured-output defects in2.3.1. Its catalog success is not acceptance of those known defects.

npm emitted the upstream `node-domexception@1.0.0` deprecation warning. It is retained in staging evidence, not suppressed or called warning-free dependency qualification. Changed source lint is clean; release dependency warning acceptance remains outstanding.
