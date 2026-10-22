# Sandbox effective-policy inspection

> **Authority limitation:** `--config` replaces the trusted config lookup chain;
> it does not narrow admin/parent policy. These inspection flags describe that
> replacement and do not make generated policy safe for extension delegation.
> Extension integration is blocked until independently trusted policy composition
> and compiler-level non-widening are implemented and qualified (`oqto-gss4`).

`oqto-sandbox --config /private/generated.toml --workspace /staging --print-effective`
prints one JSON document on stdout, with diagnostics on stderr. No payload is
required or executed, even if supplied after `--`. `--check-config` runs the same
inspection but emits no JSON. Both reject disabled sandboxing, unknown profiles,
invalid workspace policy and compiler errors. TOML remains the policy language.

Rust callers use `oqto_sandbox::effective::inspect_effective_policy` with an
already resolved/merged `SandboxConfig`, workspace and cwd. Inspection uses the
same command builder, path resolver and bwrap compiler as launch. CLI workspace
merging uses the strict loader rather than ignoring malformed policy.

## Version 1 contract

- `schema`: `oqto.sandbox.effective.v1`.
- `profile`, canonical `workspace`, canonical `cwd`.
- `allow_write`: **conservative persistent write reach**, collected from compiled
  writable binds, including both host source and sandbox destination aliases
  (excluding `/dev/null` file masks, which do not grant persistent file writes).
  Includes implicit workspace grants and scoped grants, not merely TOML entries.
  Read-only overlays/masks may narrow it. A staging-only subset check can reject
  safe configurations conservatively, but must never omit a broad bind grant.
- `deny_read`, `deny_write`: resolved filesystem-policy boundaries. These are
  **not independent absolute denials**: descendant exceptions and precedence
  live in `filesystem_policy`. Presence in a deny list alone is insufficient to
  prove that a subtree is inaccessible.
- `filesystem_policy`: resolved rules with provenance and defaults. Paths are
  canonical/sandbox-visible; a lexical home path may resolve through a symlink.
- `compiled_arguments`: actual compiler output (dummy `/bin/true` payload),
  including mount order, masks, ephemeral mounts, scoped grants and shim options.
- `resolved_config`: complete resolved settings, including Landlock/seccomp modes.
- `network_mode`, `backend_capabilities`: configured network mode and backend
  expressiveness. Capabilities are not a successful kernel-enforcement probe.
- `enforcement_verified`: always false. Inspection does not install Landlock,
  enter namespaces, prepare egress, or launch a sandbox. Launch must still fail
  on enforcement errors. Host/filesystem changes after inspection remain a race;
  this report is neither an approval token nor an authenticated policy snapshot.

Currently only Linux/bwrap reporting is supported. Other backends fail closed
rather than return an incomplete effective-write report. `--cwd` is separate
from workspace authority and cannot widen grants.

## Restriction-only overlays

Linux/bwrap also accepts:

```
oqto-sandbox --overlay /private/narrow.toml --workspace /staging --print-effective --json
oqto-sandbox --overlay /private/narrow.toml --workspace /staging -- /path/to/agent
```

Example restriction TOML:

```toml
home_access = "none"
allow_write = []
deny_read = ["~/.pi/agent"]

[network]
mode = "isolated"
```

This loads the normal config chain, then strict workspace policy, then overlay
restrictions. It cannot be combined with replacement `--config`, `--profile` or
`--no-sandbox`. Only deny lists, retained explicit writes, lower home access and
isolated networking are accepted; bind additions and unknown keys are errors.
An empty write list does not remove implicit workspace grants or existing scoped
and bind grants. Inspect remaining compiled reach rather than assuming staging-only.
Private policy must be outside workspace; no final symlink and at most 64 KiB.
The CLI snapshots it once. Separate check and launch invocations are not bound.
Do not change the launcher's `HOME`/`XDG_CONFIG_HOME` before policy resolution:
that changes home-path expansion and possibly the user config source. Set private
agent-home variables on the payload instead, e.g. after `-- /usr/bin/env HOME=...`.
An environment variable is not evidence of an authenticated account identity.
See [ADR-0070](../adr/0070-restriction-only-sandbox-overlays.md) for scope and limits.

## Extension integration

Do not hand an untrusted extension generated `--config` as a delegation primitive.
A private file and successful validation do not preserve the parent/admin ceiling.
The trusted launcher must independently obtain parent-session policy, apply
generated TOML solely as restrictions, and bind that composition to launch.
The new CLI overlay is a local restriction primitive, not that authenticated
parent-session launch protocol. Herdr/runner subagent integration is not enabled.
After that authority boundary exists, generated files must still be private and
outside child-writable paths; fail on unknown schema or inspection error.
For staging-only checks, compare every conservative `allow_write` root against
staging, not the original TOML list. To prove `~/.pi/agent` is denied, evaluate
resolved policy **and compiled overlays/aliases**; do not grep a deny list.
Nested parent/child comparisons must account for reads, writes, exceptions,
network, ephemeral resources and enforcement modes, not just list inclusion.
No extension-side comparator or cryptographic check-to-launch binding is added
by this change.

## Verification

```
cd backend
CARGO_BUILD_JOBS=1 cargo test -p oqto-sandbox --no-default-features
CARGO_BUILD_JOBS=1 cargo clippy -p oqto-sandbox --all-targets --no-default-features -- -D warnings
CARGO_BUILD_JOBS=1 cargo build -p oqto --bin oqto-sandbox
cd ..
python3 backend/crates/oqto-sandbox/tests/effective_cli_test.py
```

The CLI tests use disposable private config/staging directories and prove JSON
parsing, empty check stdout, no supplied-payload execution, unknown/disabled
config rejection and malformed workspace fail-closed behavior. Library tests
cover implicit write grants and cwd non-widening. No installed binary or user
configuration is changed.

Local overlay qualification: default and no-default-features crate suites passed
(210 tests each, 2 explicitly ignored); five real CLI tests passed, including the
independent disposable admin-config lookup test. Eight overlay tests include real
nested bind/read/write and network probes. Crate all-target Clippy with
`-D warnings` passed; Rust AI guardrails reported zero findings across 25 changed
Rust files. Herdr/runner parent-bound launch integration remains `oqto-v8n2.1`;
these probes are not proof of that unimplemented integration. The application binary build passed with Cargo's dependency
future-incompatibility advisory for `proc-macro-error2 2.0.1`. This is accepted
external dependency debt for this change, not a suppressed source warning;
upgrading that dependency is outside the reporting contract.
