# Chat conformance corpus

Platform-neutral golden fixtures for how a conversation is assembled and
presented. Every client that renders Oqto chat — the web shell and the native
desktop client today — replays these files and must produce the expected
result. A client proves equivalence by consuming the JSON alone, never by
reading another client's code.

This exists because chat drifted without it. The compositor corpus in
`contracts/oqto-ui/traces/` keeps the layout engine identical across hosts;
chat had no such contract, and within ten days the two clients disagreed on
how announcements, attachments and detail levels are presented. A new chat
behaviour now lands here as a fixture first, and a client that has not caught
up fails its replay rather than drifting silently.

## Input vocabulary

Stream inputs are pi `AgentEvent` values (ADR-0049), one object per event,
exactly as pi emits them in RPC mode. That is the one format every client can
reach: the desktop client reads it directly from pi, and the web shell reaches
it through the runner's translation, which a replay reproduces.

Blocks — the parts of an assistant message — are written neutrally:

```json
{ "type": "text", "text": "…" }
{ "type": "thinking", "text": "…" }
{ "type": "tool_call", "name": "read", "arguments": { "path": "a.md" } }
```

## Fixture kinds

Every file is self-describing: `format`, `formatVersion` (currently 1),
`name`, and `why` — one sentence on the behaviour it pins, so a failing replay
explains itself.

- **`oqto-chat-stream`** — `events` → `expect.blocks`. How deltas, which can
  interleave across blocks, assemble into an assistant message. Blocks are
  compared in order; tool calls by `name` only.

- **`oqto-chat-presentation`** — an assistant message's `blocks` and a detail
  `level` (1 minimal, 3 everything) → `expect.answer`, the texts that read as
  the answer, and at level 1 `expect.steps`, the labels of the collapsed
  activity list. Text is compared after trimming and collapsing whitespace.

- **`oqto-chat-user-message`** — a user turn's `text` → `expect.text`, what the
  reader sees, and `expect.attachments`, the workspace paths shown as
  attachments instead of inline.

## Adding a fixture

Write the fixture before the behaviour, in the client you are changing. The
other client's replay will fail until it implements the same rule, which is
the point: the failure is the ticket.

Never edit a fixture to make a client pass. If the expectation is wrong,
change it deliberately, in its own commit, with the reason in `why`.
