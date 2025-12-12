"use client"

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react"
import { appRegistry, type AppDefinition, type Locale, type LocalizedText } from "@/lib/app-registry"
import { fetchSessions, subscribeToEvents, type OpenCodeSession } from "@/lib/opencode-client"
import { hasOpencode } from "@/lib/config"

interface AppContextValue {
  apps: AppDefinition[]
  activeAppId: string
  setActiveAppId: (id: string) => void
  activeApp?: AppDefinition
  locale: Locale
  setLocale: (locale: Locale) => void
  resolveText: (value: LocalizedText) => string
  // Session state
  sessions: OpenCodeSession[]
  selectedSessionId: string
  setSelectedSessionId: (id: string) => void
  refreshSessions: () => Promise<void>
}

const AppContext = createContext<AppContextValue | null>(null)

export function AppProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>("de")
  const apps = useMemo(() => appRegistry.getAllApps(), [])
  const [activeAppId, setActiveAppId] = useState(() => apps[0]?.id ?? "")
  const activeApp = apps.find((app) => app.id === activeAppId) ?? apps[0]
  
  // Session state
  const [sessions, setSessions] = useState<OpenCodeSession[]>([])
  const [selectedSessionId, setSelectedSessionId] = useState<string>("")

  useEffect(() => {
    const storedLocale = window.localStorage.getItem("locale")
    const initialLocale: Locale = storedLocale === "en" ? "en" : "de"
    setLocaleState(initialLocale)
    document.documentElement.lang = initialLocale
  }, [])

  // Load sessions on mount
  const refreshSessions = useCallback(async () => {
    if (!hasOpencode) return
    try {
      const data = await fetchSessions()
      setSessions(data)
      // Auto-select first session if none selected
      if (!selectedSessionId && data.length > 0) {
        setSelectedSessionId(data[0].id)
      }
    } catch (err) {
      console.error("Failed to load sessions:", err)
    }
  }, [selectedSessionId])

  useEffect(() => {
    refreshSessions()
  }, [refreshSessions])

  // Subscribe to session events
  useEffect(() => {
    if (!hasOpencode) return
    const unsubscribe = subscribeToEvents((event) => {
      const eventType = event.type as string
      if (eventType?.startsWith("session")) {
        refreshSessions()
      }
    })
    return unsubscribe
  }, [refreshSessions])

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
      sessions,
      selectedSessionId,
      setSelectedSessionId,
      refreshSessions,
    }),
    [apps, activeAppId, activeApp, locale, setLocale, resolveText, sessions, selectedSessionId, refreshSessions],
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
