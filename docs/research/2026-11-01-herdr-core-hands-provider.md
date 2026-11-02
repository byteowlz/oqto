# Herdr Core as an oqt/hands process provider

Assessment: architectural candidate, **not yet an execution/security qualification**.
No installation, machine enrollment, live terminal control or provider activation
was performed. Tracking: oqto-49bm; this is separate from media implementation.

## Evidence

- Product: <https://herdr.dev/core/>
- Current upstream inspected: `herdrdev/herdr-core` master,
  `e07c2a1bedd62cf4787ee026ee23ee79c7d95491`.
- [README](https://github.com/herdrdev/herdr-core/blob/e07c2a1bedd62cf4787ee026ee23ee79c7d95491/README.md)
- [Security/trust contract](https://github.com/herdrdev/herdr-core/blob/e07c2a1bedd62cf4787ee026ee23ee79c7d95491/docs/security.md)

The documented strengths are long-lived PTYs, holder processes that survive core
updates, attach/input/read/resize/wait, explicit parent/helper lifetimes, an OpenRPC
API and program-reported OSC 7501 status. These are deterministic mechanisms,
particularly useful for interactive hands, dev servers and terminal applications.

Its security contract explicitly says it runs with the account's access, is not a
sandbox or agent permission broker, and a raw API client can execute commands,
read/change files and control terminals. Unix `HC_SESSION` separation is not a
security boundary; there is no per-client permission check. SSH machine routing
inherits the SSH account's trust, not Oqto's destination grants.

## Fit

Use Core as an optional **Runner-owned PTY/lifecycle adapter**, below the existing
Runner/Guard authority. It is not a replacement for hands, the remote Runner
protocol, destination consent, sandboxing or durable Session history.

- Runner authenticates/authorizes, records work and owns opaque job/terminal handles.
- Core manages only admitted processes/PTYs through that adapter.
- Existing bwrap/seccomp or separately qualified Seatbelt enforcement constrains
  the actual command. Never fall back to an unrestricted shell.
- The confined brain gets semantic job operations, not `hc`, its socket, raw
  terminal IDs, machine enrollment or Core-wide filesystem/command capabilities.
- Oqto-log remains durable conversation authority. Core screens and status are
  display/process state, not messages, grants or proof of successful work.
- Keep authenticated remote execution through destination Runner admission. Do not
  introduce `hc machine` SSH execution as a competing remote-hands bypass.

It can also keep an operator's complete oqt terminal alive as a convenience, but
that alone does not confine either its brain or hands. One-shot bounded hands
already work without Core; adopting it is most compelling for persistent,
interactive execution rather than adding a mandatory daemon to every command.

## Qualification before implementation/default selection

1. Pin a reviewed release/source and verify packaging/provenance. Do not run the
   website's download-and-execute installer as part of evaluating it.
2. Prove the spawn contract can enter the approved sandbox without agent-controlled
   shell/template interpolation, startup profiles or unsanitized ambient secrets
   executing before confinement. Verify no API/socket descriptors leak to a child.
3. Restrict operations to Runner-owned job handles; refuse another terminal's
   IDs, stale handles, cross-Session calls and direct Core socket access from the
   confined workload. A separate HC session alone is insufficient.
4. Test attach/reconnect, parent/helper lifetime and Core/Runner restart without
   duplicate execution or accidental termination of unrelated operator terminals.
5. Prove destination revocation, expiry and cancellation stop the entire admitted
   workload, even if Core is restarting/unavailable. Holder survival cannot extend
   a grant: use enforceable job containment/deadlines independent of Core UI state.
6. Treat OSC status as untrusted program-reported information. It cannot grant
   authority or replace exit status, a result receipt or durable history evidence.
7. Repeat isolation/lease tests on Linux and macOS separately. Cross-platform PTY
   support does not establish cross-platform sandbox enforcement.

Conclusion: **yes as a plausible process substrate; no as an already-qualified
hands security boundary or an immediate mandatory replacement**. Media delivery
should proceed independently of any decision to adopt Core.
