# OpenCode SDK Reference

Condensed reference for integrating with the OpenCode API server.

## IMPORTANT: Common Pitfalls

1. **Endpoint is `/message` (singular)** - NOT `/messages` (plural)
2. **Messages endpoint returns `{ info: Message, parts: Part[] }[]`** - NOT a flat array of messages
3. **Timestamps are Unix milliseconds** - `time.created` and `time.updated` are ms since epoch
4. **Parts contain the text content** - Message text is in `parts` array with `type: "text"`

## Quick Start

```bash
npm install @opencode-ai/sdk
```

```typescript
import { createOpencodeClient } from "@opencode-ai/sdk"

const client = createOpencodeClient({
  baseUrl: "http://localhost:4096",  // or NEXT_PUBLIC_OPENCODE_BASE_URL
})
```

## Core Data Types

### Session

```typescript
type Session = {
  id: string
  projectID: string
  directory: string
  parentID?: string
  title: string
  version: string
  time: {
    created: number   // Unix timestamp ms
    updated: number   // Unix timestamp ms
    compacting?: number
  }
  summary?: {
    additions: number | null
    deletions: number | null
    files: number
    diffs?: FileDiff[]
  }
  share?: { url: string }
  revert?: {
    messageID: string
    partID?: string
    snapshot?: string
    diff?: string
  }
}
```

### SessionStatus

```typescript
type SessionStatus =
  | { type: "idle" }
  | { type: "retry"; attempt: number; message: string; next: number }
  | { type: "busy" }
```

### Message

```typescript
type UserMessage = {
  id: string
  sessionID: string
  role: "user"
  time: { created: number }
  agent: string
  model: { providerID: string; modelID: string }
  summary?: { title?: string; body?: string; diffs: FileDiff[] }
  system?: string
  tools?: { [key: string]: boolean }
}

type AssistantMessage = {
  id: string
  sessionID: string
  role: "assistant"
  time: { created: number; completed?: number }
  parentID: string
  modelID: string
  providerID: string
  mode: string
  path: { cwd: string; root: string }
  cost: number
  tokens: {
    input: number
    output: number
    reasoning: number
    cache: { read: number; write: number }
  }
  error?: ProviderAuthError | UnknownError | MessageOutputLengthError | MessageAbortedError | ApiError
  finish?: string
  summary?: boolean
}

type Message = UserMessage | AssistantMessage
```

### Part Types

```typescript
type Part =
  | TextPart
  | ReasoningPart
  | FilePart
  | ToolPart
  | StepStartPart
  | StepFinishPart
  | SnapshotPart
  | PatchPart
  | AgentPart
  | RetryPart
  | CompactionPart
  | SubtaskPart

type TextPart = {
  id: string
  sessionID: string
  messageID: string
  type: "text"
  text: string
  synthetic?: boolean
  ignored?: boolean
  time?: { start: number; end?: number }
  metadata?: { [key: string]: unknown }
}

type ToolPart = {
  id: string
  sessionID: string
  messageID: string
  type: "tool"
  callID: string
  tool: string
  state: ToolStatePending | ToolStateRunning | ToolStateCompleted | ToolStateError
  metadata?: { [key: string]: unknown }
}

type FilePart = {
  id: string
  sessionID: string
  messageID: string
  type: "file"
  mime: string
  filename?: string
  url: string
  source?: FileSource | SymbolSource
}
```

### Input Part Types (for sending messages)

```typescript
type TextPartInput = { type: "text"; text: string; id?: string; synthetic?: boolean; ignored?: boolean }
type FilePartInput = { type: "file"; mime: string; url: string; id?: string; filename?: string }
type AgentPartInput = { type: "agent"; name: string; id?: string }
type SubtaskPartInput = { type: "subtask"; prompt: string; description: string; agent: string; id?: string }
```

## API Endpoints

### Sessions

| Method | Path | Description | Body/Query | Response |
|--------|------|-------------|------------|----------|
| GET | `/session` | List all sessions | - | `Session[]` |
| POST | `/session` | Create session | `{ parentID?, title? }` | `Session` |
| GET | `/session/status` | Get all session statuses | - | `{ [id]: SessionStatus }` |
| GET | `/session/:id` | Get session | - | `Session` |
| DELETE | `/session/:id` | Delete session | - | `boolean` |
| PATCH | `/session/:id` | Update session | `{ title? }` | `Session` |
| GET | `/session/:id/children` | Get child sessions | - | `Session[]` |
| GET | `/session/:id/todo` | Get todo list | - | `Todo[]` |
| POST | `/session/:id/fork` | Fork session | `{ messageID? }` | `Session` |
| POST | `/session/:id/abort` | Abort running session | - | `boolean` |
| POST | `/session/:id/share` | Share session | - | `Session` |
| DELETE | `/session/:id/share` | Unshare session | - | `Session` |
| GET | `/session/:id/diff` | Get file diffs | `?messageID` | `FileDiff[]` |
| POST | `/session/:id/revert` | Revert message | `{ messageID, partID? }` | `boolean` |
| POST | `/session/:id/unrevert` | Restore reverted | - | `boolean` |

