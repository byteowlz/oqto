import { appConfig } from "@/lib/config"

export type OpenCodeSession = {
  id: string
  title: string
  created: string
  updated: string
  parentID?: string | null
}

export type OpenCodeMessagePart = {
  id: string
  type: string
  text?: string
}

export type OpenCodeMessage = {
  id: string
  role: "user" | "assistant"
  content?: string
  parts?: OpenCodeMessagePart[]
  createdAt?: string
}

const base = () => {
  if (!appConfig.opencodeBaseUrl) throw new Error("NEXT_PUBLIC_OPENCODE_BASE_URL is not configured")
  return appConfig.opencodeBaseUrl
}

async function handleResponse(res: Response) {
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `Request failed with ${res.status}`)
  }
  return res.json()
}

export async function fetchSessions(): Promise<OpenCodeSession[]> {
  const res = await fetch(`${base()}/session`, { cache: "no-store" })
  return handleResponse(res)
}

export async function fetchMessages(sessionId: string): Promise<OpenCodeMessage[]> {
  const res = await fetch(`${base()}/session/${sessionId}/messages`, { cache: "no-store" })
  return handleResponse(res)
}

export async function sendMessage(sessionId: string, content: string) {
  const res = await fetch(`${base()}/session/${sessionId}/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message: content }),
  })
  return handleResponse(res)
}

export type EventCallback = (event: { type: string; data: unknown }) => void

export function subscribeToEvents(callback: EventCallback) {
  if (!appConfig.opencodeBaseUrl) return () => {}
  const source = new EventSource(`${appConfig.opencodeBaseUrl}/event`)
  source.onmessage = (event) => {
    try {
      const parsed = JSON.parse(event.data)
      callback(parsed)
    } catch (err) {
      console.error("Failed to parse opencode event", err)
    }
  }
  source.onerror = (err) => {
    console.error("Opencode SSE error", err)
  }
  return () => source.close()
}
