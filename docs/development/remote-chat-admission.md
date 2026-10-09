# Remote chat admission checkpoint

Tracked by `oqto-8x3p` / `oqto-8x3p.1`. This checkpoint hardens the existing routing path; it does **not** enable Mac chat creation.

## Safety contract

- The owning runner's oqto-log remains the durable chat authority; `chat_session_targets` is routing metadata, not a transcript store.
- Existing Sessions resolve their durable target before caller-supplied paths or connection metadata. Personal bindings require the owning Account; shared bindings require current membership before dialing or exposing services.
- No cached RunnerClient confers authority. Connection-local runner overrides have been removed. Existing Session work-directory bindings cannot be changed through `session.create`.
- New Sessions with a requested work directory must resolve that target successfully. A denied membership, unavailable endpoint, or readiness failure is an error, never a reason to use the personal runner. Only a missing requested work directory selects the existing personal default.
- An existing shared path without membership is distinguished from a path that is not shared. Old fallback branches that returned empty history, acknowledged unapplied settings, or queried personal models after failed resolution have been removed.
- Public/native ID mapping and runner event translation are unchanged. Reconnect admission uses durable routing metadata; no message reconciliation by content, position or count is introduced.

## Regression evidence

`api/ws_multiplexed/admission_tests.rs` uses disposable databases and socket fixtures to check denied and offline targets, unchanged Session binding/connection metadata on failure, no personal-runner connections, current authorization despite cached display metadata, rejection of work-directory rebinding, service-exposure authorization, and a successful authorized endpoint handshake.

Every fixture Account has an explicit disposable personal Placement too: a regression must hit a spy endpoint, not the ambient `RunnerClient::for_user` socket. The first draft of the test fixture lacked this and exposed the production-default fallback; its attempted spawn was rejected during sandbox construction. The fixture now mechanically prevents that path rather than depending on test-runner environment assumptions.

Verification: `cargo test --locked -j 1 -p oqto --bin oqto` (291 passed, one existing ignored), `cargo clippy --locked -j 1 -p oqto --all-targets -- -D warnings`, and `cargo fmt -p oqto -- --check`. No runner wire or generated-type schema changes.

## Remaining delivery

The approved next slice is multiple dedicated Workspaces per machine, initially the Mac's `~/byteowlz/`, explicit workspace admission, remote path validation, create/send/stream/stop/reopen against the Mac, and proof of that runner's oqto-log history. Machine grouping should put This machine and Mac at the same sidebar level; offline machines and their entries are greyed out and cannot execute. IndexedDB is a bounded, Account/deployment-scoped disposable cache added only after remote authority/reload behavior works. No central history replica, offline prompt queue, automatic migration, or Linux fallback is implied.
