"use client"

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react"
import { appRegistry, type AppDefinition, type Locale, type LocalizedText } from "@/lib/app-registry"
import { createSession, fetchSessions, subscribeToEvents, type OpenCodeSession } from "@/lib/opencode-client"
import {
  createWorkspaceSession,
  controlPlaneDirectBaseUrl,
  login,
  listWorkspaceSessions,
  opencodeProxyBaseUrl,
  type WorkspaceSession,
} from "@/lib/control-plane-client"

interface AppContextValue {
  apps: AppDefinition[]
  activeAppId: string
  setActiveAppId: (id: string) => void
  activeApp?: AppDefinition
  locale: Locale
  setLocale: (locale: Locale) => void
  resolveText: (value: LocalizedText) => string
  workspaceSessions: WorkspaceSession[]
  selectedWorkspaceSessionId: string
  setSelectedWorkspaceSessionId: (id: string) => void
  opencodeBaseUrl: string
  opencodeSessions: OpenCodeSession[]
  selectedChatSessionId: string
  setSelectedChatSessionId: (id: string) => void
  selectedChatSession: OpenCodeSession | undefined
  refreshWorkspaceSessions: () => Promise<void>
  refreshOpencodeSessions: () => Promise<void>
  authToken: string | null
}

const AppContext = createContext<AppContextValue | null>(null)

