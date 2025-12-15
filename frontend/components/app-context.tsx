"use client"

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react"
import { appRegistry, type AppDefinition, type Locale, type LocalizedText } from "@/lib/app-registry"
import { createSession, fetchSessions, subscribeToEvents } from "@/lib/opencode-client"
import {
  createWorkspaceSession,
  devLogin,
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
  selectedChatSessionId: string
  refreshWorkspaceSessions: () => Promise<void>
}

const AppContext = createContext<AppContextValue | null>(null)

export function AppProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>("de")
  const apps = useMemo(() => appRegistry.getAllApps(), [])
  const [activeAppId, setActiveAppId] = useState(() => apps[0]?.id ?? "")
  const activeApp = apps.find((app) => app.id === activeAppId) ?? apps[0]
  
  const [workspaceSessions, setWorkspaceSessions] = useState<WorkspaceSession[]>([])
  const [selectedWorkspaceSessionId, setSelectedWorkspaceSessionId] = useState<string>("")
  const [selectedChatSessionId, setSelectedChatSessionId] = useState<string>("")

  const opencodeBaseUrl = useMemo(() => {
    if (!selectedWorkspaceSessionId) return ""
    return opencodeProxyBaseUrl(selectedWorkspaceSessionId)
  }, [selectedWorkspaceSessionId])

  useEffect(() => {
    const storedLocale = window.localStorage.getItem("locale")
    const initialLocale: Locale = storedLocale === "en" ? "en" : "de"
    setLocaleState(initialLocale)
    document.documentElement.lang = initialLocale

    const storedWorkspaceSessionId = window.localStorage.getItem("workspaceSessionId") ?? ""
    if (storedWorkspaceSessionId) {
      setSelectedWorkspaceSessionId(storedWorkspaceSessionId)
    }
  }, [])

  const refreshWorkspaceSessions = useCallback(async () => {
    try {
      await devLogin().catch(() => false)
      let data = await listWorkspaceSessions()

      if (data.length === 0) {
        await createWorkspaceSession().catch(() => undefined)
        data = await listWorkspaceSessions()
      }
      setWorkspaceSessions(data)

      if (data.length > 0) {
        setSelectedWorkspaceSessionId((current) => current || data[0].id)
      }
    } catch (err) {
      console.error("Failed to load sessions:", err)
    }
  }, [])

  useEffect(() => {
    refreshWorkspaceSessions()
  }, [refreshWorkspaceSessions])

  useEffect(() => {
    if (!opencodeBaseUrl) return
    const unsubscribe = subscribeToEvents(opencodeBaseUrl, (event) => {
      const eventType = event.type as string
      if (eventType?.startsWith("session")) {
        refreshWorkspaceSessions()
      }
    })
    return unsubscribe
  }, [opencodeBaseUrl, refreshWorkspaceSessions])

  useEffect(() => {
    if (!selectedWorkspaceSessionId) return
    window.localStorage.setItem("workspaceSessionId", selectedWorkspaceSessionId)
  }, [selectedWorkspaceSessionId])

  useEffect(() => {
    if (!opencodeBaseUrl) return

    let cancelled = false
    async function ensureChatSession() {
      try {
        const sessions = await fetchSessions(opencodeBaseUrl)
        if (cancelled) return
        if (sessions.length > 0) {
          setSelectedChatSessionId(sessions[0].id)
          return
        }

        const created = await createSession(opencodeBaseUrl, "Workspace Session")
        if (cancelled) return
        setSelectedChatSessionId(created.id)
      } catch (err) {
        if (!cancelled) console.error("Failed to load chat session:", err)
      }
    }

    ensureChatSession()
    return () => {
      cancelled = true
    }
  }, [opencodeBaseUrl])

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
      selectedChatSessionId,
      refreshWorkspaceSessions,
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
      selectedChatSessionId,
      refreshWorkspaceSessions,
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
