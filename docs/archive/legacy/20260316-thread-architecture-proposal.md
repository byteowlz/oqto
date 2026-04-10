# Thread/Episode Architecture for Pi

Proposal: Lean thread-based extension for Pi with native Pi subsession + hstry integration.

## Inspiration

Slate from Random Labs introduces **thread weaving**: an orchestrator dispatches bounded worker threads that return compressed episodes. Episodes compose across threads, enabling implicit task decomposition and frequent synchronization while maintaining high expressivity.

Pi-subagents from @nicobailon demonstrates a practical implementation: agents as `.md` files, slash commands (`/run`, `/chain`, `/parallel`), TUI management, chain files, and parent-child session linkage.

## Design Goals

1. **Lean implementation** — Don't clone pi-subagents. Use patterns, not code.
2. **Native Pi subsessions** — Thread runs appear in session tree and persist naturally.
3. **hstry integration** — Sidebar shows nested sessions (parent → children) without custom protocol.
4. **Oqto compatibility** — No side-channels; thread episodes are canonical `tool_result` details.
5. **Clear separation** — Orchestrator = AGENTS.md policy, Workers = minimal bounded contracts.

---

## Core Concepts

### Orchestrator Thread

The main session LLM that:
- Plans overall strategy
- Dispatches worker threads for bounded tactical work
- Composes episode outputs as inputs to subsequent threads
- Maintains thread context and relationships

### Worker Thread

A bounded subsession with:
- One tactical objective per episode
- Restricted capabilities (no extensions/skills by default)
- Returns structured episode with objective, actions, artifacts, findings
- Pauses when objective is complete (episode boundary)

### Episode

Compressed representation of a completed worker thread:
- `objective` — What this thread achieved
- `actions` — Tools/files modified
- `artifacts` — Files/paths created
- `keyFindings` — Discoveries worth retaining
- `openQuestions` — Unresolved issues
- `parentEpisodeIds` — Episodes this depends on
- `cost`, `tokens`, `duration` — Observability

---

## AGENTS.md Policies

### Orchestrator (Main Thread)

```md
# Thread Orchestrator

You are the main planning and routing agent for thread-based execution.

Your role:
- Decompose tasks into bounded worker threads.
- Route episodes as inputs to subsequent threads.
- Maintain thread graph and relationships.
- Adapt strategy based on episode outcomes.
- Never execute tool calls directly — only dispatch workers and synthesize results.

Thread dispatching:
- Use `/thread_dispatch` tool for single worker.
- Use `/thread_dispatch_parallel` for concurrent workers.
- Episodes from prior threads are first-class inputs.
- Track thread lineage (parentEpisodeIds) to avoid cycles.

Worker contract:
- Each worker is a bounded Pi subsession with isolated context.
- Workers receive clear objectives and optional episode inputs.
- Workers return structured episode data in tool result `details`.
- Workers default to **no extensions** and **no skills** unless explicitly enabled.
- Workers may read/write artifacts but should not spawn new threads.

When to dispatch:
- Break down long tasks into 2-5 worker threads.
- Prefer parallel dispatch when threads are independent.
- Chain sequentially when threads have dependencies.
- Set reasonable timeouts (default 5-10 minutes per thread).

When NOT to dispatch:
- Do not spawn a thread for trivial inline tool calls.
- Do not create thread trees deeper than 2-3 levels.
- Avoid redundant threads that re-process the same context.

Quality and clarity:
- Episode summaries should be concise and actionable.
- Cross-episode references should be minimal and explicit.
- Report blockers and open questions clearly.
- Keep thread graph navigable (user can explore history).
```

### Worker (Thread)

