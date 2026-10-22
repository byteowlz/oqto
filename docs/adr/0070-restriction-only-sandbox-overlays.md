# Restriction-only sandbox overlays

## Status

Proposed. Linux/bwrap CLI/library primitive and local confinement tests implemented
under `oqto-v8n2`. Not an authenticated parent-session launch protocol, and not
integrated with Herdr or enabled in oqt. Builds are source-local, not deployed.
Refines ADR-0060 and ADR-0069 without replacing their authority contracts.

## Context

`--config` replaces the normal config lookup chain. Private generated TOML does
not make replacement policy a safe delegation interface. TS-side effective-policy
comparison cannot authorize a launch in a separate supervisor/Herdr process.

## Decision

Add `--overlay <private.toml>` as a strictly restricted input, mutually exclusive
with `--config`, `--profile` and `--no-sandbox`. Load the existing normal chain and
strict workspace restrictions first. Capture overlay bytes once, apply restrictions
and pass the resulting typed config to the existing report/launch compiler.
`--print-effective --json` is an explicit spelling of the JSON report presentation.

Accepted overlay keys are only:

- `deny_read`, `deny_write`: add restrictions, never delete parent denials.
- `allow_write`: optional exact retained subset of parent explicit entries. `[]`
  removes explicit entries, not implicit workspace/home or scoped/bind grants.
- `home_access`: may only lower the parent's setting.
- `[network] mode = "isolated"`: may isolate, not open or replace proxy rules.

Unknown keys, profile replacement, bind additions, environment changes, enforcement
weakening and unsupported backends fail closed. There are no `--grant-*` flags.
The parent config remains responsible for all baseline grants.

Validate resolved path-policy containment at every decision breakpoint in both
finite rule trees. Reject descendant read exceptions beneath deny boundaries and
compiled bind aliases conflicting with read/write denials. This deliberately
rejects some valid layered configurations rather than silently ignoring restrictions.
It is not a general policy-equivalence solver. Scoped grants are not removed merely
by supplying `allow_write = []`; the effective report exposes remaining write reach.

Overlay capture requires a regular file, at most 64 KiB, without final symlink,
private Unix mode, outside the canonical workspace grant. This is not proof that
all same-UID actors lack access: a trusted supervisor must keep authority files
outside every child-accessible grant. No file is reread to launch within the same
CLI invocation. A separate check invocation is not an approval or a check-to-launch
binding; launch recomputes and validates again.

## Parent authority and subagent integration

The CLI normal chain is not an authenticated parent-session identity. A caller
that can freely select another cwd/workspace, invoke replacement `--config`, or
reach an unconfined supervisor must not be granted extra authority by this feature.
For direct nested local subprocesses, existing outer kernel confinement remains
the ceiling. Real local tests prove a nested bind cannot recover a masked file or
write a read-only file. This is not a broad escape audit.

`pi-session-tools` currently starts subagents through Herdr (`tab create` then
`agent start`). That child does not necessarily inherit its caller's namespaces.
Do not label it isolated or switch it to this feature until the trusted Herdr/runner
launch path obtains the exact parent policy and authorizes workspace/placement,
rejects replacement-policy bypasses, binds policy to the child Session, and keeps
launch/adoption/recovery evidence. Remote and Docker placement remain unsupported.
The in-memory brain grant primitive in ADR-0069 does not supply OS launch authority.

## Verification

`tests/overlay.rs` exercises strict parsing, retained denials, write-addition
rejection, home/network narrowing, canonical policy resolution, private capture,
real read/write denial, a nested bwrap rebind attempt and host-loopback isolation.
`tests/effective_cli_test.py` uses an outer mount namespace with a disposable
admin config mounted at `/etc/oqto/sandbox.toml`: no host config is changed.
It proves the CLI loads that independent source, retains its protected Pi denial,
prints JSON and rejects replacement settings and bind additions.

## Consequences

Trusted local launchers now have a small restriction-only TOML primitive. This is
necessary but insufficient for Herdr subagent enablement. Durable audit events and
runner persistence remain tracked separately in `oqto-0rzz`. No extension may turn
an effective-policy dump into a self-issued authority token.
