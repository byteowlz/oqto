// Session type matching actual API response
export type OpenCodeSession = {
  id: string
  title: string
  time: {
    created: number
    updated: number
  }
  parentID?: string | null
  version?: string
  projectID?: string
  directory?: string
  summary?: {
    additions: number | null
    deletions: number | null
    files: number
  }
}

// Part types
export type OpenCodePart = {
  id: string
  sessionID: string
  messageID: string
  type: "text" | "tool" | "file" | "reasoning" | "step-start" | "step-finish" | "snapshot" | "patch" | "agent" | "retry" | "compaction" | "subtask"
  text?: string
  tool?: string
  callID?: string
  state?: {
    status: "pending" | "running" | "completed" | "error"
    input?: Record<string, unknown>
    output?: string
    title?: string
    time?: { start: number; end?: number }
  }
  time?: { start?: number; end?: number }
  metadata?: Record<string, unknown>
}

// Message types
export type OpenCodeUserMessage = {
  id: string
  sessionID: string
  role: "user"
  time: { created: number }
  agent?: string
  model?: { providerID: string; modelID: string }
}

export type OpenCodeAssistantMessage = {
  id: string
  sessionID: string
  role: "assistant"
  time: { created: number; completed?: number }
  parentID: string
  modelID: string
  providerID: string
  cost?: number
  tokens?: {
    input: number
    output: number
    reasoning: number
    cache: { read: number; write: number }
  }
  error?: { type: string; message?: string }
}

export type OpenCodeMessage = OpenCodeUserMessage | OpenCodeAssistantMessage

// API response for messages endpoint
export type OpenCodeMessageWithParts = {
  info: OpenCodeMessage
  parts: OpenCodePart[]
}

const trimTrailingSlash = (value: string) => value.replace(/\/$/, "")

const base = (opencodeBaseUrl: string) => {
  if (!opencodeBaseUrl) throw new Error("OpenCode base URL is not configured")
  return trimTrailingSlash(opencodeBaseUrl)
}

async function handleResponse<T>(res: Response): Promise<T> {
  const contentType = res.headers.get("content-type") || ""
  
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `Request failed with ${res.status}`)
  }
  
  // Check if response is JSON
  if (!contentType.includes("application/json")) {
    const text = await res.text()
    throw new Error(`Expected JSON response but got ${contentType}: ${text.slice(0, 100)}...`)
  }
  
  return res.json()
}

export async function fetchSessions(opencodeBaseUrl: string): Promise<OpenCodeSession[]> {
  const res = await fetch(`${base(opencodeBaseUrl)}/session`, { cache: "no-store" })
  return handleResponse<OpenCodeSession[]>(res)
}

export async function fetchMessages(opencodeBaseUrl: string, sessionId: string): Promise<OpenCodeMessageWithParts[]> {
  // Correct endpoint is /message (singular), not /messages
  const res = await fetch(`${base(opencodeBaseUrl)}/session/${sessionId}/message`, { cache: "no-store" })
  return handleResponse<OpenCodeMessageWithParts[]>(res)
}

export async function sendMessage(
  opencodeBaseUrl: string,
  sessionId: string,
  content: string,
  model?: { providerID: string; modelID: string },
) {
  // Correct endpoint is /message with POST, body contains parts array
  const res = await fetch(`${base(opencodeBaseUrl)}/session/${sessionId}/message`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      model,
      parts: [{ type: "text", text: content }],
    }),
  })
  return handleResponse<OpenCodeMessageWithParts>(res)
}

export async function sendMessageAsync(
  opencodeBaseUrl: string,
  sessionId: string,
  content: string,
  model?: { providerID: string; modelID: string },
) {
  // Async version - returns immediately, use SSE for updates
  const res = await fetch(`${base(opencodeBaseUrl)}/session/${sessionId}/prompt_async`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      model,
      parts: [{ type: "text", text: content }],
    }),
  })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `Request failed with ${res.status}`)
  }
  // Returns 204 No Content
  return true
}

export async function abortSession(opencodeBaseUrl: string, sessionId: string): Promise<boolean> {
  const res = await fetch(`${base(opencodeBaseUrl)}/session/${sessionId}/abort`, {
    method: "POST",
  })
  return handleResponse<boolean>(res)
}

export async function createSession(opencodeBaseUrl: string, title?: string, parentID?: string): Promise<OpenCodeSession> {
  const res = await fetch(`${base(opencodeBaseUrl)}/session`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title, parentID }),
  })
  return handleResponse<OpenCodeSession>(res)
}

export type EventCallback = (event: { type: string; properties: unknown }) => void

export function subscribeToEvents(opencodeBaseUrl: string, callback: EventCallback) {
  const source = new EventSource(`${base(opencodeBaseUrl)}/event`)
  
  source.onmessage = (event) => {
    try {
      const parsed = JSON.parse(event.data)
      callback(parsed)
    } catch {
      // SSE can send non-JSON data like connection keep-alives, ignore those
      console.debug("Non-JSON SSE data:", event.data)
    }
  }
  
  source.onerror = (err) => {
    console.error("Opencode SSE error", err)
  }
  
  return () => source.close()
}
