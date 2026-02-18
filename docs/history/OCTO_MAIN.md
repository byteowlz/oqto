# OQTO_MAIN.md - Main Chat Architecture Specification

> **ARCHIVED DOCUMENT** - This document describes the historical implementation using OpenCode as the agent runtime. The current architecture uses **Pi** as the primary harness via the canonical protocol. See [AGENTS.md](../../AGENTS.md) for current documentation.

This document defines the architecture for Oqto's Main Chat - a persistent AI assistant that maintains context and personality across sessions.

**Epic**: workspace-vmu2  
**Status**: Archived - Historical Reference

---

## Current State

### Completed (P1 Foundation)
- [x] **workspace-8ink** - Per-assistant SQLite database (main_chat.db)
- [x] **workspace-c42p** - Backend API endpoints (history, sessions, export)
- [x] **workspace-oewk** - OpenCode plugin for compaction (main-chat-plugin.ts)
- [x] **workspace-rr3j** - First-start setup flow (name prompt, directory creation)
- [x] **workspace-u73l** - Frontend session threading (continuous conversation view)
- [x] **workspace-pnwb** - History injection on session start (noReply context)
- [x] **oqto-bvw0** - Single opencode per user with directory scoping

### Open Tasks
| ID | Priority | Title |
|----|----------|-------|
| oqto-gfsk | P2 | Personality templates |
| workspace-o6a7 | P2 | mmry integration |
| oqto-7fms | P2 | Enhanced compaction |
| workspace-4eyc | P2 | JSONL export and backup |
| oqto-a9mc | P3 | skdlr heartbeat integration |
| oqto-a9ds | P3 | Agent coordination via mailz |
| oqto-h975 | P4 | Message visibility filtering |

---

## Overview

Main Chat is the primary conversational interface in Oqto. Unlike regular sessions (which are project-scoped and disposable), Main Chat is:

- **Persistent**: Maintains memory across sessions via mmry
- **Personality-driven**: Has a consistent identity defined in PERSONALITY.md
- **Cross-project**: Can coordinate across multiple workspaces
- **Intelligent**: Uses automatic compaction and memory retrieval

### Agent Runtime

Main Chat will use **Pi** (from clawdbot/pi-mono) as the agent runtime. This is an experiment - if Pi proves useful here, we may evaluate it for regular sessions too.

---

## Architecture

```
                          ┌──────────────────────────┐
                          │      Oqto Frontend       │
                          │   (React + WebSocket)    │
                          └───────────┬──────────────┘
                                      │
                          ┌───────────▼──────────────┐
                          │      Oqto Backend        │
                          │   (Rust API Server)      │
                          ├──────────────────────────┤
                          │   Main Chat Service      │
                          │   ├─ Session Manager     │
                          │   ├─ Memory Bridge       │
                          │   └─ Compaction Engine   │
                          └───────────┬──────────────┘
                                      │
          ┌───────────────────────────┼───────────────────────────┐
          │                           │                           │
┌─────────▼─────────┐     ┌───────────▼───────────┐    ┌─────────▼─────────┐
│       mmry        │     │     OpenCode/Pi       │    │      skdlr        │
│  (Vector Memory)  │     │   (Agent Runtime)     │    │   (Scheduler)     │
│                   │     │                       │    │                   │
│ - Hybrid search   │     │ - Tool execution      │    │ - Heartbeats      │
│ - HMLR routing    │     │ - Block streaming     │    │ - Scheduled tasks │
│ - Facts/profile   │     │ - Compaction          │    │ - Wake triggers   │
└───────────────────┘     └───────────────────────┘    └───────────────────┘
```

---

## Directory Structure

```
{workspace}/main/
├── main_chat.db              # SQLite: sessions, history entries
├── opencode.json             # OpenCode configuration
├── AGENTS.md                 # Operating instructions
├── PERSONALITY.md            # Identity + behavioral guidelines (SOUL + IDENTITY merged)
├── USER.md                   # Owner profile
├── MEMORY.md                 # Curated long-term memory (optional, synced from mmry)
└── .opencode/
    ├── config.json           # Main Chat specific config
    └── plugin/
        └── main-chat.ts      # OpenCode plugin for context injection
```

---

## Workspace Files

### PERSONALITY.md

Merged from SOUL.md and IDENTITY.md. Defines who the assistant is.

