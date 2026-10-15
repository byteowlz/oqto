# External Pi TUI attachment: runner adapter checkpoint

Tracks `oqto-8x3p.10.1` under `oqto-8x3p.10` (M3). This is the runner's **read-only transport seam**, not completed mobile/live adoption or an enabled network capability.

## Source and ownership

Ported from the fetched `oqto-desktop` main revision `b788b2fd24638bac22a0355a6c34f53ea8edb6e5`, `crates/oqto-pi-client/src/{live,transport}.rs`, and Desktop ADR-0005. Protocol v1 is defined by the canonical `pi-agent-extensions/pi-tui-rpc` extension. This does not fork the extension or embed Desktop UI dependencies.

`oqto_runner::tui_rpc::TuiObserver` connects to either same-Principal socket layout: `pi-tui-rpc-<pid>.sock` or `pi-tui-rpc-<uid>-<pid>/control.sock`. It issues only `get_state`. There is no generic command escape hatch, input lease acquisition, bulk `get_messages`, spawn, stop, or session-file mutation.

The external terminal continues to own Pi. Observed containment is always unverified/full access; attachment cannot retrofit confinement.

## Authority and identity checklist

- The embedding runner must implement `ObservationAuthority`. It resolves an authenticated Account/public Session to an unambiguous **existing durable binding**, designated work/session roots, and explicit observation grant generation **before** the adapter accesses files or sockets. Inventory, stored-history access and a local extension input lease are not observation grants.
- Binding data is not a deserializable client request. The adapter validates public/native distinction, selected canonical roots, the Pi file header, socket owner/mode, kernel peer UID (and PID where the platform provides it), socket inode stability, v1 hello, correlated response ID, native Session ID, exact session file and work directory.
- Same-UID socket ownership does not prove a process is confined or defend against a malicious same-Principal actor. These checks fence routing/identity, not process containment. Native macOS and actual foreign-UID kernel behavior remain separate platform qualification; synthetic tests do not establish those results.
- Authority revalidation occurs before admission and delivery, and during a silent observation. Revocation, malformed/oversized frames, pending-event overflow, EOF, timeout and session switch poison the attachment. Cancellation takes ownership of the socket into the read future, so dropping that future closes the socket instead of leaving a partial frame reusable.
- Directory scans, candidates, frames, pending events, per-probe and total discovery have explicit bounds. There are no background reader tasks or hidden reconnect/relaunch paths.

## History and event authority

Durable Chat remains runner-owned `oqto-log`, populated through the existing Pi importer (ADR-0025). Pi owns JSONL; Oqto never writes it. The adapter reads only the bounded source header to verify the admitted native identity, not an alternate transcript projection.

`ObservedEvent` is **private native translator input**, deliberately not a canonical wire/replay type. Its socket timestamp is not an event ID, checkpoint or replay cursor. Disconnect requires fresh admission plus canonical snapshot/tail convergence. Do not forward these values directly to HTTP/WebSocket/mobile or reconcile them by text/index/order.

## Remaining integration (not implemented by this adapter)

1. Supervisor-owned, client/Account-bound observe admission and semantic refusal/capability reporting (ADR-0057). No flag, request variant or network route currently enables this module.
2. Canonical snapshot plus event-tail ingestion/replay through `oqto-log`, with stable event/message identities and explicit drift/gap handling.
3. A single-writer continuation fence: a live externally held session file must never cause `PiSessionManager` to spawn a second Pi. The transport alone does not add that manager guard.
4. Separate control authority, fresh MFA, terminal-confirmed lease, queued-command revocation and terminal reclaim (M4). Read-only observation never acquires control implicitly.
5. Incremental import (M2), iOS Machines/adoption work, native Mac disposable-TUI proof, then separately authorized live rollout. Desktop's tracker-migration divergence must be preserved when reconciling its checkout; fetching the reference is not proof its local branch was pulled/merged.

## Verification

`just test-pi-tui-rpc` runs formatting, 13 focused tests and strict all-target runner Clippy. Tests exercise synthetic Unix sockets and synthetic JSONL only: both layouts, protocol/native/file/work-directory mismatches, source-root and symlink rejection, duplicate identities, Account denial before connect, revocation, read cancellation, oversized frames, bounded pending events, and 40 attach/drop cycles with all connections reaching EOF. Only `get_state` is observed by the fixture; Pi sentinel bytes remain identical. Peer-denial predicates are tested separately from the same-UID kernel positive fixture. The full runner library suite also passes (two intentionally ignored tests). Cargo still reports the transitive `proc-macro-error2` future-compiler incompatibility tracked by `oqto-aavh`. This checkpoint deliberately accepts that bounded dependency debt on the current toolchain, changes no dependency pins and does not claim future Rust compatibility; dependency remediation remains separate from attachment authority.

See [existing machine history](machine-history.md), [native Mac runner](macos-runner.md), and [runner-owned network grants](../adr/0057-runner-owned-network-filesystem-grants.md).
