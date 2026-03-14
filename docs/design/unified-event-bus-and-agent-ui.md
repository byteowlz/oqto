# Unified Event Bus and Agent-Controlled UI

Status: Draft proposal

Owner: Oqto platform

Last updated: 2026-03-14

## 1. Problem Statement

Oqto currently has multiple interaction patterns:

- canonical `agent` stream events
- tool-call-bound interactive UI flows (including A2UI-style patterns)
- app runtime via iframe apps
- domain-specific channels (`trx`, `hstry`, `files`, `terminal`, etc.)

This creates duplicated plumbing, weak cross-domain automation, and limited UI interactivity patterns.

We need a unified model where:

1. agents can create ad hoc visual UIs (inline or fullscreen)
2. user actions from those UIs can flow back bidirectionally
3. interactions are non-blocking by default, optionally blocking
4. events are secure and user-isolated by default
5. `trx`, `hstry`, `mmry`, and platform services can emit/consume the same event fabric

## 2. Goals and Non-Goals

### Goals

- Introduce a scoped Oqto Event Bus (`bus` channel) for publish/subscribe.
- Keep `agent` stream protocol intact for LLM conversation streaming.
- Unify inline iframe UI and fullscreen app tabs on one app runtime + bus bridge.
- Default to per-session scope, optional workspace/global scope with strict authz.
- Persist app artifacts and app state so reload restores UI.
- Support agent-controlled frontend intents with permission gates.
- Add ack support for control-plane/admin events.
- Provide low-effort integration path for Byteowlz apps via generic hooks.

### Non-Goals (v1)

- Replacing canonical `agent` event stream with bus transport.
- End-to-end payload encryption for all bus events.
- Exactly-once delivery semantics.
- Cross-org federation or multi-cluster bus routing.

## 3. Architectural Positioning

Use two planes:

- **Agent stream plane**: canonical `agent` events and commands remain unchanged.
- **Event plane**: new `bus` channel for app/UI/system/domain events.

The runner bridges these planes via policy:

- handles infra/system events directly where possible
- injects only allowlisted high-signal events into agent context
- exposes pull APIs for on-demand event retrieval

## 4. Scopes and Security Model

## 4.1 Scope defaults

- Default scope: `session`
- Optional scope: `workspace`
- Restricted scope: `global` (read-only for most clients, publish by backend/admin only)

## 4.2 Topic format

`<scope>/<domain>.<event>`

Examples:

- `session/app.submit`
- `workspace/trx.issue_updated`
- `global/admin.agents_updated`

## 4.3 Hard isolation guarantees

1. Backend is the sole authz authority.
2. Connection identity (`user_id`, `runner_id`, role) is server-bound, never client-specified.
3. Every publish and subscribe is authorized server-side.
4. Backend rewrites/stamps source and scoped topic.
5. Delivery is from server-owned subscription tables only.
6. App iframes never get raw socket access; only bridge API.

## 4.4 App iframe sandboxing

- Render with sandbox defaults (`allow-scripts`; deny same-origin and top-level navigation by default).
- Bridge only via controlled `postMessage` API.
- App can publish/subscribe only to allowed app-scoped topics unless explicit permissions are granted.

## 4.5 Workspace scope protection

- Workspace publish/subscribe requires explicit membership checks.
- App-level cross-workspace access is denied.
- Optional capability grants are explicit, scoped, short-lived.

## 5. Event Envelope (Canonical Bus Envelope)

All bus events must carry:

- `event_id`
- `scope`: `session | workspace | global`
- `scope_id`
- `topic`
- `source`: typed source identity stamped by backend
- `ts`
- `payload`

Optional:

- `priority`
- `ttl_ms`
- `idempotency_key`
- `correlation_id`
- `reply_to`
- `ack`

### Delivery semantics (v1)

- At-least-once delivery.
- Idempotency required for consumers of sensitive commands.
- Bounded queues per scope/subscriber.

## 6. Runner Event Processing Model

Runner keeps priority queues per session.

Priorities:

- `Interrupt`: immediate interrupt (abort/upgrade/security)
- `Immediate`: execute now without agent involvement (config sync, app runtime housekeeping)
- `NextIdle`: deliver to agent when idle (high-signal app submits, selected domain events)
- `Batched`: aggregate for pull/summaries

Policy:

- very small auto-inject allowlist
- most events are pull-on-demand by agent
- automatic coalescing, truncation, and summarization before context injection

