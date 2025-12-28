// ============================================================================
// Auth Types
// ============================================================================

export type UserInfo = {
  id: string
  name: string
  email: string
  role: string
}

export type LoginRequest = {
  username: string
  password: string
}

export type LoginResponse = {
  token: string
  user: UserInfo
}

export type RegisterRequest = {
  username: string
  email: string
  password: string
  invite_code: string
  display_name?: string
}

export type RegisterResponse = {
  token: string
  user: UserInfo
}

// ============================================================================
// Session Types
// ============================================================================

export type WorkspaceSessionStatus = "pending" | "starting" | "running" | "stopping" | "stopped" | "failed"

/** Workspace preference for a persona */
export type WorkspacePreference = "general" | "project" | "ask"

/** Persona metadata from persona.toml */
export type Persona = {
  /** Unique identifier (directory name) */
  id: string
  /** Display name of the persona */
  name: string
  /** Short description of what this persona does */
  description: string
  /** Accent color for UI (hex color, e.g., "#6366f1") */
  color?: string | null
  /** Path to avatar image (relative to persona directory) */
  avatar?: string | null
  /** Whether this is the default persona */
  is_default: boolean
  /** opencode agent ID to use */
  agent_id: string
  /** Workspace preference (general, project, or ask) */
  workspace: WorkspacePreference
}

export type WorkspaceSession = {
  id: string
  readable_id: string | null
  container_id: string | null
  container_name: string
  user_id: string
  workspace_path: string
  persona_path: string | null
  image: string
  opencode_port: number
  fileserver_port: number
  ttyd_port: number
  status: WorkspaceSessionStatus
  created_at: string
  started_at: string | null
  stopped_at: string | null
  error_message: string | null
  /** Persona metadata (if session has a persona_path with persona.toml) */
  persona?: Persona | null
}

export type CreateWorkspaceSessionRequest = {
  workspace_path?: string
  image?: string
  /** Persona ID to use for this session */
  persona_id?: string
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

// ============================================================================
// Auth API
// ============================================================================

export async function login(request: LoginRequest): Promise<LoginResponse> {
  const res = await fetch(`/api/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
    credentials: "include",
  })
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

export async function register(request: RegisterRequest): Promise<RegisterResponse> {
  const res = await fetch(`/api/auth/register`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
    credentials: "include",
  })
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

export async function logout(): Promise<void> {
  const res = await fetch(`/api/auth/logout`, {
    method: "POST",
    credentials: "include",
  })
  if (!res.ok) throw new Error(await readApiError(res))
}

export async function getCurrentUser(): Promise<UserInfo | null> {
  const res = await fetch(`/api/me`, {
    credentials: "include",
  })
  if (res.status === 401) return null
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

/** @deprecated Use login() instead */
export async function devLogin(): Promise<boolean> {
  try {
    await login({ username: "dev", password: "devpassword123" })
    return true
  } catch {
    return false
  }
}

export async function listWorkspaceSessions(): Promise<WorkspaceSession[]> {
  const res = await fetch(`/api/sessions`, { cache: "no-store", credentials: "include" })
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

export async function createWorkspaceSession(request: CreateWorkspaceSessionRequest = {}): Promise<WorkspaceSession> {
  const res = await fetch(`/api/sessions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
    credentials: "include",
  })
  if (!res.ok) throw new Error(await readApiError(res))
  const data = (await res.json()) as { session?: WorkspaceSession } | WorkspaceSession
  if ("id" in data) return data
  if (data.session && "id" in data.session) return data.session
  throw new Error("Unexpected create session response")
}

/** Get or create a session - handles auto-resume and auto-upgrade */
export async function getOrCreateWorkspaceSession(request: CreateWorkspaceSessionRequest = {}): Promise<WorkspaceSession> {
  const res = await fetch(`/api/sessions/get-or-create`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
    credentials: "include",
  })
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

export async function stopWorkspaceSession(sessionId: string): Promise<void> {
  const res = await fetch(`/api/sessions/${sessionId}/stop`, { method: "POST", credentials: "include" })
  if (!res.ok) throw new Error(await readApiError(res))
}

export async function deleteWorkspaceSession(sessionId: string): Promise<void> {
  const res = await fetch(`/api/sessions/${sessionId}`, { method: "DELETE", credentials: "include" })
  if (!res.ok) throw new Error(await readApiError(res))
}

export type SessionUpdateInfo = {
  update_available: boolean
  current_digest: string | null
  latest_digest: string | null
}

export async function checkSessionUpdate(sessionId: string): Promise<SessionUpdateInfo> {
  const res = await fetch(`/api/sessions/${sessionId}/update`, { credentials: "include" })
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

export async function upgradeWorkspaceSession(sessionId: string): Promise<WorkspaceSession> {
  const res = await fetch(`/api/sessions/${sessionId}/upgrade`, { method: "POST", credentials: "include" })
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

// ============================================================================
// Persona API
// ============================================================================

/** List all available personas */
export async function listPersonas(): Promise<Persona[]> {
  const res = await fetch(`/api/personas`, { cache: "no-store", credentials: "include" })
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

/** Get a specific persona by ID */
export async function getPersona(personaId: string): Promise<Persona> {
  const res = await fetch(`/api/personas/${personaId}`, { credentials: "include" })
  if (!res.ok) throw new Error(await readApiError(res))
  return res.json()
}

// ============================================================================
// Proxy URLs
// ============================================================================

export function opencodeProxyBaseUrl(sessionId: string) {
  return `/api/session/${sessionId}/code`
}

export function terminalProxyPath(sessionId: string) {
  return `/session/${sessionId}/term`
}

export function fileserverProxyBaseUrl(sessionId: string) {
  return `/api/session/${sessionId}/files`
}