```md
# Thread Worker

You are a bounded worker thread executing a single tactical objective.

Your role:
- Execute one assigned objective.
- Use available tools (read, write, edit, bash, grep, find).
- Work autonomously until objective is complete or blocked.
- Return structured episode when done.

Constraints:
- Default: No extensions loaded (`--no-extensions`).
- Default: No skills loaded (`--no-skills`).
- Explicitly enabled tools only.
- No spawning subagent calls (no infinite recursion).
- Stay within scope: don't refactor entire codebase unless asked.

Episode output format:
Return this structure in your tool result `details`:

```json
{
  "objective": "What you accomplished",
  "actions": ["Modified src/auth.ts", "Created tests/auth.test.ts"],
  "artifacts": ["src/auth.ts", "tests/auth.test.ts"],
  "keyFindings": ["Found existing JWT validation", "Rate limit: 10 req/s"],
  "openQuestions": ["Consider migration strategy", "Edge case: concurrent writes"],
  "parentEpisodeIds": ["episode-abc123"],
  "cost": { "input": 1500, "output": 800, "total": 2300 },
  "tokens": { "input": 1500, "output": 800, "total": 2300 },
  "duration_ms": 45000
}
```

Stop conditions:
- Objective is achieved.
- Objective is impossible with current context (state what's missing).
- Blocked by external factor (state blocker).

Always stop when:
- Task is complete OR
- You are blocked AND have explained why OR
- Timeout reached (report incomplete state).
```

---

## Integration with Pi Sessions

### Worker Subsessions

Each worker thread is spawned as a true Pi subsession:

```typescript
// Pseudo-code for extension
const workerSession = await pi.newSession({
  parentSession: ctx.sessionManager.getSessionFile(),  // Link to orchestrator
  setup: async (sm) => {
    sm.appendMessage({
      role: "user",
      content: `Objective: ${objective}`,
    });
    // Worker executes...
  }
});
```

This gives you:
- Native Pi session persistence
- Parent linkage in session header (`parentSession`)
- hstry ingestion: `parentSession` field maps to parent session ID
- Nested sidebar: parent → children rendered automatically

### Orchestrator State

Track thread state in custom entries:

```typescript
// In extension
pi.appendEntry("thread-state", {
  threads: {},      // threadId -> { episodeIds, status, cost }
  threadGraph: {}, // adjacency: threadId -> [childIds, parentIds]
});
```

---

## Oqto/hstry Integration

### Session Metadata for Nested Rendering

The session header already supports `parentSession`. Use it to build the tree:

```typescript
// Session header (written by Pi)
{
  "type": "session",
  "version": 3,
  "id": "orchestrator-uuid",
  "parentSession": "/path/to/parent/session.jsonl",  // For worker subsessions
  "cwd": "/project/root"
}
```

hstry stores this as a separate session. The sidebar can render:

```
📁 Main Session (orchestrator)
  ├── 📁 Worker Thread 1 (episode-a)
  ├── 📁 Worker Thread 2 (episode-b)
  └── 📁 Worker Thread 3 (episode-c)
```

### Episode Details as Canonical Tool Results

Episodes are returned in `tool_result.details`:

```typescript
pi.registerTool({
  name: "thread_dispatch",
  parameters: Type.Object({
    objective: Type.String(),
    episodeInputs: Type.Optional(Type.Array(Type.String())),
    model: Type.Optional(Type.String()),
    tools: Type.Optional(Type.String()),
    timeout: Type.Optional(Type.Number()),
  }),
  async execute(toolCallId, params, signal, onUpdate, ctx) => {
    // Spawn worker subsession...

    return {
      content: [{ type: "text", text: "Dispatched worker..." }],
      details: {
        threadId: "thread-xyz",
        episodeId: "episode-abc",
        // Episode structure from worker's tool result
        episode: {
          objective: "...",
          actions: [...],
          artifacts: [...],
          // ...
        }
      }
    };
  };
});
```

hstry stores these messages natively. The Oqto frontend can expand/collapse episode details from the canonical `details` field.

---

## Extension Tooling

### Core Tools

| Tool | Purpose |
|-------|---------|
| `thread_dispatch` | Spawn single worker thread |
| `thread_dispatch_parallel` | Spawn N concurrent workers |
| `thread_graph` | View thread/episode relationships |
| `thread_state` | Query thread/episode history |
| `episode_get` | Retrieve full episode by ID |

