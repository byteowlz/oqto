# Transport-neutral runner wire with authenticated, resumable connections

Status: accepted (2026-07-15). Extends ADR-0011 and ADR-0020, amends the local-socket-only reachability in ADR-0019, and preserves the session identity contract in ADR-0023. Tracked by `oqto-hns4`.

## Context

The live runner path is `oqto-runner::RunnerClient` to `oqto-runner::daemon` using newline-delimited JSON over a Unix socket. Client methods construct `UnixStream` directly, the daemon accepts only `UnixStream`, and routing resolves users/workspaces into socket paths. This is adequate for host-local execution but not for containers on private networks, clusters, or remote runners.

A second, currently unused protocol model exists in `oqto-protocol/src/runner.rs` (`RunnerHello`, `RunnerWelcome`, command/event envelopes and heartbeat). Adding transports without first choosing one canonical wire would preserve two competing runner protocols.

Transport recovery is also underspecified. A transport reconnect opens a new subscription without an application-level event cursor. QUIC, TLS, WebSocket, and Unix sockets can preserve bytes only while a connection lives; none can guarantee Oqto timeline convergence after disconnect. Reconciliation by text, index, array length, or visible order remains forbidden.

## Decision

### 1. One Oqto-owned runner wire

The live runner request/event behavior is the migration source of truth. The useful typed handshake, capability, heartbeat, and envelope concepts from `oqto-protocol/src/runner.rs` are folded into that live wire. The unused parallel protocol is then deleted or reduced to the canonical shared wire types; it must not remain a second implementation.

The canonical wire is independent of Pi and other harnesses. Harness-native RPC stays behind runner adapters. Public commands and events carry the Oqto session ID from ADR-0023.

### 2. Protocol and transport are separate

Runner framing and semantics operate over an established bidirectional byte stream. Concrete adapters only establish authenticated streams and report peer identity:

| Adapter | Intended placement | Default posture |
|---|---|---|
| Unix | host-local process and same-host container when a runtime directory is shared | enabled |
| TCP/TLS | containers on a VPS, private clusters, and statically reachable workers | enabled when configured |
| Iroh/QUIC | NATed, changing-network, laptop, edge, or user-operated runners | optional feature |

Iroh is not required for local or controlled-cluster operation. Public Iroh relays may be used for development; production use requires an explicitly configured relay policy, normally Oqto-managed relays.

The client-side seam is a runner dialer/connection interface; the server-side seam is a listener/accepted-connection interface. Session, workspace, API, and frontend modules receive a runner connection resolved by routing and never construct socket paths, TCP addresses, or Iroh endpoint IDs.

### 3. Placement resolves reachability

Routing becomes:

```text
ExecutionTarget
  -> PlacementStore
  -> RunnerRegistration
  -> RunnerEndpoint
  -> authenticated RunnerConnection
```

A `RunnerEndpoint` is a typed descriptor such as Unix path, TCP/TLS service identity and address, or Iroh EndpointId plus relay policy. Product sessions do not persist transport endpoints or connection IDs. Placement may move without changing the Oqto session ID.

This amends ADR-0019: bind-mounted Unix sockets remain valid for same-host container placement, but runner behavior is placement-independent because the protocol is invariant, not because every placement uses a socket.

### 4. Versioned bounded framing

Every connection starts with a bounded version/capability handshake. Subsequent frames are length-prefixed and size-limited; newline boundaries are not transport framing.

The common envelope includes:

- protocol version;
- frame/message ID;
- channel and payload kind;
- runner instance and verified runner registration IDs;
- workspace and Oqto session IDs where applicable;
- request correlation ID;
- deadline/cancellation metadata;
- event identity and delivery sequence where applicable.

Unknown versions, oversized frames, invalid routing identity, and unsupported required capabilities fail closed with typed errors.

### 5. Transport identity is not authorization

Each adapter yields verified peer evidence:

- Unix: socket ownership/permissions and peer credentials where supported;
- TCP/TLS: validated certificate/workload identity;
- Iroh: authenticated EndpointId.

That evidence binds to a `RunnerRegistration`. It does not itself authorize workspace access. Oqto separately checks that the registration may serve the requested placement/workspace and advertised isolation/capabilities. Runner enrollment, key rotation, certificate rotation, and revocation are auditable operations.

A reconnect creates a new `connection_id`/runner instance attachment. It does not mint a new runner registration, workspace, Oqto session, or harness binding.

### 6. Application-level replay and drift

Runner event delivery uses a session-scoped monotonically increasing `delivery_seq` and stable `event_id`. This sequence is a transport delivery checkpoint, not a second durable timeline identity. Durable oqto-log versions remain the authority for reload/history.

A subscriber attaches with `resume_after`. The runner reports:

- `fresh`: no prior attachment/cursor;
- `resumed`: all events after the cursor are available and replayed exactly once;
- `drift`: the cursor predates the replay floor or the runner lost its replay state.

The runner maintains a bounded replay window for transient delivery continuity. On drift, the gateway loads the durable oqto-log timeline and its checkpoint, then attaches to the live tail at an explicit watermark. Snapshot and tail must overlap by stable event/message identity, never by text or array position. If a complete durable/live join cannot be proven, the stream fails visibly rather than silently dropping or duplicating events.

Terminal tool/turn events are part of the same sequence contract. Success, error, abort, disconnect, and runner restart must converge to a non-spinning state through replay or durable recovery.

### 7. Connection shape

Unix may keep short-lived request connections during migration, but TCP/TLS and Iroh use persistent multiplexed connections to avoid repeated handshake cost. The canonical connection supports independent logical channels/streams for control, requests, session events, terminal data, and diagnostics. Multiplexing is transport-neutral: QUIC streams may implement it natively while Unix/TCP use framed channels.

Backpressure is bounded per channel. Slow consumers cannot grow unbounded queues or block runner heartbeats indefinitely.

## Implementation sequence

1. Reconcile and canonicalize the two existing runner protocol definitions.
2. Funnel live Unix I/O through one framing and connection seam without behavior changes.
3. Add in-process duplex conformance tests and retain Unix integration tests.
4. Add sequence/replay/resume/drift semantics and durable snapshot-to-tail convergence tests.
5. Integrate typed runner endpoints and registrations with PlacementStore.
6. Add TCP with mutual TLS and prove rootless-container placement.
7. Spike optional Iroh against `ubuntu-worker` using the same conformance suite.
8. Run the recorded browser reliability journey through real placement routing.

## Verification contract

The same protocol conformance suite must run against in-process, Unix, and TCP/TLS adapters. It covers handshake/version failure, bounded framing, hello/capabilities, request/response, cancellation/deadlines, event ordering, exact replay, stale-cursor drift, terminal events, unauthorized routing, reconnect, and revocation.

Container acceptance proves one workspace routes to one registered runner, survives runner reconnect without changing Oqto session identity, and produces browser screenshots, video, trace/HAR, logs, Pi JSONL, and durable timeline evidence. The optional Iroh spike additionally reports direct/relay path, reconnect latency, binary/build cost, endpoint-key lifecycle, and comparison with TCP/TLS.

## Consequences

- Unix remains the simplest local transport.
- TCP/TLS is the normal container/VPS/cluster transport.
- Iroh solves a narrower unmanaged/NATed reachability problem and remains optional.
- Placement and transport evolve independently behind typed contracts.
- Session correctness no longer assumes connection continuity.
- The migration is substantial: current inline `UnixStream` calls and socket-path construction must be centralized before network transports are safe.

## Non-goals

- Replacing oqto-log with an in-memory replay ring.
- Exposing raw harness RPC as Oqto's public runner protocol.
- Building a distributed placement registry inside the transport crate.
- Making every local runner listen on a network port.
- Using Iroh for controlled environments where Unix or TCP/TLS is simpler.