### 6.1 App Event Injection Modes: Steer and Follow-up

For app-originated events selected for auto-injection, runner supports two explicit delivery styles:

- **Steer injection**: prepend event context before the next user prompt so the model can adapt strategy immediately.
  - Use for high-signal directional inputs (user selected path, changed goal, approved/rejected plan).
  - Injected as short structured guidance with highest injection priority.

- **Follow-up injection**: append event context as trailing continuation instructions after the user prompt or as queued post-turn context.
  - Use for secondary updates (field edits, progress markers, non-critical UI state changes).
  - Lower priority; may be coalesced more aggressively.

Runner policy tags each app event with `inject_style: steer | follow_up | none`.
`none` remains default for most app events (pull-on-demand only).

## 7. Agent Context Protection Rules

Before any auto-injection:

1. dedupe/coalesce (`latest` by key)
2. drop stale by TTL
3. summarize within token budget
4. hard cap count and bytes
5. append pointer metadata: `N more events available via events.list`

Required agent tools:

- `events.list(since, topic?, limit?)`
- `events.get(event_id)`
- `events.summary(since, budget_tokens)`

## 8. Unified App Runtime (Inline + Fullscreen)

Inline and fullscreen are one artifact with different presentation modes.

- Inline: embedded in chat message
- Fullscreen: app tab/view
- Same `app_id`, same bridge API, same state backend

### Bridge API (inside app iframe)

- `oqto.publish(topic, data)`
- `oqto.subscribe(topic, cb)` / `oqto.unsubscribe(...)`
- `oqto.send(data)` convenience for app message
- `oqto.saveState(state)` / `oqto.loadState()`
- `oqto.requestFullscreen()` / `oqto.requestInline()`
- `oqto.theme`, `oqto.onThemeChange(cb)`

## 9. App Artifact and State Persistence

Recommended filesystem layout:

- user-editable reusable apps: `oqto_apps/`
- runtime/instance artifacts: `.oqto/apps/`

### Storage rules

- Reusable app sources live in `oqto_apps/<name>/index.html`.
- Ad hoc/generated app instances materialize under `.oqto/apps/instances/<app_id>/`.
- Chat history references app instances via canonical message parts (`x-app` extension part).
- Mutable runtime state persists separately (e.g. `hstry` app state store) keyed by `app_id` and session/workspace scope.

Reload behavior:

- Rehydrate `x-app` parts from history.
- Restore latest persisted app state.
- Preserve inline/fullscreen mode and restore user-visible state.

## 10. Ack Protocol

Ack is for control-plane events, not all data-plane events.

Publisher can include ack request metadata:

- `reply_to`
- timeout
- expected receiver set (optional)

Consumers respond with ack payload:

- `status`: `ok | error | skipped | queued`
- `duration_ms`
- detail/error

Backend aggregates ack status and emits summary events for admin/automation.

## 11. Runner Registration and Future Remote/Kubernetes Support

Use a control-plane registration protocol with lease heartbeats.

- register runner identity + capabilities
- issue short-lived data-plane credentials
- maintain lease via heartbeat
- support draining and graceful unregister

Identity and transport:

- local mode: existing local trust model + explicit identity binding
- remote/K8s: mTLS or signed workload identity + short-lived tokens

All bus authz remains backend-enforced regardless of transport.

## 12. Agent-to-Agent Communication

Supported via workspace-scoped topics with existing workspace membership checks.

Examples:

- `workspace/agent.request`
- `workspace/agent.result`
- `workspace/agent.handoff`
- `workspace/agent.conflict`
- `workspace/agent.status`

Runner mediates delivery and context policy; no direct bypass of isolation boundaries.

## 13. Agent-Controlled Frontend UI Intents

Agents do not mutate arbitrary DOM. They emit validated UI intents.

Namespace: `session/ui.*`

Examples:

- `session/ui.open_panel`
- `session/ui.close_panel`
- `session/ui.open_file`
- `session/ui.select_session`
- `session/ui.open_app`
- `session/ui.show_toast`
- `session/ui.layout.set`

Permission tiers:

- `ui.read`
- `ui.control.basic`
- `ui.control.navigation`
- `ui.control.layout`
- `ui.control.admin`

Frontend executes intents through a strict command registry with schema validation.

## 14. Bus Event Taxonomy (v1/vNext)

### App/UI

