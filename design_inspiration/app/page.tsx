"use client"

import { useState } from "react"
import { ChevronRight, FolderOpen, Settings, Activity, Bell, RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import WorkspacesPage from "./workspaces/page"
import SessionsPage from "./sessions/page"
import AdminPage from "./admin/page"

export default function AgentWorkspacePlatform() {
  const [activeSection, setActiveSection] = useState("workspaces")
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)

  return (
    <div className="flex h-screen">
      {/* Sidebar */}
      <div
        className={`${sidebarCollapsed ? "w-16" : "w-70"} bg-sidebar border-r border-sidebar-border transition-all duration-300 fixed md:relative z-50 md:z-auto h-full md:h-auto ${!sidebarCollapsed ? "md:block" : ""}`}
      >
        <div className="p-4">
          <div className="flex items-center justify-between mb-8">
            <div className={`${sidebarCollapsed ? "hidden" : "block"}`}>
              <h1 className="text-primary font-bold text-lg tracking-wider">AGENT WORKSPACE</h1>
              <p className="text-muted-foreground text-xs">v1.0.0 INTERNAL</p>
            </div>
            <Button
              variant="ghost"
              size="icon"
              onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
              className="text-muted-foreground hover:text-primary"
            >
              <ChevronRight
                className={`w-4 h-4 sm:w-5 sm:h-5 transition-transform ${sidebarCollapsed ? "" : "rotate-180"}`}
              />
            </Button>
          </div>

          <nav className="space-y-2">
            {[
              { id: "workspaces", icon: FolderOpen, label: "WORKSPACES" },
              { id: "sessions", icon: Activity, label: "ACTIVE SESSIONS" },
              { id: "admin", icon: Settings, label: "ADMIN PANEL" },
            ].map((item) => (
              <button
                key={item.id}
                onClick={() => setActiveSection(item.id)}
                className={`w-full flex items-center gap-3 p-3 rounded transition-colors ${
                  activeSection === item.id
                    ? "bg-primary text-primary-foreground"
                    : "text-muted-foreground hover:text-foreground hover:bg-accent"
                }`}
              >
                <item.icon className="w-5 h-5 md:w-5 md:h-5 sm:w-6 sm:h-6" />
                {!sidebarCollapsed && <span className="text-sm font-medium">{item.label}</span>}
              </button>
            ))}
          </nav>

          {!sidebarCollapsed && (
            <div className="mt-8 p-4 bg-card border border-border rounded">
              <div className="flex items-center gap-2 mb-2">
                <div className="w-2 h-2 bg-primary rounded-full animate-pulse"></div>
                <span className="text-xs text-foreground">PLATFORM ONLINE</span>
              </div>
              <div className="text-xs text-muted-foreground">
                <div>UPTIME: 72:14:33</div>
                <div>WORKSPACES: 24 ACTIVE</div>
                <div>SESSIONS: 7 RUNNING</div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Mobile Overlay */}
      {!sidebarCollapsed && (
        <div className="fixed inset-0 bg-black/50 z-40 md:hidden" onClick={() => setSidebarCollapsed(true)} />
      )}

      {/* Main Content */}
      <div className={`flex-1 flex flex-col ${!sidebarCollapsed ? "md:ml-0" : ""}`}>
        {/* Top Toolbar */}
        <div className="h-16 bg-card border-b border-border flex items-center justify-between px-6">
          <div className="flex items-center gap-4">
            <div className="text-sm text-muted-foreground">
              AI AGENT WORKSPACE / <span className="text-primary uppercase">{activeSection}</span>
            </div>
          </div>
          <div className="flex items-center gap-4">
            <div className="text-xs text-muted-foreground">LAST UPDATE: 12/08/2025 20:00 UTC</div>
            <Button variant="ghost" size="icon" className="text-muted-foreground hover:text-primary">
              <Bell className="w-4 h-4" />
            </Button>
            <Button variant="ghost" size="icon" className="text-muted-foreground hover:text-primary">
              <RefreshCw className="w-4 h-4" />
            </Button>
          </div>
        </div>

        {/* Dashboard Content */}
        <div className="flex-1 overflow-auto">
          {activeSection === "workspaces" && <WorkspacesPage />}
          {activeSection === "sessions" && <SessionsPage />}
          {activeSection === "admin" && <AdminPage />}
        </div>
      </div>
    </div>
  )
}
