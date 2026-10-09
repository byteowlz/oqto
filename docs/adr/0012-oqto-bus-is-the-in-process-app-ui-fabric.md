# oqto-bus is the in-process app/UI event fabric, distinct from gvnr

> **Status update (2026-08-21): Superseded — the bus was removed.** The bus's
> own survival condition (no consumer beyond an admin debug panel; App and
> Customization consumers, ADR-0038/0040, never shipped against it) triggered.
> A cross-account authorization hole in `BusEngine::authorize_scope`
> (`oqto-07q8`) forced the decision: fix or remove before exposing consumers.
> Removal proof: `backend/crates/oqto/src/bus/` deleted along with the WS `bus`
> channel, `/admin/bus/*` routes, frontend `bus-client`/`use-bus`, and the
> admin Event Bus panel; no production consumer existed. If a future app/UI
> fabric is needed, design it against ADR-0039 capability endpoints and this
> ADR's original separation rationale rather than reviving the bus.

oqto-bus (`backend/.../bus/`, scoped pub/sub over the WS mux's `system` channel; session/workspace/global scopes; server-enforced authz; design 20260314) is kept, and is a different layer from the gvnr event log (ADR-0004). They are not redundant:

- **oqto-bus**: in-process (one backend), ephemeral/in-memory, real-time. Job: live app/UI/agent pub/sub within a session or workspace — inline HTML apps publishing events the agent or another browser tab reacts to, multi-tab coordination, UI intents. The canonical channels carry the agent *conversation*; the bus carries everything app/UI around it.
- **gvnr event log**: cross-fleet, cross-host, durable, best-effort. Job: "what is running where" awareness.

Relationship is one-directional: oqto-bus MAY forward selected events into gvnr's event log (awareness/telemetry); gvnr never drives the live bus path.

## Placement and scope

- Lives in `oqto-gateway` (real-time WS-mux relay infrastructure), not a domain crate (ADR-0008 decomposition).
- Scoped strictly to app/UI/system events. It must NOT become a second path for agent/file events that already have canonical channels — audit the existing `agent.rs`/`files.rs` bus usage for duplication.

## Survival condition

The bus is currently lightly used: the main producer is the publish command handler plus an admin viewer, and its headline consumer — the inline-app / oqto-serve app<->agent message channel — is deferred. The bus is kept BECAUSE that consumer is to be built. If the app-messaging feature does not materialize, the bus is a deletion candidate (infrastructure without a consumer is the config-without-implementation pathology, ADR-0007 truth-in-config in spirit). Keep on-consumer, not on-spec.