### Messages

| Method | Path | Description | Body | Response |
|--------|------|-------------|------|----------|
| GET | `/session/:id/message` | List messages | `?limit` | `{ info: Message, parts: Part[] }[]` |
| POST | `/session/:id/message` | Send message (sync) | See below | `{ info: Message, parts: Part[] }` |
| GET | `/session/:id/message/:msgId` | Get message | - | `{ info: Message, parts: Part[] }` |
| POST | `/session/:id/prompt_async` | Send message (async) | Same as message | `204 No Content` |
| POST | `/session/:id/command` | Execute slash command | `{ command, arguments, agent?, model? }` | `{ info: Message, parts: Part[] }` |
| POST | `/session/:id/shell` | Run shell command | `{ agent, model?, command }` | `Message` |

**Message Body Schema:**
```typescript
{
  messageID?: string
  model?: { providerID: string; modelID: string }
  agent?: string
  noReply?: boolean  // Context injection without AI response
  system?: string
  tools?: { [key: string]: boolean }
  parts: Array<TextPartInput | FilePartInput | AgentPartInput | SubtaskPartInput>
}
```

### Files

| Method | Path | Description | Query | Response |
|--------|------|-------------|-------|----------|
| GET | `/find` | Search text in files | `pattern` | `Match[]` |
| GET | `/find/file` | Find files by name | `query` | `string[]` |
| GET | `/find/symbol` | Find workspace symbols | `query` | `Symbol[]` |
| GET | `/file` | List files/directories | `path` | `FileNode[]` |
| GET | `/file/content` | Read file content | `path` | `FileContent` |
| GET | `/file/status` | Get tracked file status | - | `File[]` |

### Config & Provider

| Method | Path | Description | Response |
|--------|------|-------------|----------|
| GET | `/config` | Get config info | `Config` |
| PATCH | `/config` | Update config | `Config` |
| GET | `/config/providers` | List providers | `{ providers: Provider[], default: {...} }` |
| GET | `/provider` | List all providers | `{ all: Provider[], default: {...}, connected: string[] }` |
| GET | `/provider/auth` | Get auth methods | `{ [providerID]: ProviderAuthMethod[] }` |

### Project & Path

| Method | Path | Description | Response |
|--------|------|-------------|----------|
| GET | `/project` | List all projects | `Project[]` |
| GET | `/project/current` | Get current project | `Project` |
| GET | `/path` | Get current path | `Path` |
| GET | `/vcs` | Get VCS info | `VcsInfo` |

### TUI Control

| Method | Path | Description |
|--------|------|-------------|
| POST | `/tui/append-prompt` | Append text to prompt `{ text }` |
| POST | `/tui/submit-prompt` | Submit current prompt |
| POST | `/tui/clear-prompt` | Clear prompt |
| POST | `/tui/execute-command` | Execute command `{ command }` |
| POST | `/tui/show-toast` | Show notification `{ title?, message, variant, duration? }` |
| POST | `/tui/open-help` | Open help dialog |
| POST | `/tui/open-sessions` | Open session selector |
| POST | `/tui/open-themes` | Open theme selector |
| POST | `/tui/open-models` | Open model selector |

### Auth

| Method | Path | Description | Body |
|--------|------|-------------|------|
| PUT | `/auth/:id` | Set auth credentials | `{ type: "api", key }` or `{ type: "oauth", refresh, access, expires }` |

### Events (SSE)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/event` | Subscribe to SSE event stream |
| GET | `/global/event` | Global events (SSE stream) |

## Event Types

### Session Events

```typescript
type EventSessionCreated = { type: "session.created"; properties: { info: Session } }
type EventSessionUpdated = { type: "session.updated"; properties: { info: Session } }
type EventSessionDeleted = { type: "session.deleted"; properties: { info: Session } }
type EventSessionStatus = { type: "session.status"; properties: { sessionID: string; status: SessionStatus } }
type EventSessionIdle = { type: "session.idle"; properties: { sessionID: string } }
type EventSessionCompacted = { type: "session.compacted"; properties: { sessionID: string } }
type EventSessionDiff = { type: "session.diff"; properties: { sessionID: string; diff: FileDiff[] } }
type EventSessionError = { type: "session.error"; properties: { sessionID?: string; error?: Error } }
```

