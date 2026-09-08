# macOS runner launch proof

Tracking: `oqto-cqfq`; parent workflow: `oqto-zb0r`.
Target: `ssh mac`, arm64 macOS 26.6.2 (25G83), Rust 1.97.0, installed Pi 0.85.0, tmux 3.6b.

## Delivered boundary

`oqto-sandbox::build_sandbox_command` selects bwrap on Linux or Seatbelt on macOS. Both the standalone CLI and runner generic/Pi spawn paths call it. Filesystem permissions come from the existing ADR-0028 resolver, including home allowlists and extra read/write grants. macOS no longer runs the separate legacy compiler that ignored these fields.

Seatbelt Unix-socket connection rules use that same resolved policy: open TCP does not grant every Unix control socket. Runner Pi launch also explicitly denies its real upstream SSH-agent socket. TCP destination/domain policy and SSH destination grants are not implemented on this Mac backend and are rejected, not treated as selected-key grants.

The launcher grants `file-ioctl` only for its inherited terminal when the caller explicitly selects `SandboxStdin::Inherited` and stdin is a TTY. RPC/generic runner children select `Redirected`, even if the runner itself has a terminal; an openpty regression proves they do not borrow its ioctl grant. This fixes the reproduced Pi `setRawMode EPERM` failure without allowing ioctl on every device. RPC launches receive no terminal grant. `sandbox-exec -p` avoids asynchronous temp-profile lifetime races. Egress guard ownership and pre-exec resource limits remain with their existing callers.

Linux-specific namespace/kernel enforcement, overlays and scoped materialisation are rejected on macOS rather than silently dropped. Consequently the Linux-oriented built-in profiles are not directly usable on Mac. `backend/crates/oqto/examples/sandbox.template.macos-host.toml` is an explicit starting policy: workdir writable, home allowlisted, Homebrew readable, no automatic Pi/SSH/home/tmp write grants. Add harness-state permissions deliberately. Denied directory names are still observable; Seatbelt is not a hostile multi-tenant boundary.

Other fixes found by native compilation: runner Path/PathBuf imports were Linux-gated, and the seccomp generator linked libseccomp on every OS. The checked-in sandbox schema also lacked the already-existing `/run/oqto` default denial; it was regenerated, with no new schema fields.

## High-risk checklist and evidence

- **Identity:** no protocol or public Session identity change. Runner integration test checks that a caller-provided routing ID survives PiManager launch. The mock harness does not claim Pi-native identity/history convergence.
- **History:** no production history changes; tests use the existing runner API/stores in a disposable HOME. Native TUI test uses Pi `--no-session`; no agent-written Pi JSONL.
- **Policy:** shared resolver; positive and negative file probes, read-only grants, implicit/explicit denied sockets, symlink/quoted paths, open vs isolated networking. Failed positive probes fail the suite.
- **Execution:** actual macOS runner subprocess, Unix RunnerClient, generic spawn and PiManager spawn with fixture harness. No shell interpolation of payload arguments in the production builder.
- **Terminal:** real installed Pi inside a dedicated tmux server, no extensions, no context files, no real config, no model credentials; native `!!` command writes inside workdir and fails a denied read.
- **SSH:** fresh fixture ssh-agent and two disposable keys. Actual proxy and sandbox binaries: only granted identity listed; granted signing succeeds; ungranted signing, upstream socket access and private-key reads fail. No external SSH host connection or keychain claim.
- **Lifecycle:** test runner stopped via SIGTERM and waited; tmux/SSH fixtures clean only their own processes and directories. No live service replacement or enrollment.
- **Deployment/config:** user's Mac repo, ~/.pi, SSH keys and launchd services untouched. Builds/artifacts/logs are staged under `~/byteowlz/oqto-mac-proof/`, not installed into PATH.

## Reproduction

Native Rust gates:

