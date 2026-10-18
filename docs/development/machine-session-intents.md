# Explicit Machine Session intents

Tracked by `oqto-8x3p.10.3`, under `oqto-8x3p.10`. Source hardening only; no deployment or live TUI completion claim.

## Routing invariant

An agent command's non-default `runner_id` constrains the user's requested Machine; it never grants access or selects a socket. Previously the field labeled replies even when a requested work directory resolved to a different runner. `ws_multiplexed::machine_intent::validate` now checks before any runner resolution/dial:

- current Account execution grant and its explicit root ceilings;
- existing durable Session owner, remote-machine ID and exact work directory, ahead of caller paths or connection metadata;
- for new Sessions only, an explicit work directory that resolves to the same granted Machine;
- non-create commands require a durable binding. Revocation is rechecked for every command.

Wrong/unknown/foreign/revoked targets cannot probe the personal runner as a fallback. Failure creates no connection metadata or Session binding. Existing default/local commands still use their existing admission path. Successful admission does not skip the subsequent runner readiness, workspace and authorization checks.

`GET /api/runner-targets` adds `session_route_pinning: true`, a Server compatibility fact. It is not an execution grant, containment attestation, observation permission or input lease. New native clients default a missing field to false rather than trust an older Server that merely echoes the hint.

## Continuation is a separate safety gate

Route pinning does **not** prove a Pi file is idle or safely adopted. Mobile also defaults a missing per-Session `continuation_supported` fact to false; only the owning runner's integrated quiescent/adopted single-writer path may report it true. This checkpoint does not emit that fact. Do not set it from an inventory flag, command name, client request, connection status or sandbox-profile request. A live terminal Session must be attached through its admitted transport, never reopened by another Pi process. Full-access input additionally needs the Server's recent-MFA policy and terminal-confirmed/revocable lease. Canonical snapshot/tail convergence, containment facts and those control gates remain `oqto-8x3p.10` acceptance.

History authority remains the owning runner's oqto-log. Public/native IDs and event translation are unchanged; no raw socket traffic or second transcript store is introduced. The mobile canonical reducer continues to reject foreign-runner events and never automatically replays uncertain machine input.

## Evidence

Disposable DB/transport-spy tests in `api/ws_multiplexed/admission_tests.rs` cover missing/wrong/escaping work directories before any personal-runner dial, foreign Account, unknown Machine, wrong durable scope/Machine/owner/path and revoked grants. Matching new/bound intents pass the preflight; ordinary commands retain existing admission. The full handler's wrong-path test fails before the hook and passes with it.

The Mac verification lane also exposed Linux-only peer-credential code/imports. The admin Unix-socket helper now uses Tokio's platform peer credentials instead of substituting this process's uid on non-Linux platforms. This removes the lane's warnings and has a socket-pair check; it is not a foreign-UID or deployment security acceptance claim. A primary-checkout-only read-probe helper reorder was needed for that older snapshot's strict Clippy; it is not part of the isolated published diff.

The isolated source fixture of the published origin/main-based tree passed **328 server tests (2 existing ignored)**, Cargo check, strict all-target Clippy and format check. No branch switch, additional Git worktree or publication of the primary checkout's four unrelated local commits was needed. Pin `RUSTUP_TOOLCHAIN=1.97.0` for every Cargo invocation in this nested fixture: an implicit clippy-driver toolchain lookup otherwise selected a different parent/fixture Rust version and failed with E0514; no shared cargo cache was cleaned.

Local logs: `target/machine-intent-proof/`. Reproduce with `cargo test --manifest-path backend/Cargo.toml --locked -p oqto --bin oqto`, `cargo check --manifest-path backend/Cargo.toml --locked -p oqto`, `cargo clippy --manifest-path backend/Cargo.toml --locked -p oqto --all-targets -- -D warnings` and Rust format check. Tests regenerate unrelated frontend type files; they are not part of this scoped change.
