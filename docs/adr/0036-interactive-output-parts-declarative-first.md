# Interactive output parts: declarative payloads first, sandboxed apps later

## Status

Proposed (2026-08-08), generalized by accepted [ADR-0038](0038-agent-built-apps-installation-binding-and-filesystem-discovery.md). Tracked by `oqto-76r0`. Fills the gap left by [ADR-0031](0031-session-centric-workbench-shell.md), which removed the A2UI v0.8 runtime and stated "a future lean interactive-output mechanism requires a new decision." ADR-0038 retains declarative interaction parts and MCP-Apps-shaped sandboxed web content as two portable App presentations, while ADR-0037 allows App Views outside Chat. Constrained by the canonical-protocol authority in [ADR-0006](0006-pi-first-class-harness-others-bridged.md) and OqtoUI's multi-host trust boundary.

## Context

Agents need to return more than prose: forms that collect structured input, choice prompts, small dashboards, previews. The industry converged on two viable shapes while we had this gap open:

- **Declarative payload specs** (Google A2UI v0.9/v1.0-RC, OpenAI Open-JSON-UI): the agent emits JSON describing UI intent; the client renders it with its own component library. Safe like data, themed like the host, capped by the client's component registry.
- **Sandboxed app specs** (MCP Apps, the official MCP extension unifying OpenAI's Apps SDK and community MCP-UI): a tool declares a `ui://` HTML resource; the host renders it in a sandboxed iframe with a postMessage bridge. Arbitrary expressiveness, portable across ChatGPT/Claude/VS Code/Goose, but foreign chrome and a heavier runtime.

A third category, interaction protocols (AG-UI), was evaluated and rejected as a competing contract for the seam oqto's canonical protocol already owns; its own documentation defers the payload question to the specs above. Full evaluations live in the wiki (`external-repo-evals/ag-ui.md`, `guides/generative-ui-landscape.md`).

oqto constraints that shape the choice:

- **Multi-user trust boundary.** Sessions render in browsers of users who did not author the workspace's agents. Unsandboxed agent-authored components are excluded categorically; the two admissible trust models are data-only payloads and sandboxed iframes.
- **Canonical protocol and oqto-log are the only authorities.** Whatever renders must arrive as a typed message part, persist in oqto-log, and replay identically on reload — no side-channel runtime.
- **Design-system integrity.** The workbench's Base24/theming discipline should extend to agent-produced UI; interactive outputs that ignore the user's theme read as foreign objects in the timeline.
- **Harness honesty (ADR-0006).** Interactive outputs originate as tool calls in the harness; the mechanism must not require harness-side changes beyond an ordinary tool.

## Decision

### 1. Interactive outputs are typed message parts on the canonical protocol

An interactive output is a part (`kind: interactive`) in an ordinary Session message: emitted by the agent as a tool call, carried by the runner, persisted in oqto-log with the payload and a schema/version tag, projected into the timeline like any other part. User responses travel back as ordinary Session input attributed to the part id. Reload replays the part from oqto-log; no live-connection state is authoritative.

### 2. The first payload family is declarative, rendered by first-party components

The lean mechanism ADR-0031 asked for is a declarative JSON payload rendered by oqto's own component registry (forms, choices, confirmations, tables, previews), styled entirely by the active scheme. The payload schema follows the A2UI direction (v0.9+, "safe like data") rather than reviving the removed v0.8 runtime; whether we adopt A2UI verbatim or a profile of it is an implementation decision inside this ADR's boundary, recorded when the first renderer lands.

Rationale: data-only payloads preserve the multi-user trust boundary without a sandbox runtime, keep theming intact, and cover the overwhelming share of real interactions (collect input, choose, confirm, show structured results).

### 3. Sandboxed apps are the later, heavier tier — MCP Apps shaped

When an interaction outgrows declarative payloads (canvases, players, third-party apps), the escape hatch is an iframe-sandboxed resource following the MCP Apps direction, so oqto stays compatible with where the tool ecosystem converged and agents can reuse existing app-building skills. This tier is explicitly deferred: it lands only when a concrete surface demands it, behind its own security review (CSP, bridge capability allowlist, egress policy per ADR-0030/0035).

### 4. What is rejected

- **AG-UI as protocol or payload** — wrong layer; the canonical protocol owns the seam. Its interrupt-outcome taxonomy and run-lineage framing are noted as references for protocol evolution, nothing more.
- **Unsandboxed agent-authored components** (bb-style host-bundle plugins) — incompatible with multi-user trust.
- **Reviving the A2UI v0.8 runtime** — ADR-0031's removal stands; v0.8 is legacy upstream too.

### 5. Agent-side contract

A mechanism agents cannot discover or get feedback from does not exist. Each tier ships with its agent affordances, not after them.

Declarative tier:

- **Runner-provided tool.** The runner registers an interactive-output tool with the harness (MCP server or extension; ADR-0006 keeps tool ownership in the harness, the runner is the actions seam). The call is long-running: it resolves with the user's validated submission as the tool result, so waiting maps onto the Blocked status. Timeout and cancel semantics are part of the tool contract.
- **Skill over schema-in-description.** The tool description stays minimal (provider description limits are real); the payload spec, component catalog, worked examples, and composition guidance live in a workspace-synced skill. Composing the payload as a discrete step is recommended in the skill (dedicated UI-generation calls outperform mixed-task authoring); a UI-composer subagent remains an option if quality demands it.
- **Fail-loud validation.** Invalid payloads return structured tool errors naming each violation so the model can retry unaided. The renderer never best-efforts an invalid payload.
- **Capability discovery.** The component registry is versioned additive; agents can query which components/versions the connected frontend renders before authoring against them.

Sandboxed tier:

- **App authoring is ordinary workspace coding.** Agents build MCP servers with `ui://` resources; the upstream MCP Apps agent skills (`create-mcp-app`, `add-app-to-server`, `migrate-oai-app`) are installed rather than re-authored.
- **oqto's obligations are host-side:** MCP server registration with the harness, resource fetch through the runner under egress policy (ADR-0030/0035), and a bridge capability allowlist that is readable by the agent.
- **Preview loop.** Agents verify their app against the sandboxed host in the dev environment (agent-browser) before handing over, matching the frontend verification discipline.

## Consequences

- Chat remains the single surface for agent-initiated interaction; the timeline gains one part kind, not a parallel UI channel.
- The declarative component registry becomes a versioned contract (additive-only), reviewed like protocol changes, with generated types kept fresh.
- Pending interactive parts integrate with the Blocked status and future pending-interaction surfacing (favicon/title attention), since a Session waiting on a form is blocked on the user.
- The sandboxed tier, when it comes, inherits egress and capability policy work — it must not arrive as "just an iframe."

## Verification

- The first implementation must demonstrate: part persisted in oqto-log with schema version; identical render after reload; response attributed to the part and visible in Session history; theming across all four house schemes; rejection (fail-loud placeholder) of unknown schema versions.
- Agent-side proof: a harness session discovers the tool, authors a valid payload from the skill alone, receives a structured validation error for an invalid payload and self-corrects, and receives the user's submission as the tool result.
