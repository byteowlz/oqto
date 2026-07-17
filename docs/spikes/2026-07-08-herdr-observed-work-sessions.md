# Herdr observed Work Sessions spike

Date: 2026-07-08  
Issue: `oqto-fxen`

## Assessment

Recommendation: **adopt Herdr as an `observed` backend with constraints**, not as a replacement for native Pi.

Herdr is good enough to be Oqto's broad, low-friction attach/read/send layer for terminal harnesses when Oqto remains the authority for session identity, task metadata, tenancy, placement, and durable history. The observed tier should be explicitly lower fidelity than native Pi: terminal text is useful and attach is straightforward, but status and structured events depend on Herdr integrations or explicit `pane report-agent` calls.

## Prototype

Added a spike wrapper:

```bash
scripts/spikes/herdr_observed_spike.sh
```

It exposes Oqto-facing operations keyed by an Oqto session id while storing Herdr ids only as adapter-local bindings in `/tmp/oqto-herdr-observed-spike.json`.

Supported commands:

```bash
scripts/spikes/herdr_observed_spike.sh start [session-id] [cwd] [task]
scripts/spikes/herdr_observed_spike.sh list
scripts/spikes/herdr_observed_spike.sh read [lines]
scripts/spikes/herdr_observed_spike.sh send <text>
scripts/spikes/herdr_observed_spike.sh status
scripts/spikes/herdr_observed_spike.sh attach
scripts/spikes/herdr_observed_spike.sh cleanup
```

## Verification run

Command sequence run from repo root:

```bash
scripts/spikes/herdr_observed_spike.sh start sess_spike_herdr_002 /tmp 'Herdr observed backend spike via wrapper'
scripts/spikes/herdr_observed_spike.sh read 30
scripts/spikes/herdr_observed_spike.sh send 'hello wrapper'
scripts/spikes/herdr_observed_spike.sh read 40
scripts/spikes/herdr_observed_spike.sh status
scripts/spikes/herdr_observed_spike.sh attach
```

Observed output proved:

- Oqto-owned session id: `sess_spike_herdr_002`.
- Herdr-backed launch succeeded.
- `AGENT_CTX_*` injection reached the process:
  - `AGENT_CTX_PLATFORM_NAME=oqto`
  - `AGENT_CTX_PLATFORM_SESSION_ID=sess_spike_herdr_002`
  - `AGENT_CTX_WORKSPACE_PATH=/tmp`
  - `AGENT_CTX_HARNESS=shell`
  - `AGENT_CTX_TASK=Herdr observed backend spike via wrapper`
- Bounded terminal read works via `herdr agent read --source visible`.
- Send works; the harness echoed `GOT:hello wrapper`.
- Attach can be represented as an attach descriptor: `herdr agent attach oqto-herdr-spike`.
- Oqto-facing `list/status/attach/read` output does not use Herdr ids as public session ids.

## What worked

1. **Launch and env injection**: `herdr agent start ... --env KEY=VALUE -- <argv>` is sufficient for `AGENT_CTX_*`.
2. **External binding model**: `herdr agent start` returns `terminal_id`, `pane_id`, `workspace_id`, and `tab_id`; these map cleanly to adapter-local bindings/capabilities.
3. **Read/send**: `herdr agent read` and `herdr agent send` are enough for initial polling-based frontend streaming and input.
4. **Attach**: `herdr agent attach <label>` is a minimal viable attach contract.
5. **Existing integrations**: `herdr agent list` already detected live Pi/Claude sessions with statuses and native session ids/paths, which supports using Herdr as a broad observation surface.

## What failed or is risky

1. **Raw shell status remains `unknown`**: expected for a generic shell harness. Semantic status requires a Herdr integration or explicit `herdr pane report-agent` calls from the harness adapter.
2. **Visible reads can wrap long lines**: `--source visible` is reliable, but long env values wrapped at terminal width. `recent`/`recent-unwrapped` returned empty for this harmless shell in this run, so consumers should treat terminal reads as presentation text unless a better source is proven per harness.
3. **Agent labels are not stable identity**: labels are useful targets but must not become Oqto public identity. Production needs an Oqto session id -> Herdr binding store and collision handling.
4. **Durability is not enough for history authority**: Herdr terminal buffer is not a substitute for `oqto-log`. Observed sessions need separate event/log capture if Oqto wants replayable history.
5. **Placement boundary remains unresolved**: this local run proves host use. Container placement needs a follow-up test to decide whether Herdr runs inside the runner placement or at the host/placement boundary.

## Fidelity tier recommendation

Use three explicit tiers:

- `native`: Pi-native integration; structured events and durable history remain first-class.
- `bridged`: harness-specific adapter reports semantic state/events into Oqto while Herdr may provide terminal attach.
- `observed`: Herdr terminal operations only; best-effort status/read/send/attach.

Herdr should first ship as `observed`, then selected harnesses can become `bridged` by reporting status/session metadata through Herdr or Oqto adapter code.

## Minimal production contract

Public Oqto APIs should use only `oqto_session_id`:

```json
{
  "oqto_session_id": "sess_...",
  "task": "...",
  "repo": "/workspace/project",
  "harness": "claude|codex|opencode|shell",
  "fidelity_tier": "observed",
  "backend": "herdr",
  "status": "idle|working|blocked|unknown",
  "attach": { "kind": "command|url|descriptor", "command": "..." }
}
```

Adapter-private binding:

```json
{
  "herdr": {
    "internal": true,
    "agent_label": "...",
    "terminal_id": "...",
    "pane_id": "...",
    "workspace_id": "...",
    "tab_id": "..."
  }
}
```

## Next implementation issues

1. Add a small runner-side `HerdrObservedBackend` behind a feature/config flag with fake-command parsing tests.
2. Add a persisted append-only binding record keyed by `oqto_session_id` (respect ADR-0023).
3. Define Work Session read model and attach descriptor ADR.
4. Test one real Claude and one Codex/OpenCode launch through the wrapper, including status transitions.
5. Test Herdr inside the intended container placement environment.
6. Add a polling stream prototype and measure whether `herdr agent read --source visible` is acceptable or whether Herdr needs a push/socket path.
