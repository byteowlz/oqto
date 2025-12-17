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

type SessionStatusMap = Record<string, { status: string }>

function tryParseJson(value: string): unknown | null {
  try {
    return JSON.parse(value) as unknown
  } catch {
    return null
  }
}

// Subscribe to events.
// Prefer SSE (/event) and fall back to polling (/session/status) with a conservative interval.
function extractWorkspaceSessionIdFromOpencodeBaseUrl(opencodeBaseUrl: string): string | null {
  const match = opencodeBaseUrl.match(/\/session\/([^/]+)\/code(?:\/)?$/)
  return match ? match[1] : null
}

export function subscribeToEvents(
  opencodeBaseUrl: string,
  callback: EventCallback,
  authToken?: string | null,
  directControlPlaneUrl?: string,
) {
  let active = true
  const statusBySession: Record<string, string> = {}

  let pollTimeout: ReturnType<typeof setTimeout> | null = null
  let pollDelayMs = 2000
  const minPollDelayMs = 2000
  const maxPollDelayMs = 15000

  const stopPolling = () => {
    if (pollTimeout) clearTimeout(pollTimeout)
    pollTimeout = null
  }

  const emitStatusTransitions = (status: SessionStatusMap) => {
    for (const [sessionId, sessionStatus] of Object.entries(status)) {
      const prevStatus = statusBySession[sessionId]
      const currentStatus = sessionStatus.status

      if (prevStatus !== currentStatus) {
        if (currentStatus === "idle") {
          callback({ type: "session.idle", properties: { sessionId } })
        } else if (currentStatus === "busy") {
          callback({ type: "session.busy", properties: { sessionId } })
        }
        callback({ type: "message.updated", properties: { sessionId } })
      }

      statusBySession[sessionId] = currentStatus
    }
  }

  const poll = async () => {
    if (!active) return

    try {
      const statusBase = (() => {
        const direct = trimTrailingSlash(directControlPlaneUrl ?? "")
        const sessionId = extractWorkspaceSessionIdFromOpencodeBaseUrl(opencodeBaseUrl)
        if (direct && sessionId) return `${direct}/session/${sessionId}/code`
        return base(opencodeBaseUrl)
      })()

      const statusUrl = new URL(
        `${statusBase}/session/status`,
        typeof window === "undefined" ? "http://localhost" : window.location.href,
      )
      if (authToken) statusUrl.searchParams.set("token", authToken)
      const res = await fetch(statusUrl.toString(), { cache: "no-store" })
      if (res.ok) {
        const status = (await res.json()) as SessionStatusMap
        emitStatusTransitions(status)
        pollDelayMs = minPollDelayMs
      } else {
        pollDelayMs = Math.min(maxPollDelayMs, Math.round(pollDelayMs * 1.5))
      }
    } catch {
      pollDelayMs = Math.min(maxPollDelayMs, Math.round(pollDelayMs * 1.5))
    }

    if (!active) return
    pollTimeout = setTimeout(poll, pollDelayMs)
  }

  let eventSource: EventSource | null = null
  const sseUrl = (() => {
    const direct = trimTrailingSlash(directControlPlaneUrl ?? "")
    const sessionId = extractWorkspaceSessionIdFromOpencodeBaseUrl(opencodeBaseUrl)
    const sseBase = direct && sessionId ? `${direct}/session/${sessionId}/code` : base(opencodeBaseUrl)
    const url = new URL(
      `${sseBase}/event`,
      typeof window === "undefined" ? "http://localhost" : window.location.href,
    )
    if (authToken) url.searchParams.set("token", authToken)
    return url.toString()
  })()

  const startSse = () => {
    try {
      eventSource = new EventSource(sseUrl, { withCredentials: true })
    } catch {
      eventSource = null
      poll()
      return
    }

    eventSource.onopen = () => {
      pollDelayMs = minPollDelayMs
      stopPolling()
    }

    eventSource.onmessage = (event) => {
      const parsed = tryParseJson(event.data)

      if (parsed && typeof parsed === "object" && parsed !== null && "type" in parsed) {
        const typed = parsed as { type: string; properties?: unknown }
        callback({ type: typed.type, properties: typed.properties ?? typed })
        return
      }

      callback({ type: "message.updated", properties: parsed ?? { raw: event.data } })
    }

    eventSource.onerror = () => {
      // If SSE isn't available (dev proxy/config), fall back to polling.
      if (eventSource) {
        eventSource.close()
        eventSource = null
      }
      if (!pollTimeout) poll()
    }
  }

  startSse()

  return () => {
    active = false
    stopPolling()
    if (eventSource) {
      eventSource.close()
      eventSource = null
    }
  }
}