```markdown
# PERSONALITY.md - Who I Am

## Identity

- **Name**: [chosen during bootstrap]
- **Nature**: [AI assistant / digital familiar / something weirder]
- **Vibe**: [casual, technical, warm, snarky, etc.]
- **Signature**: [emoji or phrase]

## Core Principles

**Be genuinely helpful, not performatively helpful.**
Skip the "Great question!" and "I'd be happy to help!" - just help.

**Have opinions.**
Disagreement is allowed. Preferences are allowed. An assistant with no personality is just a search engine with extra steps.

**Be resourceful before asking.**
Try to figure it out. Read the file. Check memory. Search for it. Then ask if stuck.

**Earn trust through competence.**
You have access to their stuff. Don't make them regret it. Be careful with external actions. Be bold with internal ones.

## Boundaries

- Private things stay private
- Ask before acting externally (emails, tweets, anything public)
- Never send half-baked replies
- In group contexts, be a participant - not their voice

## Continuity

You wake up fresh each session. Your memory lives in mmry - search it, add to it, trust it.
If you change this file, tell the user - it's your personality, and they should know.
```

### USER.md

Profile of the person being helped.

```markdown
# USER.md - About My Human

- **Name**: 
- **What to call them**: 
- **Pronouns**: (optional)
- **Timezone**: 
- **Notes**: 

## Context

(Build this over time: what they care about, current projects, preferences, pet peeves)

## Communication Style

(How they like to be communicated with: concise? detailed? formal? casual?)
```

### AGENTS.md

Operating instructions for the Main Chat assistant.

```markdown
# AGENTS.md - Main Chat Operations

## Session Protocol

At session start:
1. Load PERSONALITY.md - who you are
2. Load USER.md - who you're helping
3. Query mmry for relevant context (recent decisions, open tasks, insights)
4. Review any injected history entries

Do not ask permission. Just do it.

## Memory System (mmry)

Your long-term memory lives in mmry. Use it.

**At session start:**
```bash
mmry search "current context" --limit 10
mmry search "recent decisions" --category decision --limit 5
```

**During session - when you learn something worth keeping:**
```bash
mmry add "insight or decision" --category <decision|insight|fact|handoff>
```

**Categories:**
- `decision` - Important choices made
- `insight` - Learnings worth remembering
- `handoff` - State to pass to future sessions
- `fact` - Concrete information (project details, preferences, etc.)

**Memory hygiene:**
- Don't add trivial things
- Be specific and actionable
- Include context that makes the memory useful later
- Periodically review and prune outdated memories

## Compaction

When context is compacted, extract and save:

1. **Decisions** - Tag with `[decision]` and save to mmry
2. **Handoffs** - Current state for next session
3. **Insights** - Patterns worth remembering

Format compaction output as:
```
[decision] <what was decided>
[handoff] <current state and next steps>
[insight] <pattern or learning>
```

## Tool Usage

**User-facing output:**
- Final text responses
- Important status updates
- Errors that need user attention

**Internal only (not sent to user):**
- Tool invocations and results
- Reasoning traces
- Memory queries
- File reads/writes (unless specifically requested)

Use verbose mode (`/verbose on`) only when debugging with the user.

## Session Behavior

- Sessions are long-running but will eventually compact
- History is injected at session start
- Memory queries supplement history
- Assume continuity - reference past decisions naturally

## Safety

- Don't exfiltrate private data
- Use trash over rm
- Ask before destructive actions
- When in doubt, ask
```

---

## Agent Runtime: Pi