export function AppProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>("de")
  const apps = useMemo(() => appRegistry.getAllApps(), [])
  const [activeAppId, setActiveAppId] = useState(() => apps[0]?.id ?? "")
  const activeApp = apps.find((app) => app.id === activeAppId) ?? apps[0]
  
  const [workspaceSessions, setWorkspaceSessions] = useState<WorkspaceSession[]>([])
  const [selectedWorkspaceSessionId, setSelectedWorkspaceSessionId] = useState<string>("")
  const [opencodeSessions, setOpencodeSessions] = useState<OpenCodeSession[]>([])
  const [selectedChatSessionId, setSelectedChatSessionId] = useState<string>("")
  const [authToken, setAuthToken] = useState<string | null>(null)

  const selectedChatSession = useMemo(() => {
    return opencodeSessions.find((s) => s.id === selectedChatSessionId)
  }, [opencodeSessions, selectedChatSessionId])

  const selectedWorkspaceSession = useMemo(() => {
    if (!selectedWorkspaceSessionId) return undefined
    return workspaceSessions.find((session) => session.id === selectedWorkspaceSessionId)
  }, [selectedWorkspaceSessionId, workspaceSessions])

  const opencodeBaseUrl = useMemo(() => {
    if (!selectedWorkspaceSession) return ""
    if (selectedWorkspaceSession.status !== "running") return ""
    return opencodeProxyBaseUrl(selectedWorkspaceSession.id)
  }, [selectedWorkspaceSession])

  useEffect(() => {
    const storedLocale = window.localStorage.getItem("locale")
    const initialLocale: Locale = storedLocale === "en" ? "en" : "de"
    setLocaleState(initialLocale)
    document.documentElement.lang = initialLocale

    const storedWorkspaceSessionId = window.localStorage.getItem("workspaceSessionId") ?? ""
    if (storedWorkspaceSessionId) {
      setSelectedWorkspaceSessionId(storedWorkspaceSessionId)
    }

    const storedAuthToken = window.localStorage.getItem("authToken")
    if (storedAuthToken) {
      setAuthToken(storedAuthToken)
    }
  }, [])

  const refreshWorkspaceSessions = useCallback(async () => {
    try {
      // Dev login - store token for WebSocket auth
      if (!authToken) {
        try {
          const loginResponse = await login({ username: "dev", password: "devpassword123" })
          setAuthToken(loginResponse.token)
          try {
            window.localStorage.setItem("authToken", loginResponse.token)
          } catch {
            // ignore storage failures
          }
        } catch {
          // Login might fail if already logged in via cookie
        }
      }
      let data = await listWorkspaceSessions()

      if (data.length === 0) {
        await createWorkspaceSession().catch(() => undefined)
        data = await listWorkspaceSessions()
      }
      setWorkspaceSessions(data)

      if (data.length > 0) {
        setSelectedWorkspaceSessionId((current) => {
          // If no current selection, pick the first running session or first session
          if (!current) {
            const running = data.find((s) => s.status === "running")
            return running?.id || data[0].id
          }
          // If current session is failed/stopped, switch to a running one if available
          const currentSession = data.find((s) => s.id === current)
          if (currentSession?.status === "failed" || currentSession?.status === "stopped") {
            const running = data.find((s) => s.status === "running")
            if (running) return running.id
          }
          return current
        })
      }
    } catch (err) {
      console.error("Failed to load sessions:", err)
    }
  }, [authToken])

  useEffect(() => {
    refreshWorkspaceSessions()
  }, [refreshWorkspaceSessions])

  useEffect(() => {
    if (!opencodeBaseUrl) return
    const unsubscribe = subscribeToEvents(
      opencodeBaseUrl,
      (event) => {
        const eventType = event.type as string
        if (eventType?.startsWith("session")) {
          refreshWorkspaceSessions()
        }
      },
      authToken,
      controlPlaneDirectBaseUrl(),
    )
    return unsubscribe
  }, [authToken, opencodeBaseUrl, refreshWorkspaceSessions])

  useEffect(() => {
    if (!selectedWorkspaceSession) return
    if (selectedWorkspaceSession.status === "starting" || selectedWorkspaceSession.status === "pending") {
      const timeout = setTimeout(() => {
        void refreshWorkspaceSessions()
      }, 1000)
      return () => clearTimeout(timeout)
    }
  }, [selectedWorkspaceSession, refreshWorkspaceSessions])

  useEffect(() => {
    if (!selectedWorkspaceSessionId) return
    window.localStorage.setItem("workspaceSessionId", selectedWorkspaceSessionId)
  }, [selectedWorkspaceSessionId])

  const refreshOpencodeSessions = useCallback(async () => {
    if (!opencodeBaseUrl) return
    try {
      const sessions = await fetchSessions(opencodeBaseUrl)
      setOpencodeSessions(sessions)
      // Select most recently updated session, or create one if none exist
      if (sessions.length > 0) {
        const sorted = [...sessions].sort((a, b) => b.time.updated - a.time.updated)
        setSelectedChatSessionId((current) => {
          // Keep current if it exists in the list
          if (current && sessions.some((s) => s.id === current)) return current
          return sorted[0].id
        })
      } else {
        const created = await createSession(opencodeBaseUrl)
        setOpencodeSessions([created])
        setSelectedChatSessionId(created.id)
      }
    } catch (err) {
      console.error("Failed to load opencode sessions:", err)
    }
  }, [opencodeBaseUrl])

  useEffect(() => {
    refreshOpencodeSessions()
  }, [refreshOpencodeSessions])

  const setLocale = useCallback((next: Locale) => {
    setLocaleState(next)
    document.documentElement.lang = next
    try {
      window.localStorage.setItem("locale", next)
    } catch {
      // ignore storage failures
    }
  }, [])

  const resolveText = useCallback(
    (value: LocalizedText) => {
      if (typeof value === "string") return value
      return locale === "en" ? value.en : value.de
    },
    [locale],
  )

  const value = useMemo(
    () => ({
      apps,
      activeAppId,
      setActiveAppId,
      activeApp,
      locale,
      setLocale,
      resolveText,
      workspaceSessions,
      selectedWorkspaceSessionId,
      setSelectedWorkspaceSessionId,
      opencodeBaseUrl,
      opencodeSessions,
      selectedChatSessionId,
      setSelectedChatSessionId,
      selectedChatSession,
      refreshWorkspaceSessions,
      refreshOpencodeSessions,
      authToken,
    }),
    [
      apps,
      activeAppId,
      activeApp,
      locale,
      setLocale,
      resolveText,
      workspaceSessions,
      selectedWorkspaceSessionId,
      opencodeBaseUrl,
      opencodeSessions,
      selectedChatSessionId,
      selectedChatSession,
      refreshWorkspaceSessions,
      refreshOpencodeSessions,
      authToken,
    ],
  )

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>
}

export function useApp() {
  const ctx = useContext(AppContext)
  if (!ctx) {
    throw new Error("useApp must be used within an AppProvider")
  }
  return ctx
}
