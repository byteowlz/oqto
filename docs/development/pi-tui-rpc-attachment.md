# External Pi TUI attachment: runner adapter checkpoint

Tracks `oqto-8x3p.10.1` and `oqto-8x3p.10.2` under `oqto-8x3p.10` (M3/M4). This is a runner transport/control seam, not completed launcher/mobile/live adoption or an enabled network capability. The read-only checkpoint is `56c07e7b`; subsequent control work remains gated on host admission and integration.

## Source and ownership

Ported from the fetched `oqto-desktop` main revision `b788b2fd24638bac22a0355a6c34f53ea8edb6e5`, `crates/oqto-pi-client/src/{live,transport}.rs`, and Desktop ADR-0005. Protocol v1 is defined by the canonical `pi-agent-extensions/pi-tui-rpc` extension. This does not fork the extension or embed Desktop UI dependencies.

`oqto_runner::tui_rpc::TuiObserver` connects to either same-Principal socket layout: `pi-tui-rpc-<pid>.sock` or `pi-tui-rpc-<uid>-<pid>/control.sock`. Observation issues only `get_state`. Explicit control methods additionally support lease acquisition/release and typed prompt/steer/abort, never generic commands, bulk `get_messages`, spawn, process stop, or session-file mutation. Legacy sockets without `connection_leases_v1` remain observable but must not receive control.

The external terminal continues to own Pi. Unmanaged external Pi remains unverified/full access; attachment cannot retrofit confinement. Independent managed-launcher confinement evidence must be retained, not inferred from a command name or discarded merely because transport uses a socket. This module does not itself issue confinement attestations.

## Authority and identity checklist

- The embedding runner must implement `ObservationAuthority`. It resolves an authenticated Account/public Session to an unambiguous **existing durable binding**, designated work/session roots, and explicit observation grant generation **before** the adapter accesses files or sockets. Inventory, stored-history access and a local extension input lease are not observation grants.
- Binding data is not a deserializable client request. The adapter validates public/native distinction, selected canonical roots, the Pi file header, socket owner/mode, kernel peer UID (and PID where the platform provides it), socket inode stability, v1 hello, correlated response ID, native Session ID, exact session file and work directory.
- Same-UID socket ownership does not prove a process is confined or defend against a malicious same-Principal actor. These checks fence routing/identity, not process containment. Native macOS and actual foreign-UID kernel behavior remain separate platform qualification; synthetic tests do not establish those results.
- Authority revalidation occurs before admission and delivery, and during a silent observation. Revocation, malformed/oversized frames, pending-event overflow, EOF, timeout and session switch poison the attachment. Cancellation takes ownership of the socket into the read future, so dropping that future closes the socket instead of leaving a partial frame reusable.
- Directory scans, candidates, frames, pending events, per-probe and total discovery have explicit bounds. There are no hidden reader/reconnect/relaunch paths. Active control has a bounded idle revocation watchdog that closes only the attachment, never the terminal-owned Pi.
- A socket root is a shared temporary directory, so the scan bound counts what it examines, never unrelated files: unrelated entries never fail discovery or consume a candidate slot, `pi-tui-rpc` names whose PID is gone are skipped before probing, and live candidates are probed with bounded concurrency inside the discovery deadline. A crowded root with tens of thousands of entries must still find the matching Pi, so the outcome cannot depend on directory order.

## History and event authority

Durable Chat remains runner-owned `oqto-log`, populated through the existing Pi importer (ADR-0025). Pi owns JSONL; Oqto never writes it. The adapter reads only the bounded source header to verify the admitted native identity, not an alternate transcript projection.

`ObservedEvent` is **private native translator input**, deliberately not a canonical wire/replay type. Its socket timestamp is not an event ID, checkpoint or replay cursor. Disconnect requires fresh admission plus canonical snapshot/tail convergence. Do not forward these values directly to HTTP/WebSocket/mobile or reconcile them by text/index/order.

## Remaining integration (not implemented by this adapter)

1. Supervisor-owned, client/Account-bound observe admission and semantic refusal/capability reporting (ADR-0057). No flag, request variant or network route currently enables this module.
2. Canonical snapshot plus event-tail ingestion/replay through `oqto-log`, with stable event/message identities and explicit drift/gap handling.
3. A single-writer continuation fence: a live externally held session file must never cause `PiSessionManager` to spawn a second Pi. The transport alone does not add that manager guard.
4. Wire the control primitives to an actual authenticated host grant/MFA issuer (M4). `ControlAuthority` and the non-deserializable `ControlPermit` fence Account/resource/controller/generation, verified MFA age (at most five minutes), and permit lifetime (at most one minute); they are not themselves authentication. Connection-scoped terminal confirmation, idle revocation and typed input are implemented at the seam. Read-only observation never acquires control implicitly; uncertain writes never retry. Queued input cleanup, canonical convergence and native-platform acceptance still require integration proof.
5. Incremental import (M2), iOS Machines/adoption work, native Mac disposable-TUI proof, then separately authorized live rollout. Desktop's tracker-migration divergence must be preserved when reconciling its checkout; fetching the reference is not proof its local branch was pulled/merged.

## Verification

`just test-pi-tui-rpc` runs formatting, focused adapter tests and strict all-target runner Clippy. Control fixtures additionally test legacy refusal, MFA/permit bounds, controller fencing, active/idle revocation, disconnect and terminal lease behavior. Tests exercise synthetic Unix sockets and synthetic JSONL only: both layouts, protocol/native/file/work-directory mismatches, source-root and symlink rejection, duplicate identities, Account denial before connect, revocation, read cancellation, oversized frames, bounded pending events, and 40 attach/drop cycles with all connections reaching EOF. The observation-only fixtures issue only `get_state`; Pi sentinel bytes remain identical. Separate control fixtures exercise input explicitly without contacting real Pi sessions. Peer-denial predicates are tested separately from the same-UID kernel positive fixture. The full runner library suite also passes (two intentionally ignored tests). Cargo still reports the transitive `proc-macro-error2` future-compiler incompatibility tracked by `oqto-aavh`. This checkpoint deliberately accepts that bounded dependency debt on the current toolchain, changes no dependency pins and does not claim future Rust compatibility; dependency remediation remains separate from attachment authority.

See [existing machine history](machine-history.md), [native Mac runner](macos-runner.md), and [runner-owned network grants](../adr/0057-runner-owned-network-filesystem-grants.md).
