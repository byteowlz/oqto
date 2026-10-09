# AgentEvent is the harness-event contract; JSONL is one carrier of it

Status: proposed (2026-09-09). Tracked by `oqto-7rq1.10`. Refines [ADR-0006](0006-pi-first-class-harness-others-bridged.md) by naming the input contract `PiTranslator` already consumes. Does not change [ADR-0025](0025-read-only-chat-reads-background-jsonl-ingest.md), which concerns Pi's durable session files, a different artifact that shares only the letters JSONL.

## Context

The Oqto Flash spike (`oqto-7rq1.1`, `../oqto-flash`) embeds `@earendil-works/pi-agent-core` directly in a browser tab. It receives Pi's typed `AgentEvent` values in-process, with no subprocess and no wire. That prompted the question this ADR answers: if a client can consume `AgentEvent` directly, is the runner's JSONL hop earning its place?

Examining the code makes the question sharper and different from how it was first posed.

`oqto-pi/src/types.rs` declares `PiEvent` as `#[serde(tag = "type", rename_all = "snake_case")]` over `AgentStart`, `AgentEnd`, `TurnStart`, `TurnEnd`, `MessageStart`, `MessageUpdate`, `MessageEnd`, `ToolExecutionStart`, `ToolExecutionUpdate`, `ToolExecutionEnd`, plus RPC-mode additions (`ExtensionUiRequest`, `AutoCompaction*`, `AutoRetry*`, `HookError`). `pi-agent-core` declares `AgentEvent` over `agent_start`, `agent_end`, `turn_start`, `turn_end`, `message_start`, `message_update`, `message_end`, `tool_execution_start`, `tool_execution_update`, `tool_execution_end`. The Flash spike's observed event trace matches both.

`PiEvent` is therefore not a foreign wire format that `PiTranslator` decodes. It is a hand-written Rust mirror of `AgentEvent`, and `PiTranslator::translate(&PiEvent) -> Vec<EventPayload>` already takes `AgentEvent` as its input contract without saying so.

The JSONL hop is not a translation step. It is transport of the same typed events across a language boundary: `oqto-runner` is Rust, Pi is JavaScript, and a Rust process cannot embed a JavaScript library. Flash drops the hop not because the hop is wasteful but because Flash is written in the same language as Pi.

Two further facts constrain the options. Pi publishes `@earendil-works/pi-protocol`, but that package contains only transport concerns — `codec`, `framing`, `cbor` — and no event schema. And `AgentEvent` is a TypeScript type with no runtime schema object, so there is nothing to generate a Rust mirror from mechanically.

## Decision

**`AgentEvent` is the declared input contract of canonical translation.** Canonical translation is defined as a function from Pi's harness event stream to canonical protocol events. It is not defined against a transport.

**JSONL over stdio is one carrier of that contract, not the contract.** It stays for `oqto-runner`, where the language boundary makes it the cheapest faithful carrier: Pi's own events, tagged JSON, nothing lost. In-process delivery is a second carrier for hosts written in JavaScript. Neither carrier is privileged and neither may add or drop events.

**Every implementation of the contract is a mirror, and mirrors must be bound by shared fixtures, not by discipline.** `oqto-pi`'s `PiEvent` is one mirror. Flash's mapping will be a second. A published schema would be preferable, but Pi ships none, so:

- A fixture corpus of recorded `AgentEvent` streams becomes the shared artifact — real captures covering at minimum a plain turn, a tool call, a tool error, auto-compaction, auto-retry, and an aborted turn.
- Every mirror decodes the same corpus in CI. A mirror that fails to decode a fixture is a build failure, not a runtime surprise in one host.
- Adding a Pi version pin bump means re-capturing the corpus. The existing test-gated auto-bump (`just check-agent-updates`) is the right place to enforce this, because an unrecognised event variant is exactly the class of breakage that gate exists to catch.

**`PiEvent` keeps its RPC-only variants and this is not drift.** `ExtensionUiRequest`, `AutoCompaction*`, `AutoRetry*` and `HookError` arrive from Pi's RPC mode, which is a superset of what an embedded harness emits. A mirror may cover more of Pi's surface than another host needs; it may not cover the same event differently.

## What this does not decide

It does not propose removing the JSONL hop from `oqto-runner`. Rust cannot embed Pi, and rewriting the runner in JavaScript to avoid a serialization step would trade a cheap, faithful carrier for a rewrite of 21.4k lines whose real content is placement concerns — process supervision, sandbox integration, transport security, remote bridging — none of which the hop causes.

It does not make Flash's mapping a port of `pi_translator.rs`. That file is 1873 lines because it maps `AgentEvent` onto canonical events *and* carries Pi-RPC-specific handling. Flash needs the mapping, not the RPC handling. Sizing Flash's work from the Rust line count overstates it, which is what prompted `oqto-7rq1.9`.

It does not weaken [ADR-0001](0001-runner-is-the-sole-execution-interface.md). Flash consuming `AgentEvent` in-process is a runner implementation, not a bypass: the events still cross a socket-shaped seam before any UI sees them (`oqto-7rq1.2`). A host that renders `AgentEvent` directly in its UI has stopped implementing the contract and started forking it.

It says nothing about Pi's durable session files. ADR-0025's background ingest reads what Pi owns and writes on disk. That artifact and this event stream are unrelated despite both being newline-delimited JSON, and conflating them would put Oqto on the wrong side of "Pi owns JSONL session files and Oqto must not write them".

## Consequences

- The canonical translator gains a nameable, testable input contract, so "does this host translate correctly" becomes a question with a shared answer.
- A second mirror in TypeScript is accepted cost, bounded by the fixture corpus. Without the corpus it would be unbounded, because the drift is silent until a specific event variant appears in production.
- Bridged harnesses (ADR-0006) are unaffected: they translate their own native events to canonical and never claim to emit `AgentEvent`. This ADR names Pi's contract, not a universal harness event format, and generalising it to bridges would repeat the mistake ADR-0006 rejected.
- A Pi upgrade that changes event shape now fails in CI at the mirror, rather than in one host at runtime.

## Open question for review

Whether the fixture corpus should live in `oqto_refactor` or beside the Pi pin. Keeping it here makes it available to the auto-bump gate immediately; keeping it beside the pin makes it obviously versioned with the thing it describes. Recommend here, under `fixtures/`, because the gate is the mechanism that gives it teeth.
