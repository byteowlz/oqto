"use client"

import { useEffect, useState } from "react"
import {
  SunMedium,
  MoonStar,
  Globe2,
  PanelLeftClose,
  PanelRightClose,
  FolderKanban,
  MessageSquare,
  Bot,
  Shield,
  Menu,
  X,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { ClientOnly } from "@/components/client-only"
import { AppProvider, useApp } from "@/components/app-context"
import "@/apps"

function AppShell() {
  const { apps, activeAppId, setActiveAppId, activeApp, locale, setLocale, resolveText } = useApp()
  const [theme, setTheme] = useState<"light" | "dark">("dark")
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)
  const ActiveComponent = activeApp?.component ?? null

  useEffect(() => {
    const stored = window.localStorage.getItem("theme")
    const prefersDark = window.matchMedia?.("(prefers-color-scheme: dark)").matches
    const initial = stored === "light" || stored === "dark" ? stored : prefersDark ? "dark" : "light"
    document.documentElement.classList.toggle("dark", initial === "dark")
    setTheme(initial)
  }, [])

  const toggleTheme = () => {
    setTheme((prev) => {
      const next = prev === "dark" ? "light" : "dark"
      document.documentElement.classList.toggle("dark", next === "dark")
      window.localStorage.setItem("theme", next)
      return next
    })
  }

  const toggleLocale = () => {
    const next = locale === "de" ? "en" : "de"
    setLocale(next)
  }

  const sidebarWidth = sidebarCollapsed ? "4.5rem" : "16.25rem"
  const shellBg = "var(--background)"
  const sidebarBg = "var(--sidebar, #181b1a)"
  const navIdle = "var(--sidebar, #181b1a)"
  const navIconFor = (id: string) => {
    switch (id) {
      case "projects":
        return FolderKanban
      case "sessions":
        return MessageSquare
      case "workspaces":
        return Bot
      case "admin":
        return Shield
      default:
        return FolderKanban
    }
  }

  const handleMobileNavClick = (appId: string) => {
    setActiveAppId(appId)
    setMobileMenuOpen(false)
  }

  return (
    <div className="flex min-h-screen bg-[#222624] text-foreground">
      {/* Mobile header */}
      <header className="fixed top-0 left-0 right-0 h-14 flex items-center justify-between px-4 z-50 md:hidden" style={{ backgroundColor: sidebarBg }}>
        <Button
          variant="ghost"
          size="icon"
          aria-label="Menu"
          onClick={() => setMobileMenuOpen(true)}
          className="text-muted-foreground hover:text-primary"
        >
          <Menu className="w-5 h-5" />
        </Button>
        <img src="/Logo_Green-02.png" alt="Fraunhofer IEM" className="h-8 w-auto" />
      </header>

      {/* Mobile fullscreen menu */}
      {mobileMenuOpen && (
      <div
        className="fixed inset-0 z-50 flex flex-col md:hidden"
        style={{ backgroundColor: sidebarBg }}
      >
        <div className="h-14 flex items-center justify-between px-4">
          <img src="/Logo_Green-02.png" alt="Fraunhofer IEM" className="h-8 w-auto" />
          <Button
            variant="ghost"
            size="icon"
            aria-label="Close menu"
            onClick={() => setMobileMenuOpen(false)}
            className="text-muted-foreground hover:text-primary"
          >
            <X className="w-5 h-5" />
          </Button>
        </div>
        
        <nav className="flex-1 w-full space-y-3 px-4 pt-6">
          {apps.map((app) => {
            const isActive = activeAppId === app.id
            const Icon = navIconFor(app.id)
            return (
              <button
                key={app.id}
                onClick={() => handleMobileNavClick(app.id)}
                className={`w-full px-4 py-4 text-base font-semibold tracking-wide transition flex items-center gap-3 ${
                  isActive ? "text-[#f2f5f3] ring-2 ring-[#3ba77c]" : "text-[#dfe5e1] hover:bg-[#222624]"
                }`}
                style={{
                  backgroundColor: isActive ? "#222624" : navIdle,
                  border: isActive ? "1px solid #3ba77c" : "1px solid transparent",
                }}
              >
                <Icon className="w-5 h-5 shrink-0" />
                <span>{resolveText(app.label)}</span>
              </button>
            )
          })}
        </nav>

        <div className="w-full px-4 pb-8 space-y-3">
          <Button
            variant="ghost"
            size="lg"
            onClick={() => { toggleLocale(); setMobileMenuOpen(false); }}
            aria-label="Sprache wechseln"
            className="w-full justify-start text-muted-foreground hover:text-primary py-4"
          >
            <Globe2 className="w-5 h-5" />
            <span className="text-base font-semibold">{locale === "de" ? "ENG" : "DE"}</span>
          </Button>
          <Button
            variant="ghost"
            size="lg"
            onClick={() => { toggleTheme(); setMobileMenuOpen(false); }}
            aria-pressed={theme === "dark"}
            className="w-full justify-start text-muted-foreground hover:text-primary py-4"
          >
            {theme === "dark" ? <SunMedium className="w-5 h-5" /> : <MoonStar className="w-5 h-5" />}
            <span className="text-base font-semibold">Theme</span>
          </Button>
        </div>
      </div>
      )}

      {/* Desktop sidebar */}
      <aside
        className={`fixed inset-y-0 left-0 flex-col transition-all duration-200 z-40 hidden md:flex ${
          sidebarCollapsed ? "w-[4.5rem] items-center" : "w-[16.25rem] items-center"
        }`}
        style={{ backgroundColor: sidebarBg }}
      >
        <div className="h-16 w-full flex items-center justify-between px-4">
          {!sidebarCollapsed && <img src="/Logo_Green-02.png" alt="Fraunhofer IEM" className="h-10 w-auto" />}
          <Button
            variant="ghost"
            size="icon"
            aria-label="Sidebar umschalten"
            onClick={() => setSidebarCollapsed((prev) => !prev)}
            className="text-muted-foreground hover:text-primary"
          >
            {sidebarCollapsed ? <PanelRightClose className="w-4 h-4" /> : <PanelLeftClose className="w-4 h-4" />}
          </Button>
        </div>
        <nav className={`w-full space-y-3 ${sidebarCollapsed ? "px-2" : "px-4"} pt-6`}>
          {apps.map((app) => {
            const isActive = activeAppId === app.id
            const Icon = navIconFor(app.id)
            return (
              <button
                key={app.id}
                onClick={() => setActiveAppId(app.id)}
                className={`w-full px-4 py-3 text-sm font-semibold tracking-wide transition flex items-center gap-2 ${
                  isActive ? "text-[#f2f5f3] ring-2 ring-[#3ba77c]" : "text-[#dfe5e1] hover:bg-[#222624]"
                } ${sidebarCollapsed ? "justify-center" : ""}`}
                style={{
                  backgroundColor: isActive ? "#222624" : navIdle,
                  border: isActive ? "1px solid #3ba77c" : "1px solid transparent",
                }}
              >
                <Icon className="w-4 h-4 shrink-0" />
                {!sidebarCollapsed && <span className="truncate">{resolveText(app.label)}</span>}
              </button>
            )
          })}
        </nav>

        <div className={`w-full ${sidebarCollapsed ? "px-2 pb-4" : "px-4 pb-6"} space-y-3 mt-6`}>
          <Button
            variant="ghost"
            size="sm"
            onClick={toggleLocale}
            aria-label="Sprache wechseln"
            className="w-full justify-start text-muted-foreground hover:text-primary"
            style={{ justifyContent: sidebarCollapsed ? "center" : "flex-start" }}
          >
            <Globe2 className="w-4 h-4" />
            {!sidebarCollapsed && <span className="text-sm font-semibold">{locale === "de" ? "ENG" : "DE"}</span>}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={toggleTheme}
            aria-pressed={theme === "dark"}
            className="w-full justify-start text-muted-foreground hover:text-primary"
            style={{ justifyContent: sidebarCollapsed ? "center" : "flex-start" }}
          >
            {theme === "dark" ? <SunMedium className="w-4 h-4" /> : <MoonStar className="w-4 h-4" />}
            {!sidebarCollapsed && <span className="text-sm font-semibold">Theme</span>}
          </Button>
        </div>
      </aside>

      {/* Main content */}
      <div className="flex-1 flex flex-col min-h-0 overflow-hidden" style={{ backgroundColor: shellBg }}>
        <div 
          className={`flex-1 min-h-0 overflow-auto pt-14 md:pt-0 transition-all duration-200 ${
            sidebarCollapsed ? "md:pl-[4.5rem]" : "md:pl-[16.25rem]"
          }`}
        >
          <div className="min-h-full">
            {ActiveComponent ? <ActiveComponent /> : <EmptyState />}
          </div>
        </div>
      </div>
    </div>
  )
}

function EmptyState() {
  return (
    <div className="flex items-center justify-center h-full">
      <div className="text-center space-y-2">
        <p className="text-sm text-muted-foreground">No apps registered</p>
        <p className="text-xs text-muted-foreground">Register an app in apps/index.ts to get started.</p>
      </div>
    </div>
  )
}

export default function AgentWorkspacePlatform() {
  return (
    <ClientOnly>
      <AppProvider>
        <AppShell />
      </AppProvider>
    </ClientOnly>
  )
}
