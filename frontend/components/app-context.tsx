"use client"

import { createContext, useContext, useMemo, useState, type ReactNode } from "react"
import { appRegistry, type AppDefinition } from "@/lib/app-registry"

interface AppContextValue {
  apps: AppDefinition[]
  activeAppId: string
  setActiveAppId: (id: string) => void
  activeApp?: AppDefinition
}

const AppContext = createContext<AppContextValue | null>(null)

export function AppProvider({ children }: { children: ReactNode }) {
  const apps = useMemo(() => appRegistry.getAllApps(), [])
  const [activeAppId, setActiveAppId] = useState(() => apps[0]?.id ?? "")
  const activeApp = apps.find((app) => app.id === activeAppId) ?? apps[0]

  const value = useMemo(
    () => ({
      apps,
      activeAppId,
      setActiveAppId,
      activeApp,
    }),
    [apps, activeAppId, activeApp],
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