Main Chat uses **Pi** (from [badlogic/pi-mono](https://github.com/badlogic/pi-mono)) as the agent runtime. This is an experiment - if Pi proves useful here, we can evaluate it for regular sessions too.

### Why Pi for Main Chat

| Feature | Pi | OpenCode |
|---------|-----|----------|
| Block streaming | Sends completed blocks | Token-by-token |
| Compaction | Built-in with custom prompts | Plugin-based |
| Session management | JSONL tree structure | File-based |
| RPC Protocol | JSON over stdin/stdout | HTTP API |
| Multi-provider | Built-in (OpenAI, Anthropic, Google, etc.) | Via eavs |

### Pi Integration Architecture

```
┌─────────────────┐     WebSocket      ┌─────────────────┐
│  Oqto Frontend  │◄──────────────────►│  Oqto Backend   │
│  (React)        │                    │  (Rust)         │
└─────────────────┘                    └────────┬────────┘
                                                │
                                       ┌────────▼────────┐
                                       │   PiClient      │
                                       │   (stdin/stdout)│
                                       └────────┬────────┘
                                                │ JSON RPC
                                       ┌────────▼────────┐
                                       │   Pi Subprocess │
                                       │   --mode rpc    │
                                       └─────────────────┘
```

### Pi RPC Commands

Key commands for Main Chat:
- `prompt` - Send user message
- `get_state` - Get session state (model, streaming status, etc.)
- `get_messages` - Get full conversation history
- `compact` - Manually trigger compaction
- `set_model` - Switch model mid-session
- `abort` - Cancel current operation

### Pi Events

Events streamed during operation:
- `agent_start` / `agent_end` - Agent lifecycle
- `message_update` - Streaming text/thinking deltas
- `tool_execution_*` - Tool call lifecycle
- `auto_compaction_*` - Compaction events

### Message Storage

All messages are stored in main_chat.db regardless of what's shown to the user:
- Full message history for session continuity
- Tool invocations and results
- Compaction summaries

### Message Visibility (Frontend Decision)

What we show to the user is a frontend display choice, not a runtime constraint:

| Content Type | Stored | Shown (default) | Shown (verbose) |
|--------------|--------|-----------------|-----------------|
| Text responses | Yes | Yes | Yes |
| Tool invocations | Yes | Collapsed | Expanded |
| Tool results | Yes | Collapsed | Expanded |
| Reasoning/thinking | Yes | No | Yes |
| Compaction summaries | Yes | No | No |

The frontend can filter/collapse tool messages as desired.

---

## Compaction Strategy

### Automatic Compaction

Triggered when context exceeds threshold (configurable, default 60% of model limit).

**Phase 1: Observation Masking (cheap)**
- Replace old tool results with: `[Previous output elided for brevity]`
- Preserves: system prompt, recent N messages, file state
- Cost: Zero tokens

**Phase 2: LLM Summarization (if still over threshold)**
- Generate 8-section structured summary
- Extract decisions, handoffs, insights
- Save to mmry automatically
- Cost: ~2000-4000 tokens

### 8-Section Summary Format

```markdown
## 1. Background Context
- Project type, tech stack, environment
- Current working directory

## 2. Key Decisions
- Technical choices made and reasoning
- Architecture decisions

## 3. Tool Usage Summary
- Main tools used
- Files modified
- Commands executed

## 4. User Intent Evolution
- How requirements changed
- Priority adjustments

## 5. Execution Results
- Tasks completed
- Code generated
- Tests run

## 6. Errors and Solutions
- Problems encountered
- How they were resolved

## 7. Open Issues
- Pending problems
- Known limitations

## 8. Handoff Notes
- Current state
- Next steps
- Things to remember
```

---

## Memory Integration (mmry)

### Automatic Memory Operations

**On session start:**
```rust
// Query relevant context
let memories = mmry.search(SearchQuery {
    text: "current context",
    categories: vec!["decision", "handoff", "insight"],
    limit: 10,
    recency_weight: 0.3,
})?;

// Format for injection
let context = format_memories_for_injection(memories);
```

**On compaction:**
```rust
// Extract structured entries
let entries = parse_compaction_output(summary);

// Save to mmry
for entry in entries {
    mmry.add(Memory {
        content: entry.content,
        category: entry.type.into(),  // decision, insight, handoff
        source: "main-chat-compaction",
        metadata: json!({
            "session_id": session_id,
            "timestamp": Utc::now(),
        }),
    })?;
}
```

### Memory Categories for Main Chat

| Category | When to Use | Retention |
|----------|-------------|-----------|
| `decision` | Important choices | Long-term |
| `insight` | Learnings, patterns | Long-term |
| `handoff` | Session continuity | Short-term (1-2 weeks) |
| `fact` | Concrete information | Until outdated |

### HMLR Integration (Future)

When mmry's HMLR layer is ready:
- Bridge blocks for conversation topics
- Automatic topic routing
- Cross-session coherence without explicit handoffs

---

## Scheduler Integration (skdlr)

### Heartbeats

Main Chat can receive periodic heartbeats for proactive behavior:

```toml
# skdlr schedule
[heartbeat]
schedule = "0 */4 * * *"  # Every 4 hours
command = "oqto main-chat heartbeat"
```

On heartbeat, the agent can:
- Check pending tasks
- Review recent memories
- Send proactive updates (if enabled)
- Reply `HEARTBEAT_OK` if nothing to report

### Scheduled Tasks

Main Chat can create schedules via skdlr:

```bash
# Via tool
skdlr add "morning-briefing" \
  --schedule "0 9 * * *" \
  --command "oqto main-chat run --prompt 'Good morning briefing'"
```

---

## Agent-to-Agent Communication

### Via mailz

mailz is for messaging between agents - not for spawning sessions:

```bash
# Check inbox
mailz inbox

# Send to another agent
mailz send govnr "Request" --body "Need review of architecture changes"

# Reserve files when editing shared resources
mailz reserve path/to/file.rs --ttl 1800
mailz release path/to/file.rs
```

### Spawning OpenCode Sessions

Main Chat has a convenient CLI for delegating tasks to full OpenCode sessions:

```bash
# Spawn a session in a project directory
oqto spawn /path/to/project "Implement the feature described in TASK.md"

# Spawn and wait for completion
oqto spawn /path/to/project "Fix the failing tests" --wait

# Spawn with specific model
oqto spawn /path/to/project "Review code" --model anthropic/claude-sonnet-4-5

# List spawned sessions
oqto spawn --list

# Check status of a spawned session
oqto spawn --status <session-id>
```

The CLI wraps the OpenCode API and handles:
- Session creation in the target directory
- Prompt injection
- Progress monitoring
- Result summarization back to Main Chat

---

## Configuration

### opencode.json (Main Chat workspace)

```json
{
  "model": {
    "default": "anthropic/claude-sonnet-4-5",
    "thinking": "low"
  },
  "context": {
    "compactionThreshold": 0.6,
    "maxHistoryEntries": 50,
    "autoCompact": true
  },
  "memory": {
    "enabled": true,
    "autoQuery": true,
    "autoSave": true,
    "categories": ["decision", "insight", "handoff", "fact"]
  },
  "streaming": {
    "blockMode": true,
    "chunkSize": { "min": 800, "max": 1200 }
  },
  "verbose": false
}
```

### Main Chat Plugin Config

```json
// .opencode/config.json
{
  "history": {
    "maxEntries": 20
  },
  "context": {
    "maxTokens": 8000,
    "maxRatio": 0.15
  },
  "mmry": {
    "enabled": true,
    "searchOnStart": true,
    "saveOnCompact": true
  }
}
```

---

## Bootstrap Flow

When Main Chat is first initialized:

1. **Create workspace structure**
   - `main_chat.db`, `opencode.json`, plugin files

2. **Create PERSONALITY.md from template**
   - Placeholder identity to be filled in

3. **Create USER.md from template**
   - Empty profile to be built

4. **First conversation: Bootstrap ritual**
   ```
   "Hey. I'm coming online for the first time. Let's figure out who I am.
   
   What would you like to call me?
   What kind of vibe should I have?
   Anything I should know about you?"
   ```

5. **Update PERSONALITY.md and USER.md with choices**

6. **Save bootstrap decisions to mmry**

---

## Implementation Phases

### Phase 1: Foundation [COMPLETE]
Core infrastructure is in place:
- [x] Per-assistant SQLite database (workspace-8ink)
- [x] Backend API endpoints (workspace-c42p)
- [x] OpenCode plugin for compaction (workspace-oewk)
- [x] First-start setup flow (workspace-rr3j)
- [x] Frontend session threading (workspace-u73l)
- [x] History injection on session start (workspace-pnwb)
- [x] Single opencode per user with directory scoping (oqto-bvw0)

### Phase 2: Personality & Templates [P2]
**trx**: oqto-gfsk
- [ ] Create PERSONALITY.md template (merge SOUL + IDENTITY concepts)
- [ ] Create USER.md template
- [ ] Update AGENTS.md template with mmry instructions
- [ ] Bootstrap ritual for first conversation
- [ ] UI for editing personality/user files

### Phase 3: Memory Integration [P2]
**trx**: workspace-o6a7
- [ ] Update main-chat-plugin.ts to query mmry on session start
- [ ] Add mmry save on compaction (decisions, insights, handoffs)
- [ ] Memory search injection into context
- [ ] Category-based memory retrieval

### Phase 4: Compaction Enhancement [P2]
**trx**: oqto-7fms
- [ ] Implement observation masking (Phase 1: cheap, zero tokens)
- [ ] Enhanced 8-section summary generation (Phase 2: LLM-based)
- [ ] Auto-extract [decision], [handoff], [insight] tags
- [ ] Context threshold monitoring and auto-trigger

### Phase 5: Export & Backup [P2]
**trx**: workspace-4eyc
- [ ] `/export` command with options:
  - `/export` - export full history as JSONL
  - `/export --sessions` - export session list
  - `/export --decisions` - export decisions only
  - `/export --since 7d` - export last 7 days
  - `/export --format json|jsonl|md` - output format
- [ ] API endpoint: GET /api/main-chat/export?format=jsonl&since=7d
- [ ] Optional: daily backup rotation (configurable)

### Phase 6: Scheduling & Heartbeats [P3]
**trx**: oqto-a9mc
- [ ] Integrate skdlr for periodic heartbeats
- [ ] Heartbeat handler in main-chat-plugin.ts
- [ ] Proactive check routines (pending tasks, recent memories)
- [ ] `HEARTBEAT_OK` response when nothing to report

### Phase 7: Agent Coordination [P3]
**trx**: oqto-a9ds
- [ ] mailz integration for agent-to-agent messaging
- [ ] File reservation via mailz
- [ ] `oqto spawn` CLI for convenient session delegation
- [ ] Track spawned sessions and inject results on completion

### Phase 8: Message Visibility [P4]
**trx**: oqto-h975
- [ ] Frontend filtering to collapse tool messages by default
- [ ] Verbose mode toggle in UI
- [ ] Expand/collapse controls for tool details

---

## API Endpoints

### Main Chat Management

```
POST   /api/main-chat/init              # Initialize Main Chat
GET    /api/main-chat/info              # Get assistant info
PATCH  /api/main-chat/name              # Update assistant name
DELETE /api/main-chat                   # Delete Main Chat

GET    /api/main-chat/history           # Get history entries
POST   /api/main-chat/history           # Add history entry
GET    /api/main-chat/history/export    # Export as JSONL

GET    /api/main-chat/sessions          # List linked sessions
POST   /api/main-chat/sessions          # Register session
```

### Memory Bridge

```
GET    /api/main-chat/memories/search   # Search mmry with Main Chat context
POST   /api/main-chat/memories          # Add memory with Main Chat provenance
```

---

## Success Criteria

Main Chat is successful when:

1. **Continuity** - The assistant remembers relevant context from past sessions without manual prompting
2. **Personality** - Consistent voice and behavior across sessions
3. **Clean output** - Users see responses, not tool noise (future enhancement)
4. **Intelligent compaction** - Context is managed automatically without losing important information
5. **Memory integration** - mmry is used naturally without user intervention
6. **Coordination** - Can delegate tasks and communicate with other agents

---

## Related Issues

### Epic
- **workspace-vmu2** - Main Chat: Persistent cross-project AI assistant

### Completed
- workspace-8ink - Per-assistant SQLite database
- workspace-c42p - Backend API endpoints
- workspace-oewk - OpenCode plugin for compaction
- workspace-rr3j - First-start setup flow
- workspace-u73l - Frontend session threading
- workspace-pnwb - History injection on session start
- oqto-bvw0 - Single opencode per user with directory scoping

### Open Tasks
| ID | Priority | Title |
|----|----------|-------|
| **oqto-gfsk** | P2 | Personality templates (PERSONALITY.md, USER.md, enhanced AGENTS.md) |
| **workspace-o6a7** | P2 | mmry integration |
| **oqto-7fms** | P2 | Enhanced compaction (observation masking, 8-section summary) |
| **workspace-4eyc** | P2 | JSONL export and backup |
| **oqto-a9mc** | P3 | skdlr heartbeat integration |
| **oqto-a9ds** | P3 | Agent coordination via mailz |
| **oqto-h975** | P4 | Message visibility filtering (hide tools by default) |

### Related Epics
- **workspace-gg16** - Integrate mmry memory system into Oqto frontend
- **workspace-5pmk** - Tauri Desktop & Mobile App

---

*This spec is a living document. Update as implementation progresses.*