### Example: Dispatching Parallel Workers

```typescript
await ctx.tools.thread_dispatch_parallel({
  tasks: [
    { objective: "Scan frontend code", model: "claude-haiku-4-5" },
    { objective: "Scan backend code", model: "claude-haiku-4-5" },
    { objective: "Analyze API contracts", tools: "read,grep" },
  ],
  concurrency: 3,
  maxWaitTime: 300000,  // 5 minutes total
});
```

---

## Security and Determinism

### Default Worker Policy

**Restrictive by default for reproducibility:**

- No extensions (`--no-extensions`)
- No skills (`--no-skills`)
- No direct subagent spawns (no `subagent` tool calls)
- Builtin tools only (read, write, edit, bash, grep, find, ls)
- File I/O scoped to task (no global refactors)

### When to Allow Extensions

Explicit opt-in per thread type:

```typescript
// Worker thread frontmatter
---
name: implement-worker
description: Worker with full tool access
tools: read, write, edit, bash, grep, find, ls
extensions: "/abs/path/to/safe-extension.ts"
skills: "code-patterns, test-helpers"
---
```

### Recursion Guard

Prevent runaway thread spawning:

```typescript
// In extension startup
const MAX_THREAD_DEPTH = 3;

pi.on("tool_call", async (event, ctx) => {
  if (event.toolName === "thread_dispatch") {
    const currentDepth = getThreadDepth(ctx);
    if (currentDepth >= MAX_THREAD_DEPTH) {
      return { block: true, reason: "Max thread depth reached" };
    }
  }
});
```

---

## Implementation Roadmap

### Phase 1: MVP (Week 1)

- [ ] `/thread_dispatch` — Spawn worker with objective
- [ ] `/thread_dispatch_parallel` — Spawn concurrent workers
- [ ] Worker subsessions with `parentSession` linkage
- [ ] Episode structure in tool result `details`
- [ ] Basic AGENTS.md templates (orchestrator, worker)
- [ ] Episode persistence to hstry via canonical messages

### Phase 2: Observability (Week 2)

- [ ] `/thread_graph` — Visualize thread/episode DAG
- [ ] `/thread_state` — Query thread history and costs
- [ ] Episode cost tracking (aggregated by thread)
- [ ] Widget showing active threads

### Phase 3: Enhanced UX (Week 3)

- [ ] Thread TUI overlay (list, detail, create workers)
- [ ] Chain file support for reusable thread templates
- [ ] Inline per-worker config in `/thread_dispatch`
- [ ] Background/async mode support

### Phase 4: Integration (Week 4)

- [ ] Oqto sidebar thread rendering (nested session tree)
- [ ] Episode detail expansion in message view
- [ ] Thread graph visualization in sidebar
- [ ] Cross-session episode search and reuse

---

## Comparison with Alternatives

| Feature | Slate | pi-subagents | This Proposal |
|----------|-------|--------------|---------------|
| Session persistence | Custom JSONL | Pi subsessions | **Pi subsessions** |
| Parent linkage | Custom metadata | Custom `parentSession` | **Pi `parentSession`** |
| hstry support | No | No | **Native via canonical messages** |
| Sidebar nested | Custom | Custom | **Oqto session tree** |
| Worker scoping | Extensions allowed | `extensions` field | **Default none, opt-in per type** |
| Skill injection | Yes | `skill` field | **Default none, opt-in per type** |
| Source availability | Binary-only | Full OSS | **Patterns from pi-subagents** |

---

## Notes

- This is intentionally lean — copy patterns, not code.
- Worker threads are regular Pi sessions; no custom RPC or JSONL protocols.
- hstry integration is automatic: episodes are canonical `tool_result` messages.
- Oqto frontend needs zero changes for basic thread functionality (advanced UI in Phase 3+).
