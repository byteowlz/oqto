# Native Pi TUI observation qualification

Tracks `oqto-8x3p.10.4`, based on published runner `56c07e7b329af80d6e70792ffd25bb0092438a79`. This qualifies the existing **observation transport** on macOS; it does not implement network admission, canonical ingestion, single-writer continuation, MFA or mobile control. No production Rust API/wire behavior changes here.

## Reproduce

Requires installed Pi 1.1.0, uv/Python 3.12, Rust and the explicitly selected existing `pi-agent-extensions/pi-tui-rpc/index.ts`. No package, extension, service or operator configuration is installed/modified. Older Pi versions lacking the isolation flags must fail rather than silently dropping those flags.

```bash
# Use one installed compiler consistently for Cargo, Clippy and their children.
export RUSTUP_TOOLCHAIN=1.97.0
just test-pi-tui-rpc
just test-pi-tui-rpc-launcher
just test-pi-tui-rpc-native /path/to/pi-agent-extensions/pi-tui-rpc/index.ts
```

The opt-in native Rust test is ignored by default; its result is **separate** from the 13 synthetic tests. `test-pi-tui-rpc-native` runs two launcher safety units, creates a new private short `/tmp` fixture, starts one real Pi TUI on an owned PTY, and calls the exact ignored test with its fixture manifest. The Rust test uses the unchanged `TuiObserver`, not another socket client. It admits the explicit synthetic Account/public/native binding, observes idle state, proves a deadline poisons only its own socket, explicitly reattaches, and compares source bytes. The launcher verifies that Pi is still alive, source bytes are unchanged, and its local trap received zero model requests, then terminates/reaps only its direct child.

## Isolation and limits

- Fresh HOME, Pi profile, work/session/socket directories and XDG roots; child environment is an allowlist, not inherited provider credentials. Tool, MCP, extension/skill/template/theme/context discovery, startup network and telemetry are disabled. Only the selected existing RPC extension loads. Terminal lease confirmation remains required; the observer sends only `get_state`, never lease/prompt/abort.
- The guardrail scanner previously missed Rust's inner `#![cfg(test)]` module attribute and treated test helpers as production. The fixture now explicitly declares that true compile-time scope; the scanner recognizes inner and outer attributes. Five regression tests cover both scopes, production preservation, missing files and unexpected read errors (the inner-attribute case failed before the fix). This is not a blanket lint acknowledgement; production remains checked. The touched Python reporter also uses current annotations and specific I/O/subprocess exceptions instead of swallowing arbitrary bugs.
- Pi creates brand-new empty session files lazily. The initial no-history attempt timed out and was reaped safely. The repeatable fixture therefore authors a small **synthetic seed before Pi starts**, using create-only writes, then leaves ownership to Pi. It never writes a real or already-owned Pi JSONL. Launcher units prove no operator credentials/profile enter the child and seed creation cannot overwrite an existing file.
- A loopback provider trap returns errors and counts requests; no prompt is submitted and no inference is performed. This is not an OS egress/sandbox proof. PTY capture is capped at 256KiB. Startup is bounded to 20s; observer test to 180s; owned-child shutdown to 5s before a last-resort kill of that child only.
- Native manifests, process/session identifiers, profile state, PTY captures and raw receipts stay in the private fixture directory. Do not commit them. The fixture authority is test-only, **not** a real Account grant issuer. No private profile, session or global socket scan occurs.
- Both socket layouts still require the same peer UID/PID, inode, protocol and exact identity checks. The existing fixture now uses a short private temporary root because macOS's normal per-user TMPDIR plus the uid/pid layout exceeds `sockaddr_un.sun_path`. No check was disabled or weakened.

## Observed proof (2026-10-16)

- Native Mac baseline: 11 synthetic passes, 2 `SUN_LEN` failures. Short fixture root: **13 pass**, 1 new opt-in test ignored by default. **Real disposable Pi TUI observation passes separately**, with terminal alive after observation, unchanged source, 0 model requests and owned process reaped. Two Python launcher safety units pass.
- `cargo check`, `cargo fmt --all --check`, strict all-target runner Clippy and Python Ruff checks pass in the consistent Rust 1.97.0 lane. An earlier unpinned Clippy invocation selected a different nested compiler and failed with incompatible rmeta; retained, then corrected by pinning, not by cleaning the shared Cargo cache. Existing `proc-macro-error2` future-compiler debt remains tracked by `oqto-aavh` as accepted at the source checkpoint; no future-Rust compatibility claim.
- **Full native runner suite is not green.** Exact unmodified baseline: 157 pass/8 fail/2 ignored. Candidate: 159 pass/6 fail/3 ignored. The two removed failures are precisely the socket-path fixtures; no additional failures or blanket skips were introduced. Five credential fixtures assume a supported storage backend (Mac reports Unavailable), and one enforced-app fixture requires Linux bwrap. Follow-up **`oqto-8x3p.10.5`** requires platform-appropriate positive/negative qualification without weakening credential or sandbox enforcement. macOS runner/Seatbelt support already exists; these failures do not mean otherwise.

Static review reports 0 preexisting-worse gates and 15 new-symbol findings: twelve callback/test/thread entry points the name-based graph cannot see, one 27-token clone between intentionally separate inner/outer scope regression cases, and verbosity on the fixture configuration and explicit process-cleanup functions. These remain visible; no acknowledgement/baseline suppression was added. Both changed existing function contracts are unchanged; the fixture constructor's 11 callers remain compatible. Nine source/tooling/recipe/documentation hashes match before/after the final run and must match the published commit. No production Rust API/protocol changed.

This proves real native macOS **same-Principal positive transport**, not real foreign-UID denial, hosted authorization, live canonical replay, control, oq/oqt support, phone installation or release. Parent `oqto-8x3p.10` remains open; `.10.2` and its lifecycle-review child retain their separate integration work. Shared primary branches, unrelated edits, tracker migration and running services were preserved.

See [attachment contract](pi-tui-rpc-attachment.md), [native Mac runner](macos-runner.md), and [read-only chat / background ingestion](../adr/0025-read-only-chat-reads-background-jsonl-ingest.md).
