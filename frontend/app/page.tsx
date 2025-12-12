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
  Clock,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { ClientOnly } from "@/components/client-only"
import { AppProvider, useApp } from "@/components/app-context"
import { cn } from "@/lib/utils"
import "@/apps"

function AppShell() {
  const { apps, activeAppId, setActiveAppId, activeApp, locale, setLocale, resolveText, sessions, selectedSessionId, setSelectedSessionId } = useApp()
  const [theme, setTheme] = useState<"light" | "dark">("dark")
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)
  const ActiveComponent = activeApp?.component ?? null

  // Handle session click - select session and switch to chats view
  const handleSessionClick = (sessionId: string) => {
    setSelectedSessionId(sessionId)
    setActiveAppId("sessions")
    setMobileMenuOpen(false)
  }

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
        
        <nav className="flex-1 w-full space-y-3 px-4 pt-6 overflow-y-auto">
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

          {/* Session history in mobile menu */}
          {sessions.length > 0 && (
            <div className="pt-4 border-t border-[#2a3632]">
              <div className="flex items-center gap-2 px-4 py-2">
                <span className="text-xs uppercase tracking-wide text-[#6b7974]">
                  {locale === "de" ? "Verlauf" : "History"}
                </span>
                <span className="text-xs text-[#4a5550]">({sessions.length})</span>
              </div>
              <div className="space-y-1">
                {sessions.slice(0, 10).map((session) => {
                  const isSelected = selectedSessionId === session.id
                  return (
                    <button
                      key={session.id}
                      onClick={() => handleSessionClick(session.id)}
                      className={cn(
                        "w-full px-4 py-3 text-left transition-colors",
                        isSelected 
                          ? "bg-[#1b2d26] text-[#d5f0e4]" 
                          : "text-[#9aa8a3] hover:bg-[#222624]"
                      )}
                    >
                      <div className="text-sm truncate">
                        {session.title || `Session ${session.id.slice(0, 8)}`}
                      </div>
                    </button>
                  )
                })}
              </div>
            </div>
          )}
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
        <nav className={`w-full space-y-2 ${sidebarCollapsed ? "px-2" : "px-4"} pt-6`}>
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

        {/* Session history list */}
        {!sidebarCollapsed && sessions.length > 0 && (
          <div className="w-full px-4 mt-4 flex-1 min-h-0 flex flex-col">
            <div className="flex items-center gap-2 py-2 border-t border-[#2a3632]">
              <span className="text-xs uppercase tracking-wide text-[#6b7974]">
                {locale === "de" ? "Verlauf" : "History"}
              </span>
              <span className="text-xs text-[#4a5550]">({sessions.length})</span>
            </div>
            <div className="flex-1 overflow-y-auto space-y-1 pr-1 -mr-1">
              {sessions.slice(0, 20).map((session) => {
                const isSelected = selectedSessionId === session.id
                const updatedAt = session.time?.updated ? new Date(session.time.updated) : null
                return (
                  <button
                    key={session.id}
                    onClick={() => handleSessionClick(session.id)}
                    className={cn(
                      "w-full px-3 py-2 text-left rounded-md transition-colors",
                      isSelected 
                        ? "bg-[#1b2d26] border border-[#3ba77c] text-[#d5f0e4]" 
                        : "text-[#9aa8a3] hover:bg-[#222624] border border-transparent"
                    )}
                  >
                    <div className="text-sm truncate font-medium">
                      {session.title || `Session ${session.id.slice(0, 8)}`}
                    </div>
                    {updatedAt && (
                      <div className="flex items-center gap-1 text-[10px] text-[#6b7974] mt-0.5">
                        <Clock className="w-3 h-3" />
                        {updatedAt.toLocaleDateString()} {updatedAt.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                      </div>
                    )}
                  </button>
                )
              })}
            </div>
          </div>
        )}

        {/* Collapsed session indicator */}
        {sidebarCollapsed && sessions.length > 0 && (
          <div className="w-full px-2 mt-4">
            <div className="border-t border-[#2a3632] pt-2">
              <button
                onClick={() => setSidebarCollapsed(false)}
                className="w-full p-2 text-[#6b7974] hover:text-[#9aa8a3] transition-colors"
                title={locale === "de" ? "Verlauf anzeigen" : "Show history"}
              >
                <Clock className="w-4 h-4 mx-auto" />
              </button>
            </div>
          </div>
        )}

        <div className={`w-full ${sidebarCollapsed ? "px-2 pb-4" : "px-4 pb-6"} space-y-3 mt-auto pt-4`}>
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
