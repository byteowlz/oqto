# oqto-bus is the in-process app/UI event fabric, distinct from gvnr

oqto-bus (`backend/.../bus/`, scoped pub/sub over the WS mux's `system` channel; session/workspace/global scopes; server-enforced authz; design 20260314) is kept, and is a different layer from the gvnr event log (ADR-0004). They are not redundant:

- **oqto-bus**: in-process (one backend), ephemeral/in-memory, real-time. Job: live app/UI/agent pub/sub within a session or workspace — inline HTML apps publishing events the agent or another browser tab reacts to, multi-tab coordination, UI intents. The canonical channels carry the agent *conversation*; the bus carries everything app/UI around it.
- **gvnr event log**: cross-fleet, cross-host, durable, best-effort. Job: "what is running where" awareness.

Relationship is one-directional: oqto-bus MAY forward selected events into gvnr's event log (awareness/telemetry); gvnr never drives the live bus path.

## Placement and scope

- Lives in `oqto-gateway` (real-time WS-mux relay infrastructure), not a domain crate (ADR-0008 decomposition).
- Scoped strictly to app/UI/system events. It must NOT become a second path for agent/file events that already have canonical channels — audit the existing `agent.rs`/`files.rs` bus usage for duplication.

## Survival condition

The bus is currently lightly used: the main producer is the publish command handler plus an admin viewer, and its headline consumer — the inline-app / oqto-serve app<->agent message channel — is deferred. The bus is kept BECAUSE that consumer is to be built. If the app-messaging feature does not materialize, the bus is a deletion candidate (infrastructure without a consumer is the config-without-implementation pathology, ADR-0007 truth-in-config in spirit). Keep on-consumer, not on-spec.
