# Fragmented 312: Understanding Pi Durable (Mario Zechner, Armin Ronacher)

Sources: Fragmented podcast episode 312 (YouTube `7mBQknCnovM`; audio from the
public Simplecast feed, transcribed locally with whisper base.en, so names may
be misspelled) and Earendil's written companion
<https://earendil.com/posts/pi-durable/> (2026-10-01). Read 2026-10-22 for
`oqto-1b8f`. Statements are the speakers' views, not commitments.

## What matters for oqt

1. **Pi is moving onto Durable; networked plugins are the hard part.** "We are
   building this already." Durable is deliberately not a server: watches and an
   inbox let a server and a client each be ~100 lines. Earendil is building
   *chord*: networked plugins with parts in the agent host, web UI and TUI,
   versioned contracts, a known-good active plugin set, per-object hot reload.
   "Mostly done, but not battle-tested." An official GUI is coming, not on Pi 1.0.
   Consequence: host2 on chord2 stays thin and converges with upstream (ADR-0071).
2. **Clients must not run host-supplied code without isolation.** In multiplayer
   the server ships client plugins; a TUI client is a Node process, so a hostile
   server owner can reach the joining user's files and network. Joining a session
   must never expose the joiner's credentials. oqt rule: no host-supplied code in
   any oqt client (TUI, Mac/Linux peers, Herdr messaging) unless isolated.
   Tracked separately.
3. **Brain and hands on different machines is intended.** Execution is "clearly
   delineated from where the harness sits"; schedule on one machine and "reach
   with your arms to some other box". The `env` function builds an execution
   environment per tool call (written companion). Supports Mac brain + Linux
   hands through the runner (`oqto-49bm.1`).
4. **Recovery model.** Tasks are explicit phases, each an effect sandwich: store
   intent, perform the effect, store the result. No deterministic replay (Temporal
   style), because LLM-written code handles explicit state better. Read, write and
   edit are replay-safe; bash is not: after a crash the model is told the call was
   interrupted and inspects state. Matches oqt's uncertain/no-replay rule. A
   `requestId` makes submissions exactly-once (`oqto-1b8f.2`); approvals belong in
   memos or ledger, never re-asked or widened on recovery (`oqto-1b8f.3`).
5. **Subagents are owned conversations** in one ownership tree, scheduled like any
   other work and resumed after restart (32 in one process). Basis for isolated
   subagents and `/rewrite-session` (`oqto-49bm`).
6. **Debugging and evals.** Fork a problematic trajectory at the failure point and
   sample the loop ~50 times, optionally with an injected message. Durable forks
   make this cheap; use it for instruction-efficacy evals.
7. **Self-improving harnesses** need a precise goal and validation oracle; a
   harness rewriting itself loses the shape observability and UIs depend on.
   Self-extension through versioned hot reload is the safer form.

Also noted: MCP joined Pi via codemode (classifier and image models were the real
motivation); the Pi extension API stays frozen for stability, which is why new
work lands in Durable.
