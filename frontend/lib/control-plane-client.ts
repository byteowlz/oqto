export type WorkspaceSessionStatus = "pending" | "starting" | "running" | "stopping" | "stopped" | "failed"

export type WorkspaceSession = {
  id: string
  container_id: string | null
  container_name: string
  user_id: string
  workspace_path: string
  image: string
  opencode_port: number
  fileserver_port: number
  ttyd_port: number
  status: WorkspaceSessionStatus
  created_at: string
  started_at: string | null
  stopped_at: string | null
  error_message: string | null
}

export type CreateWorkspaceSessionRequest = {
  workspace_path?: string
  image?: string
  env?: Record<string, string>
}

type ApiErrorResponse = {
  error?: string
}

const trimTrailingSlash = (value: string) => value.replace(/\/$/, "")

export function controlPlaneDirectBaseUrl(): string {
  return trimTrailingSlash(process.env.NEXT_PUBLIC_CONTROL_PLANE_URL ?? "")
}

async function readApiError(res: Response): Promise<string> {
  const contentType = res.headers.get("content-type") ?? ""
  if (contentType.includes("application/json")) {
    const parsed = (await res.json().catch(() => null)) as ApiErrorResponse | null
    if (parsed?.error) return parsed.error
  }
  return (await res.text().catch(() => res.statusText)) || res.statusText
}

export async function devLogin(): Promise<boolean> {
  const res = await fetch(`/api/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username: "dev", password: "dev" }),
  })
  return res.ok
}

export async function listWorkspaceSessions(): Promise<WorkspaceSession[]> {
  const res = await fetch(`/api/sessions`, { cache: "no-store" })
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

export async function createWorkspaceSession(request: CreateWorkspaceSessionRequest = {}): Promise<WorkspaceSession> {
  const res = await fetch(`/api/sessions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
  })
  if (!res.ok) throw new Error(await readApiError(res))
  const data = (await res.json()) as { session?: WorkspaceSession } | WorkspaceSession
  if ("id" in data) return data
  if (data.session && "id" in data.session) return data.session
  throw new Error("Unexpected create session response")
}

export async function stopWorkspaceSession(sessionId: string): Promise<void> {
  const res = await fetch(`/api/sessions/${sessionId}/stop`, { method: "POST" })
  if (!res.ok) throw new Error(await readApiError(res))
}

export function opencodeProxyBaseUrl(sessionId: string) {
  return `/api/session/${sessionId}/code`
}

export function terminalProxyPath(sessionId: string) {
  return `/session/${sessionId}/term`
}
