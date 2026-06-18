# Backend agent guide

This Rust workspace is Oqto's control plane. Keep backend changes on the established seams; do not add shortcuts to make one symptom pass.

- Prefer crate ownership over monolith growth: `oqto` composes, `oqto-runner` owns harness processes, `oqto-protocol` owns wire types, `oqto-history` owns oqto-log/history storage, `oqto-sandbox` owns isolation, setup/provisioning crates own host mutation.
- Runtime actions go through the runner protocol. Do not call harnesses, write session files, or mutate history directly from unrelated API/CLI paths.
- History writes for runner sessions go through oqto-log abstractions. hstry is legacy/interop; do not introduce new hstry-primary runtime paths.
- Protocol/type changes must regenerate/check frontend types; do not hand-edit generated TypeScript to make builds pass.
- Session IDs need typed discipline: keep platform, external/Pi, and temp IDs distinct across API, runner, history, and frontend payloads.
- Sandbox/security/config work must fail closed and prove docs/examples match runtime behavior; unsupported modes should error, not silently degrade.
- User/host provisioning must be idempotent and preserve user-owned config with backup/diff/merge semantics.
- Rust production code: `anyhow::Result` + context, no `unwrap`/`expect`, no broad `allow` without rationale, no warning debt handoff.
- Test the touched crate with targeted `cargo fmt`, `cargo clippy`, and `cargo test` plus `just lint-rust-ai-guardrails`; broaden only when the changed seam crosses crates.
- If cargo fails under Claude Code with EPERM/statx/timer_create on git deps, treat it as harness sandbox capability and run cargo gates in an unsandboxed lane.