```sh
# This Mac has broken rustup proxy symlink dispatch for rustdoc/clippy-driver.
# Use the actual installed toolchain bin for this command, not a user config edit.
export PATH="$(dirname "$(rustup which rustc)"):/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
cd backend
cargo fmt -p oqto-sandbox -p oqto-runner --check
cargo test --locked -p oqto-sandbox -p oqto-runner
cargo clippy --locked -p oqto-sandbox -p oqto-runner --all-targets -- -D warnings
cargo build --locked -p oqto --bin oqto-sandbox --bin oqto-ssh-proxy
cargo build --locked -p oqto-runner --bin oqto-runner
cd ..
bash scripts/sandbox/tests/macos-ssh.sh
bash scripts/sandbox/tests/macos-pi-tui.sh
```

Pi TUI proof defaults to the Mac's Bun-installed Pi; override `PI_BIN` and `PI_PACKAGE_ROOT` for another installation. It uses Homebrew tmux and Node. It does not load `pi-tui-rpc` or test the rich frontend.

Evidence logs on Mac: `all-tests.log`, `clippy.log`, `runner-live.log`, `macos-live.log`, `tools-build.log`, `ssh-live.log`, `pi-tui-live.log` below `~/byteowlz/oqto-mac-proof/`. Native tests: 259 passed in aggregate, plus four explicitly ignored tests (two existing manual tests and two child helpers invoked explicitly by their parent tests). Some existing Linux containment tests report a non-Linux skip as a pass; the new macOS launch tests and standalone SSH/TUI scripts are the enforcement evidence, not those skip counts.

Linux: 273 tests passed with only the separately failing live-kernel egress test excluded (two existing manual tests ignored). This includes runner lib 94, sandbox lib 147, containment 16, and repaired port-exposure/socket-activation fixtures. Those transport fixtures now explicitly disable sandboxing and never recover a production runner; containment remains covered by the separate explicit-policy tests (`oqto-djkg`). Clippy all-targets passes warning-free against an isolated source snapshot. Full sandbox run is blocked by `egress_live`: the running kernel lacks usable nft NAT/conntrack expressions (`dnat`, `ct state` report ENOENT); `nft_nat` is not loaded and `modinfo` cannot find modules for running kernel 7.1.2-arch3-1. No host kernel/module mutation was performed. Tracked as blocked `oqto-16g0`; this is not a Mac policy fallback. Missing `macos-seatbelt` feature also has a native fail-closed regression test, which passed.

Source isolation: concurrent unrelated app edits temporarily broke whole-backend compilation. Native proof was reset to tracked `f72319704b9b626abcbe4534bf844d2a7b4f1b38` plus the owned change list. After git archive, source mtimes must be refreshed (or use a new target directory): restoring older mtimes can leave Cargo using stale artifacts. Linux proof uses the same isolated snapshot and `-j 1`; unrestricted parallel linking exhausted this host's available memory/I/O and timed out.

Guardrail debt is explicit: the branch-wide `just lint-rust-ai-guardrails` reports existing findings outside this change. Focused production scan reports two unchanged findings in touched files: schema serialization's `expect`, and the documented `result_large_err` allowance in stdout subscription. They are not new warnings (Clippy is clean); rewriting unrelated schema/subscription contracts was kept outside the Mac launch patch. This is a development checkpoint, not a clean release-gate or deployment claim.

## Not delivered

- `oq` executable, terminal-host provisioning adapter, runner `pi-tui-rpc` transport, native-turn attribution or snapshot/replay convergence.
- One central control plane connected to this Mac, runner enrollment/launchd installer, sleep/wake reconnection proof.
- Discovery/reconnection to the user's real SSH agent or 1Password/Apple Keychain. SSH session inherited no `SSH_AUTH_SOCK`; the integration uses explicit fixture sockets only.
- Destination-restricted SSH, Mac domain-filtered egress, container/multi-tenant isolation, temporary permission grants.

The next slice remains one real Pi process attached through the runner to both TUI and rich frontend, with existing public-ID authority and oqto-log persistence. Do not substitute a standalone sandbox shell wrapper and call it `oq` before those contracts exist.
