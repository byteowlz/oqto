# oqto-sandbox

## Responsibility

Sandbox policy types and sandbox wrapper binary for restricting agent processes.

## Non-goals

No session orchestration, no user provisioning, and no backend route handlers.

## Depends on

Low-level process/config/logging crates needed to apply sandbox policies.

## Used by

`oqto-host`, `oqto`, deploy/runtime flows, and the `oqto-sandbox` binary.

## Migration notes

Sandbox configuration is security-sensitive. Per-workspace overrides may add restrictions but must not silently weaken host-level policy.

## Native macOS launch

The runner and CLI share `build_sandbox_command`: bwrap on Linux, Seatbelt on macOS. Both use the ADR-0028 permission resolver. macOS denies unsupported namespace, overlay, scoped-materialisation and destination-filter requirements instead of silently weakening a Linux profile.

Start from `../oqto/examples/sandbox.template.macos-host.toml`, not a Linux built-in profile. It grants workdir writes and Homebrew reads, not existing Pi/SSH state. Add deliberate harness-state grants and set `TMPDIR` inside a writable workdir. File access and Unix-socket access follow policy; denied directory names are not hidden. Native TUI receives ioctl permission for its inherited terminal only.

See [Mac proof, commands and remaining gaps](../../../docs/development/macos-runner.md). This is native single-user isolation, not container/multi-tenant parity.
