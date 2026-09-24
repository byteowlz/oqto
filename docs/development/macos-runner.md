# macOS runner launch proof

Tracking: `oqto-cqfq`; parent workflow: `oqto-zb0r`.
Target: `ssh mac`, arm64 macOS 26.6.2 (25G83), Rust 1.97.0, installed Pi 0.85.0, tmux 3.6b.

## Delivered boundary

`oqto-sandbox::build_sandbox_command` selects bwrap on Linux or Seatbelt on macOS. Both the standalone CLI and runner generic/Pi spawn paths call it. Filesystem permissions come from the existing ADR-0028 resolver, including home allowlists and extra read/write grants. macOS no longer runs the separate legacy compiler that ignored these fields.

Seatbelt Unix-socket connection rules use that same resolved policy: open TCP does not grant every Unix control socket. Runner Pi launch also explicitly denies its real upstream SSH-agent socket. TCP destination/domain policy and SSH destination grants are not implemented on this Mac backend and are rejected, not treated as selected-key grants.

The launcher grants `file-ioctl` only for its inherited terminal when the caller explicitly selects `SandboxStdin::Inherited` and stdin is a TTY. RPC/generic runner children select `Redirected`, even if the runner itself has a terminal; an openpty regression proves they do not borrow its ioctl grant. This fixes the reproduced Pi `setRawMode EPERM` failure without allowing ioctl on every device. RPC launches receive no terminal grant. `sandbox-exec -p` avoids asynchronous temp-profile lifetime races. Egress guard ownership and pre-exec resource limits remain with their existing callers.

Linux-specific namespace/kernel enforcement, overlays and scoped materialisation are rejected on macOS rather than silently dropped. The shipped `development`/`strict` profiles ask for pid/userns isolation and `no_new_privs`, which Seatbelt cannot enforce, so they are not usable on Mac. A shipped `development-macos` profile is now the macOS default: it keeps `development`'s path allowlist/denylist and workspace scoping (which Seatbelt enforces) and drops only the unenforceable namespace guarantees. For strict, untrusted, or allowlist-style isolation `backend/crates/oqto/examples/sandbox.template.macos-host.toml` is an explicit starting policy: workdir writable, home allowlisted, Homebrew readable, no automatic Pi/SSH/home/tmp write grants. Add harness-state permissions deliberately. Denied directory names are still observable; Seatbelt is not a hostile multi-tenant boundary.

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
- Remote chat/session admission, full audited runner enrollment/rotation, a general launchd installer, and physical sleep/wake proof. Read-only authenticated connectivity is now delivered below.
- Discovery/reconnection to the user's real SSH agent or 1Password/Apple Keychain. SSH session inherited no `SSH_AUTH_SOCK`; the integration uses explicit fixture sockets only.
- Destination-restricted SSH, Mac domain-filtered egress, container/multi-tenant isolation, temporary permission grants.

The next slice remains one real Pi process attached through the runner to both TUI and rich frontend, with existing public-ID authority and oqto-log persistence. Do not substitute a standalone sandbox shell wrapper and call it `oq` before those contracts exist.

## Connected Mac inventory (2026-09-08)

[ADR-0047](../adr/0047-runner-inventory-is-not-session-placement.md) separates machine inventory from execution admission. The central backend can probe an explicitly Account-authorized Mac runner and expose it through `GET /api/runner-targets`. The collapsible sidebar roster reports connectivity; it does **not** offer remote chat creation or move existing Sessions.

Operator setup checklist:

1. Use a dedicated state root, synthetic HOME, explicit native Mac sandbox config, and an absolute Pi executable. The synthetic `HOME/.config/oqto/config.toml` must set `[local] single_user = true`, disable Linux users, and set `[runner] runner_id = "mac"`. Missing single-user configuration correctly fails closed on Mac; it is not a reason to disable sandboxing. Do not copy existing user Pi settings/JSONL or inherit the real SSH agent.
2. Generate a dedicated CA and separate server/client leaf certificates with serverAuth/clientAuth EKUs. Require the server SAN `oqto-mac.runner`; store Linux private material under `~/.local/share/oqto/credentials/runner-targets/mac` (already denied by built-in sandbox profiles), mode 0700/0600. Copy only the server key/certificate and public CA to the Mac, and explicitly deny its credential/control/config directories in the Mac sandbox. A 0700 directory alone does not isolate same-UID agents.
3. Start a dedicated launchd agent with `oqto-runner --listen-tls 127.0.0.1:39443 --tls-cert ... --tls-key ... --tls-client-ca ... --sandbox-config ...`. Set HOME/XDG_CONFIG_HOME/XDG_RUNTIME_DIR/TMPDIR explicitly. A dedicated working directory and logs prevent existing services, worktrees and user settings from being replaced.
4. Supervise `ssh -N -T -o BatchMode=yes -o StrictHostKeyChecking=yes -o ExitOnForwardFailure=yes -o ForwardAgent=no -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -L 127.0.0.1:39443:127.0.0.1:39443 mac` in a separate Linux user service. SSH transports TLS; it does not replace certificate or Account authorization. Both listeners are loopback-only.
5. Create endpoint JSON containing `transport: "tcp_tls"`, `address: "127.0.0.1:39443"`, `server_name: "oqto-mac.runner"`, and absolute `ca`, `certificate`, `key` paths. Run `python3 scripts/dev/register_runner_target.py --config "$HOME/.config/oqto/config.toml" --id mac --label Mac --account-id <Account-ID> --endpoint <endpoint.json> --dry-run`, review the addition, then omit `--dry-run`. The helper preserves original bytes, takes a private backup, validates the TOML round-trip, refuses conflicting IDs and checks for concurrent edits. Never hand-edit placement stores.
6. Activate the tested backend using a reversible development service override rather than replacing an immutable release. Verify manifest structure, package version, tests, TLS rejection, Account filtering, unchanged placement registry and UI status. Keep the healthcheck startup-grace requirement below in mind.

