# ADR-0069: One-command pairing over local owner control

Status: implemented source checkpoint; Mac isolated-source integration/activation remains blocked.
Workstream: `pair-wizard`, `oqto-8x3p.10.7.6`.
Related: [ADR-0062](0062-backendless-personal-host-device-pairing.md),
[ADR-0063](0063-native-session-graph-convergence.md).

## Context

Requiring manual launchd plist surgery and a second administrative invocation to
obtain a QR exposes internal composition as the product interface. Starting a
second Runner to add pairing risks duplicate managers/writers and wrong source
identity. A Device pairing ticket is not Account/MFA or a native input lease.

## Decision

`oqto-runner pair` is an ephemeral terminal wizard over an explicit shared plan.
Terminal stdin/stderr automatically select the wizard when inputs are incomplete;
flags provide the same action headlessly. Explicit `--interactive` requires a TTY;
`--non-interactive` requires complete scopes/network and `--yes`. Choices do not
mutate authority/configuration before final default-No confirmation. Full access
means session control, writable Workspace Files and an unrestricted OS-user shell;
that last scope is printed as a warning, never described as directory-confined.

Use dialoguer's standard transient selection primitives, not a full-screen TUI or
new interaction/theming framework. Workspaces/access/network are one-shot
selections, not persistent modes. Terminal-native styling and NO_COLOR apply.
The user-approved checklist is the interaction specification. Flags are the
explicit headless/agent fallback, not a second policy implementation.

Dedicated host-user Runners publish an owner-only management socket at
`~/.local/share/oqto/pairing/control.sock`, independent of personal HTTPS.
It is same-effective-UID authenticated and requires private owned directories,
600 sockets/files and an exclusive no-follow singleton lock. Both it and personal
authorization state are denied to sandboxed children. It is not added to network
Runner RPC or Device actions. Local status schema1 exposes PID, active state,
selected transport/Workspace plan and authority state location, never keys/tickets.
The bounded command codec rejects unknown fields and oversized/incomplete frames.

Enabling personal HTTPS composes the existing `run_personal_inner` on the same
Runner Arc, Pi manager and serialized ingest handle. Readiness acknowledges only
after actual listener, TLS identity, owner endpoint and backend initialization.
Management processing does not pause the primary transport accept loop. Scoped
tasks abort on cancellation/drop; owner socket cleanup is inode-fenced. The
primary transport is not stopped by an unsuccessful hot-enable request.

Only transport/source configuration is persisted in private `listener.json`, via
create-only atomic publication and sync. Grants/identities/tickets stay in the
existing PairingAuthority store (`pairing/host`), not this config file.
Unknown/manual identities/configurations are preserved, not silently migrated or
rotated. Workspaces cannot overlap the private admin store. Subsequent enable
requests must match the configured plan; there is no implicit scope widening for
existing Devices. Reconfiguration/migration is separate work, not hidden repair.

A missing normal-user Runner can be started using its existing dedicated-host
config without rewriting it, or a new known private minimal config if absent.
Bootstrap is serialized and refuses known old/ambiguous processes/socket layouts.
The new process uses its own private primary socket/log, is readiness/PID checked,
and is detached only after readiness. Startup cleanup targets only the newly
created child. The new transport survives CLI exit and is restored on Runner
restart; this does not install a login service.

CLI discovery can read the registered Mac service's HOME using plutil extraction,
without reading key values, modifying the plist or restarting a running service.
An old/stopped registered service without the new control endpoint is refused:
this checkpoint does not silently bootstrap or replace that service. An isolated
service HOME must not be substituted for the caller's native source. The current
source-binding mismatch fails before authority creation; `.10.7.5` must be reviewed
and integrated before claiming Studio acceptance. No HOME flipping, copying native
JSONL, spawning a second Runner or fabricated head/catalog fallback is allowed.

QR issuance is one call to the existing owner grant gate, after readiness. Only
the selected output stream receives it. No automatic retry follows uncertain
ticket dispatch. Pairing never confirms a terminal-owned Pi input lease.

## Consequences / verification

- Normal supported profiles gain a single command; no backend, GUI or plist edit
  is required for the pairing action. Existing mTLS authorization is unchanged.
- Older binaries need an explicit upgrade; unknown service/process layouts fail
  conservatively rather than guessing absence.
- Configuration replacement, login-service installation, changing the global
  Workspace set, multiple active-profile arbitration and isolated-source binding
  are not smuggled into ticket creation. These limitations remain visible.
- Proof includes same-PID loopback hot-enable, persistence/stable identity,
  grants/QR, no widen, occupied port/source mismatch refusal, private-path/lock
  checks, primary transport availability under a slow admin peer, headless errors,
  real PTY selection/cancellation and synthetic fresh CLI bootstrap. See
  [runbook](../development/runner-pairing-cli.md). These are not live Mac/phone,
  actual production process-census, native Pi or tailnet acceptance.
