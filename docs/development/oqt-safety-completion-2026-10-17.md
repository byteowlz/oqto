# oqt safety completion — 2026-10-17

Scope: `oqto-g7rk.2` and guardrail blocker `oqto-db6j`, source-only Linux local
composition. Pi pin `cdaf6c227d4718a72b59ccd02152c9614df44cfc`.
No deployment, installed launcher update, main-repository commit, owner-config
migration or live-session manipulation. Unrelated working-tree work is retained.

## Implemented contract

See [ADR-0064](../adr/0064-worker-owned-operator-result-receipts.md) and
[operator documentation](../../tools/oqt/README.md#operator-commands-and-durable-receipts).

- Guard journals before effects; model/operator serialization includes confirmed
  descendant retirement. Cancellation, disconnect, revocation and expiry cannot
  authorize a queued invocation or prematurely release execution ownership.
- Native worker owns durable result content. Protected permission storage holds
  sealed effect hashes/tombstones, not output or a second Chat timeline. Hosted
  canonical history remains `oqto-log`.
- `!` uses passive native write admission, never prompt injection or a local
  output queue. Admission acknowledgment is not placement/model delivery.
- `!!` sends only an opaque envelope into the brain. Command/output never enter
  native documents, native entries or model context. Private output is not
  recoverable from Session history.
- IDs bind exact payloads and Sessions. Lost acknowledgments reconcile against
  sealed native receipts without repeating effects or restoring grants.
- Failed native storage, expired resources and unconfirmed teardown withhold
  success/disclosure and retain uncertain journal identity. Unconfirmed teardown
  freezes authority. `/shell-results` and `/permissions` expose inspection paths.
- Explicit worker `beforeClose` drains admitted receipt work before Harness close;
  transport cleanup follows. No Harness monkeypatch or direct Pi JSONL writing.
- Protected pre-effect capacity is 128 operator invocations per Session;
  publication is below 28 KiB. Operator execution has a 30-second hard timeout.

The real restart fixture exposed two additional root causes, both repaired:
startup observers now receive the cached state and use stable interruption IDs,
and asset generation/source publication share a private process lock. A cold
second launcher cannot inspect a half-published source snapshot. The lock does
not cover interactive runtime execution.

## Guardrail mechanism, not a waiver

The old changed-file gate scanned committed changes only and filtered test-like
filenames before provenance analysis. It could miss staged, unstaged and new
production files. The final gate selects all these Git states, preserves NUL
path boundaries, and lets AST/Cargo provenance decide test scope. Symlinked or
escaping candidate source fails closed.

Cargo integration test roots are positively identified, including explicit
entries and disabled auto-discovery. Any production entry/edge wins over shared
test use. Real production files under `tests/` or ending `_test.rs` are not
blanket-exempt. A disposable Git repository runs the actual gate and proves an
untracked, test-named production canary fails.

This found two additional genuine production `expect`s in sandbox configuration:
platform construction now selects concrete builtins, with a named-profile
parity test; schema serialization returns `Result` with context and its callers
propagate/explicitly handle failure. Earlier genuine runner suppressions were
removed rather than waived. One additional test-only collapsible conditional
was corrected without changing its native identity assertion.

## Final evidence

All commands used one CPU, nice 19, bounded timeouts and one Cargo build job.
Provider, owner and process-kill fixtures are disposable; no real provider calls
or adopted/live process kills are inferred.

| Gate | Result | Log |
|---|---|---|
| `just test-oqt-core` | pass: permissions/sandbox/schema, strict scoped Clippy, 13 input/wire checks, 2 process-lock regressions | `/tmp/oqt-final-core.log` |
| `just test-oqt-hands PIN ENV BPF` | 44 pass, including native SQLite and composed receipt failures, privacy, active inference, capacity and recovery | `/tmp/oqt-final-hands-all.log` |
| `just test-oqt-bang` | pass: concurrent TUI clients, reopen, actual captured provider request context, private-byte exclusion, no inference until explicit prompt | `/tmp/oqt-final-bang.log` |
| `just test-oqt-operator-recovery` | pass: host SIGKILL during effect stops descendants, restores uncertain identity and never replays; completed native receipt survives another host kill/reopen | `/tmp/oqt-final-operator-kill.log` |
| prepared runtime typecheck | clean | `/tmp/oqt-final-receipt-typecheck.log` |
| touched TS/MJS Biome | 10 files clean | `/tmp/oqt-final-biome.log` |
| `just lint-rust-ai-guardrails` | 110 selected Rust files, TOTAL 0; 10 mechanism regressions including real-gate negative canary | `/tmp/oqt-final-guardrails.log` |
| runner locked/offline all-target Clippy `-D warnings` | pass | `/tmp/oqt-runner-final-clippy.log` |
| runner library tests, disposable HOME/XDG, serial | 182 pass, 2 ignored | `/tmp/oqt-runner-final-tests.log` |
| workspace Rust format check; `git diff --check` | pass | `/tmp/oqt-final-rustfmt.log` |

## Explicit limits / retained debt

- This closes local source safety implementation, not release or deployment
  acceptance, Account/MFA transport, hosted translation, mobile/device adapters,
  power-loss durability or cross-machine handoff. Process-crash SQLite proof is
  not storage/power-failure proof. External exactly-once is not promised.
- Classic import/context parity remains `oqto-g7rk.3`; extension ports, forks,
  workspace/provider proposals and the unreproduced Herdr input report remain
  separate. No claim that the latter input issue is fixed.
- Two privileged sandbox namespace tests remain ignored (CAP_NET_ADMIN/root).
  Runner's ignored tests are a fixture subprocess (explicitly exercised by its
  parent) and the separately provisioned real Pi Selection acceptance case.
- Cargo still reports third-party `proc-macro-error2 2.0.1` future incompatibility,
  tracked as accepted bounded current-toolchain dependency debt in `oqto-aavh`.
  This patch changes no dependencies, introduces no warning suppression and
  claims no future Rust compatibility. Owned production compiler/Clippy gates
  are clean; deliberately upgrading unrelated dependencies is not this patch.
- The working tree contains substantial other-owner changes. These receipts are
  not blanket acceptance of that work or permission to commit/deploy it.
