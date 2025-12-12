"use client"

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react"
import { appRegistry, type AppDefinition, type Locale, type LocalizedText } from "@/lib/app-registry"

interface AppContextValue {
  apps: AppDefinition[]
  activeAppId: string
  setActiveAppId: (id: string) => void
  activeApp?: AppDefinition
  locale: Locale
  setLocale: (locale: Locale) => void
  resolveText: (value: LocalizedText) => string
}

const AppContext = createContext<AppContextValue | null>(null)

export function AppProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>("de")
  const apps = useMemo(() => appRegistry.getAllApps(), [])
  const [activeAppId, setActiveAppId] = useState(() => apps[0]?.id ?? "")
  const activeApp = apps.find((app) => app.id === activeAppId) ?? apps[0]

  useEffect(() => {
    const storedLocale = window.localStorage.getItem("locale")
    const initialLocale: Locale = storedLocale === "en" ? "en" : "de"
    setLocaleState(initialLocale)
    document.documentElement.lang = initialLocale
  }, [])

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
    }),
    [apps, activeAppId, activeApp, locale, setLocale, resolveText],
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