**Proposed network Files grant (not deployed):** [ADR-0051](../adr/0051-runner-owned-network-filesystem-grants.md) makes this TLS endpoint inventory-only unless the dedicated single-user runner's **supervisor** passes `--remote-file-root /canonical/absolute/work-root` (repeatable) plus `--remote-client-cert-sha256 <64-hex-digits>` (one or more supervisor-owned client leaf pins; `openssl x509 -in client.pem -outform DER | shasum -a 256`). Never place this grant in the agent-editable `config.toml`. It bounds and confines mediated Files operations; these flags alone do not authorize Pi/process commands. A separate `--remote-full-principal-pi` requires a `/` root and explicitly permits Pi command control as the dedicated single user's OS principal (generic process/App commands remain denied). This is not narrow-root agent confinement or client-specific enrollment. A root containing the real home or `/` includes secrets—there is no implicit "minus SSH keys" carve-out. The isolated native and negative probes passed with a disposable test process; keep the live Mac service/config unchanged. Provision only through a backup + diff + merge of its launchd definition and restart for revocation.

Current development installation:

- Mac state: `~/.local/share/oqto/mac-runner`; launchd label `dev.oqto.mac-runner`, plist `~/Library/LaunchAgents/dev.oqto.mac-runner.plist`. This uses the already-proved staged native 0.5.0 runner and Pi 0.85.0, with no model credentials or original Pi-session writes.
- Linux SSH service: `oqto-mac-tunnel.service`; Account grant: `wismut` only; target ID: `mac`.
- Backend override: `~/.config/systemd/user/oqto.service.d/40-mac-runner-targets.conf`, referring to the separately installed root-owned `/usr/local/lib/oqto/dev/mac-runner-targets-20260908/oqto`. `/usr/local/bin/oqto` and the immutable release remain unchanged. This is a development activation, not a packaged release.
- Leaf certificates expire after 90 days; CA after 365 days. No automatic renewal claim. Remove or rotate trust deliberately before expiry.

Proof: mutual-TLS capability request returned wire version 1 and Pi support. Missing client certificate and wrong server identity were rejected. A sandboxed native shell wrote the dedicated workspace proof file while reading the TLS private key failed. The authenticated backend returns Mac online with `session_creation: false`; an unauthenticated request returns 401. Account isolation/no secret projection, protocol mismatch, UI error/cache behavior and preserving registration updates have regressions. Desktop and 390px mobile browser checks show the Mac roster. Stopping only the tunnel changed the UI to Offline; restoring it returned Online without reload. All 80 sampled pre-existing Session IDs remained present, and the placement registry SHA-256 was unchanged. Screenshots are under `/tmp/oq-mac-connect-deploy/mac-{online,offline,reconnected,mobile}.png`.

Gates: backend binary tests 287 passed/one existing ignored; all-target Clippy and fmt clean; frontend focused tests 19 passed, OqtoUI typecheck and OqtoUI guardrails pass; four Python registration tests pass; dist manifest validates 32 assets. Focused production Rust scan has zero findings. Branch-wide Rust (11 existing findings) and legacy useEffect (two unchanged renderer findings) gates remain red under `oqto-msvq`; this is explicitly outside this additive inventory slice, not silently waived or described as a green release gate.

Startup safeguard (`oqto-n956`): the existing 30-second HTTP health timer repeatedly restarted the backend during its synchronous target backfill (5,762 records in 56 seconds). Setup now generates a bounded 120-second `ExecCondition` grace period using the service's monotonic activation timestamp. It suppresses checks only for a recently active backend; expired grace, inactive service, missing/malformed evidence or a future timestamp allow the original health/repair command. Twenty user/system boundary cases pass. This host uses the additive `~/.config/systemd/user/oqto-healthcheck.service.d/40-startup-grace.conf`, preserving the original healthcheck command. A real backend restart completed with an unchanged PID while the timer stayed enabled; the Mac remained online. No session backfill bypass or permanent monitoring disablement.

Rollback: disable/stop only `oqto-mac-tunnel.service`; boot out only `gui/501/dev.oqto.mac-runner` on Mac; remove only the new backend drop-in and reload/restart that service. Remove only the exact added target stanza (or restore its backup **only if no later config edits exist**). Keep dedicated state and credentials until separately authorized cleanup. Never stop the personal Linux runner, rewrite placement/session stores, or replace a user's complete config as rollback.