### Message Events

```typescript
type EventMessageUpdated = { type: "message.updated"; properties: { info: Message } }
type EventMessageRemoved = { type: "message.removed"; properties: { sessionID: string; messageID: string } }
type EventMessagePartUpdated = { type: "message.part.updated"; properties: { part: Part; delta?: string } }
type EventMessagePartRemoved = { type: "message.part.removed"; properties: { sessionID: string; messageID: string; partID: string } }
```

### Other Events

```typescript
type EventServerConnected = { type: "server.connected"; properties: { [key: string]: unknown } }
type EventPermissionUpdated = { type: "permission.updated"; properties: Permission }
type EventPermissionReplied = { type: "permission.replied"; properties: { sessionID: string; permissionID: string; response: string } }
type EventFileEdited = { type: "file.edited"; properties: { file: string } }
type EventFileWatcherUpdated = { type: "file.watcher.updated"; properties: { file: string; event: "add" | "change" | "unlink" } }
type EventTodoUpdated = { type: "todo.updated"; properties: { sessionID: string; todos: Todo[] } }
type EventVcsBranchUpdated = { type: "vcs.branch.updated"; properties: { branch?: string } }
type EventInstallationUpdated = { type: "installation.updated"; properties: { version: string } }
```

## Usage Examples

### Fetch Sessions

```typescript
const res = await fetch(`${baseUrl}/session`)
const sessions: Session[] = await res.json()
```

### Fetch Messages for Session

```typescript
const res = await fetch(`${baseUrl}/session/${sessionId}/message`)
const messages: { info: Message; parts: Part[] }[] = await res.json()
```

### Send a Message

```typescript
const res = await fetch(`${baseUrl}/session/${sessionId}/message`, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    model: { providerID: "anthropic", modelID: "claude-sonnet-4-20250514" },
    parts: [{ type: "text", text: "Hello!" }],
  }),
})
const result = await res.json()
```

### Context Injection (No AI Response)

```typescript
await fetch(`${baseUrl}/session/${sessionId}/message`, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    noReply: true,
    parts: [{ type: "text", text: "You are a TypeScript expert." }],
  }),
})
```

### Subscribe to Events (SSE)

```typescript
const source = new EventSource(`${baseUrl}/event`)

source.onmessage = (event) => {
  const parsed = JSON.parse(event.data)
  console.log("Event:", parsed.type, parsed.properties)
  
  if (parsed.type === "session.updated") {
    // Refresh sessions list
  }
  if (parsed.type === "message.part.updated") {
    // Update message display
  }
}

source.onerror = (err) => {
  console.error("SSE error", err)
}

// Cleanup
source.close()
```

### Using the SDK Client

```typescript
import { createOpencodeClient } from "@opencode-ai/sdk"

const client = createOpencodeClient({ baseUrl: "http://localhost:4096" })

// List sessions
const sessions = await client.session.list()

// Send prompt
const result = await client.session.prompt({
  path: { id: sessionId },
  body: {
    model: { providerID: "anthropic", modelID: "claude-sonnet-4-20250514" },
    parts: [{ type: "text", text: "Write hello world in TypeScript" }],
  },
})

// Subscribe to events
const events = await client.event.subscribe()
for await (const event of events.stream) {
  console.log("Event:", event.type)
}
```

## Server Configuration

Default server: `http://127.0.0.1:4096`

```bash
# Start standalone server
opencode serve --port 4096 --hostname 127.0.0.1

# View OpenAPI spec
open http://localhost:4096/doc
```

## SDKs Available

| Language | Package |
|----------|---------|
| JavaScript/TypeScript | `@opencode-ai/sdk` (npm) |
| Go | `github.com/sst/opencode-sdk-go` |
| Python | `opencode-ai` (PyPI) |

## Key Notes

1. **Timestamps are Unix ms** - `time.created` and `time.updated` are milliseconds since epoch
2. **Use SSE for real-time** - Subscribe to `/event` for live updates instead of polling
3. **Handle permissions** - Listen for `permission.updated` events and respond via `/session/:id/permissions/:permissionID`
4. **Use `noReply: true`** - For context injection without triggering AI response
5. **Check session status** - Use `session.status` events to know when busy/idle/retrying
6. **Messages have parts** - Content is in `parts` array, not directly on message
