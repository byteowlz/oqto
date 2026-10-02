# Herdr is Oqto's agent-process runtime carrier; the runner stays the authority

Status: proposed (2026-10-02). Direction set by Tommy (ship herdr with Oqto as the sub-agent orchestration layer); this ADR records the decision and its boundaries. Tracked by the herdr-carrier epic. Refines [ADR-0039](0039-placement-neutral-capabilities.md) (herdr becomes a named placement/carrier for agent processes) and builds on the sandbox-isolation thread decision (`oqto-q1vq`: runner-owned enforcement, neutral request surface). Constrains, and is constrained by, [ADR-0047](0047-runner-inventory-is-not-session-placement.md) (a Herdr profile grants nothing; Herdr is not the registration/control authority) and [ADR-0049](0049-agent-event-is-the-harness-event-contract.md) (AgentEvent is the event contract herdr-placed sessions feed back through).

## Context

Herdr (herdrdev/herdr, Apache-2.0, single Rust binary, 0.9.0 in the local stack) is "the runtime your coding agents live on": a background server that owns agent terminals, survives disconnects, marks every pane working/blocked/idle, and is agent-native — a CLI + socket API to spawn panes, prompt agents, and wait until an agent is genuinely blocked, plus `session.snapshot`/`layout.updated` events and SSH remote attach. It is already load-bearing in our local stack: Pi's subagent tool spawns into herdr tabs, and the `pi-herdr-tools` extension (sandbox-isolation thread) drives isolated subagent sessions through it. Oqto's own ADR-0047 already speaks of Herdr profiles while denying Herdr any control authority.

The question this ADR answers: what is Herdr *in* Oqto — a bundled dependency, an orchestration layer, or an incidental local tool? Shipping it with Oqto without naming its seam would fork orchestration logic between herdr-side and runner-side, exactly what `oqto-q1vq` was filed to prevent.

## Decision

1. **Ship Herdr with Oqto as a managed component.** Setup/doctor detects, installs (version floor ≥ 0.9), and can launch a Herdr server per host; Oqto config owns the socket location. Herdr is a system dependency, not vendored code.
2. **Herdr is the agent-process runtime carrier; Oqto remains the authority.** Herdr owns process placement, lifecycle, visibility, and attach for locally-placed agent processes. Oqto (runner/session layer) owns spawn authorization, sandbox profiles, session identity, the oqto-log history authority, approval gates, and monitors. The two are complementary substrates — the same substrate/orchestration boundary the herdr/firstmate ecosystem draws (lifecycle substrate below, orchestration above), mapped onto runner-below/herdr-carries.
3. **The authoritative spawn chain for subagents is:** request → runner (Gate + profile resolution, per `oqto-q1vq`) → placement adapter (carrier = herdr tab/pane running the sandboxed Pi process, carrying the neutral env contract) → AgentEvent feedback (ADR-0049) into oqto-log. Herdr pane ids are ephemeral carriers; the pane↔session mapping lives in oqto-log, session identity is never persisted in Herdr, and `pending-*`/`tmp:*` ids are never written anywhere.
4. **Herdr's agent-native signals are supervision inputs, not decision authorities.** working/blocked/idle state and wait-until-blocked feed monitors and the steer-vs-queue control plane (ADR-0024, Jev via native codemode); they never substitute for Gate decisions, approval pre-gates, or oqto-log facts.
5. **Host-neutral by construction:** the Herdr *server* is the integration point (socket API + `session.snapshot` events), not the TUI. Workbench/Glimpse attach through committed event feeds; native TUI attach remains a human convenience; every Oqto surface must work when Herdr is absent (degraded placement, headless fallback).
6. **Remote hosts:** one Herdr server per runner target (ADR-0047 runner inventory). `herdr --remote` is a human attach path over SSH — never the control plane, never an authority grant.

## Alternatives considered

- **tmux/zellij as the carrier** — rejected: no agent-native socket API, weaker lifecycle/state model, worse visibility semantics for agents.
- **Oqto builds its own multiplexer** — rejected: duplicates Herdr's matured lifecycle/attach work; Herdr is byteowlz-aligned and Apache-2.0, so alignment is cheap and upstream-improving.
- **Runner manages PTYs directly per subagent** — rejected for the same reason; it is the carrier's job, and losing attach/visibility would regress the human side.
- **Pi Durable as the process carrier** — category error: Durable is a candidate *conversation* substrate ([[pi-1-0-and-durable]], ADR-0039 input), orthogonal to process placement. If adopted it changes conversation durability, not where processes live.

## Consequences

- Oqto gains a supported way to see and steer every subagent (human attach) without giving Herdr any authority over enforcement or history.
- Sub-agent placement becomes a first-class, testable seam (placement adapters) instead of an extension-side convention.
- Herdr becomes a release dependency: version floors, socket-compat checks in doctor, and upgrade notes enter the release checklist.
- The sandbox-isolation thread's profile work (`oqto-q1vq`, subagent profile) lands on a named seam instead of an ad-hoc wrapper.

## Proof / acceptance

Children of the herdr-carrier epic must demonstrate: doctor install/verify with version floor (test); a subagent spawn flowing request → runner Gate → herdr pane with sandboxed Pi, with pane↔session mapping recorded in oqto-log (integration test); blocked-state supervision signal consumed as monitor input (unit test); Workbench/Glimpse attach without TUI (headless proof); all gates green per repo rules.
