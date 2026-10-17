# Read-only Project agent activity

`GET /api/session-activity` (authenticated; optional `shared_workspace_id`) projects current **Oqto-managed Pi** facts. It is not process liveness, durable history, a work queue, or execution authority. External/TUI/herdr-only agents and remote-machine observation are not included in v1. Do not advertise all-agent totals or infer those sources are idle.

Both Server and owning runner need this additive protocol update. Older Servers return404; older/unreachable runners produce unavailable coverage, never fabricated zero. Publishing source does not deploy/restart either service. Existing placement, authorization, transport authentication and runner ownership remain independent.

## Contract

Response: `version:1`, `source:"oqto_managed_pi"`, `coverage:"complete"|"partial"|"unavailable"`, `observed_at_ms`, and bounded `sessions`. Each row carries platform Session ID, optional external history ID, exact absolute Work directory, execution UUID, retained-input revision, active-turn `running`, `attention_count` (pending canonical input records), and `complete`. The revision describes input-event retention, not a canonical/history cursor. Full responses replace prior observations; clients must not merge them into transcript history.

Runner retains request IDs/deadlines only, not prompt titles/messages/credentials. It observes canonical input-needed/resolved/idle/closed/fatal events without subscribers. Successfully written extension replies resolve the corresponding pending input; timeout/exit clear attention. Logical New/Switch/Fork replaces the execution UUID and clears old requests, conservatively incomplete until an explicit idle/created observation. Working does not erase outstanding input. Running is Streaming/Compacting/Aborting (active turn), so an agent with outstanding input can also be running; categories are not disjoint totals. Multiple requests from one agent count as one Needs attention agent in Project rollups. Errors are not heuristically classified from message text as human blocking.

512 Session rows/128 pending requests per execution; overflows are partial. Ordered bounded snapshots, execution identities and client cancellation/access epochs prevent replaying old results as a new credential or selection. Freshness is the consumer's recent bounded observation, not an invented OSC heartbeat requirement. External program-status integration needs its own qualified owner/source projection.

## Safety

- Resolver accepts existing placements/endpoints only; no ensure-ready, provisioning, recovery, terminal attach, Session open/resume or model dispatch.
- `RunnerClient::pi_activity_snapshot` deliberately uses `request_once`, not the retry/recovery execution helper.
- Server authorizes shared membership before dial and after read. Personal rows require exact owner/scope/Work directory bindings under the authorized root; only legacy development mode allows missing target records under that root. Known nonpersonal/other-owner records never become personal.
- Reject parent escapes, excessive/invalid rows and unsupported versions. Response is `Cache-Control: no-store`; internal endpoint errors are not returned.2s Server read deadline; client4s/2MiB cap.
- Status observation grants no execution, approval, stdin replay, private terminal access or continuation capability.

## Evidence and limits

`just project-activity-test` covers retained lifecycle/bounds/epochs, one-attempt refusal, snapshot-only wire requests, exact personal owner/path/scope and root escape checks. Client fake integration additionally checks no Session opens/creates, runs, prompts or workspace requests, plus late sign-out/denial/unsupported handling. Native shared root/thumb evidence belongs in Studio's existing Projects activity-r8 trail.

Qualified source: full Server library **329pass/2ignored**; runner **165pass/6fail/3ignored** versus the established native baseline159/6/3. All six added activity tests pass; the same five credential fixtures and sandbox fixture remain failing under `.10.5`, so this is not a green full runner/platform claim. Workspace cargo check, strict library Clippy and fmt pass with1.97.0.2425source entries match before/after; only this qualification documentation changes afterward. Rust full-server tests rewrite19tracked frontend TypeScript export outputs: their diffs were retained privately and only those owned outputs restored to published baseline; they are not silently shipped as an unrelated frontend update.

Static delta retains9major clone gates,20minor and14new-symbol findings. Typed request/response wrappers deliberately follow existing per-command APIs and do not share the execution/recovery helper; scoped lifecycle fixtures remain independent. Added8th/17th task metadata parameters and bounded validator size/complexity are explicit trade-offs, not a clean-quality claim or reason to refactor unrelated authority paths in this change. No blanket acknowledgement/reset suppresses the evidence.

Live backend deployment/restart, private workflows, external/TUI source integration, physical/VoiceOver acceptance and canonical single-writer continuation are separate gates. Source publication does not authorize them.