- `session/app.created`
- `session/app.updated`
- `session/app.closed`
- `session/app.mode_changed`
- `session/app.message`
- `session/app.submit`
- `session/app.state_changed`
- `session/app.error`

### Agent

- `session/agent.turn_started`
- `session/agent.turn_completed`
- `session/agent.working`
- `session/agent.idle`
- `session/agent.error`
- `session/agent.tool_call_started`
- `session/agent.tool_call_completed`
- `session/agent.tool_call_failed`
- `session/agent.context_usage`

### Runner

- `global/runner.registered`
- `global/runner.unregistered`
- `global/runner.heartbeat`
- `global/runner.draining`
- `session/runner.queue_enqueued`
- `session/runner.queue_dropped`
- `session/runner.queue_drained`

### Files/Terminal/Session

- `workspace/files.batch_changed`
- `session/terminal.output`
- `session/terminal.exit`
- `session/lifecycle.created`
- `session/lifecycle.stopped`

### trx

- `workspace/trx.issue_created`
- `workspace/trx.issue_updated`
- `workspace/trx.issue_closed`
- `workspace/trx.sync_completed`
- `workspace/trx.sync_failed`

### hstry

- `session/hstry.message_appended`
- `session/hstry.persist_completed`
- `session/hstry.persist_failed`
- `session/hstry.compaction_completed`

### mmry

- `workspace/mmry.entry_added`
- `workspace/mmry.entry_updated`
- `workspace/mmry.entry_deleted`
- `session/mmry.search_completed`
- `session/mmry.search_failed`

### Admin/Security

- `global/admin.agents_updated`
- `global/admin.skills_updated`
- `global/admin.models_updated`
- `global/admin.ack`
- `global/admin.ack_summary`
- `global/security.policy_violation`

## 15. Byteowlz-Wide Adoption with Minimal Effort

Adopt a shared hook contract and lightweight emitter libraries.

### Generic hook support

Cross-app hook shape:

- `before(op, ctx)`
- `after(op, ctx, result)`
- `error(op, ctx, err)`

Map operations to topics via config (`event-hooks.toml`), so most apps can emit bus events at API/CLI/service boundaries without deep refactors.

Minimal strategy:

1. boundary instrumentation first (no major app logic changes)
2. native deep domain emit points later where useful
3. bus down -> non-fatal (best effort) for non-critical telemetry events

## 16. Migration Plan

### Phase 1: Foundations

- Add `bus` channel to WS mux and backend routing/authz.
- Implement scope checks and subscription registry.
- Add minimal ack support for admin events.

### Phase 2: App Runtime Unification

- Refactor inline/fullscreen app rendering to shared runtime and bridge.
- Add persistent app state backend + reload restoration.
- Keep non-blocking default, blocking optional.

### Phase 3: Runner Policy + Agent Tools

- Add runner queue policy and event pull tools.
- Enable small auto-inject allowlist + truncation/summarization.

### Phase 4: Domain Integrations

- Emit `trx`/`hstry`/`mmry` boundary events.
- Introduce generic hooks in Byteowlz services.

### Phase 5: UI Intent Control

- Implement `session/ui.*` intent registry + permissions.
- Add audited policy checks and tests.

### Phase 6: A2UI Retirement

- Migrate remaining A2UI use cases to app runtime + bus interactions.
- Remove A2UI surfaces and docs once parity is reached.

## 17. Risks and Mitigations

- Event storms -> queue limits, coalescing, pull-first design
- Context bloat -> strict token budgets and summarization
- Cross-user leakage -> server-side authz on every publish/subscribe
- Operational uncertainty -> ack summaries + audit logs
- Scope creep -> strict v1 event subset and phased rollout

## 18. Open Decisions

1. `hstry` app-state persistence schema shape and retention policy.
2. Capability token format for optional workspace/global app permissions.
3. Exact `ui.*` intent list included in v1.
4. Which domain events are mandatory in v1 vs vNext.

## 19. Acceptance Criteria (v1)

- Session-scoped bus events are isolated by `user_id` and `session_id`.
- Workspace events are visible only to workspace members.
- Inline and fullscreen app views share one runtime and persisted state.
- Non-blocking app interactions work end-to-end.
- Blocking app interactions are optionally supported.
- Runner only auto-injects allowlisted events and enforces truncation budgets.
- Admin `global/admin.agents_updated` supports ack + summary.
- At least one `trx`, one `hstry`, and one `mmry` event integrated via boundary hooks.
