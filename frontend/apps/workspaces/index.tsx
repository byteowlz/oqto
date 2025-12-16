"use client"

import { useState, useMemo } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Search, Plus, ChevronLeft } from "lucide-react"
import { useApp } from "@/components/app-context"
import { cn } from "@/lib/utils"

export function WorkspacesApp() {
  const { locale } = useApp()
  const [searchTerm, setSearchTerm] = useState("")
  const [selectedWorkspace, setSelectedWorkspace] = useState<string | null>(null)
  const [mobileView, setMobileView] = useState<"list" | "details">("list")

  const copy = useMemo(
    () => ({
      de: {
        searchPlaceholder: "Agents durchsuchen",
        createAgent: "Agent erstellen",
        lastModified: "Zuletzt geändert",
        scope: "Umfang",
        files: "Dateien",
        templates: "Agenten-Templates",
        startAgent: "Agent starten",
        persona: "Persona",
        back: "Zurück",
        selectWorkspace: "Wähle einen Workspace aus der Liste",
      },
      en: {
        searchPlaceholder: "Search agents",
        createAgent: "Create Agent",
        lastModified: "Last modified",
        scope: "Scope",
        files: "files",
        templates: "Agent Templates",
        startAgent: "Start Agent",
        persona: "Persona",
        back: "Back",
        selectWorkspace: "Select a workspace from the list",
      },
    }),
    [],
  )
  const t = copy[locale]

  const workspaces = [
    {
      id: "ws-001",
      name: "Customer Analytics Dashboard",
      lastModified: "2 hours ago",
      files: 47,
      size: "234 MB",
    },
    {
      id: "ws-002",
      name: "Q4 Financial Reports",
      lastModified: "5 hours ago",
      files: 32,
      size: "156 MB",
    },
    {
      id: "ws-003",
      name: "Product Documentation",
      lastModified: "1 day ago",
      files: 128,
      size: "512 MB",
    },
    {
      id: "ws-004",
      name: "API Integration Project",
      lastModified: "3 days ago",
      files: 89,
      size: "421 MB",
    },
    {
      id: "ws-005",
      name: "Marketing Campaign Assets",
      lastModified: "1 week ago",
      files: 203,
      size: "1.2 GB",
    },
    {
      id: "ws-006",
      name: "Research Papers Collection",
      lastModified: "2 weeks ago",
      files: 67,
      size: "892 MB",
    },
  ]

  const templates = [
    {
      id: "coding-copilot",
      name: "Coding Copilot",
      tag: "[DEV]",
      description: locale === "de" 
        ? "Vollständige Entwicklungsumgebung mit Terminal, Git und Code-Editing."
        : "Full development environment with terminal, Git and code editing.",
      persona: "Senior Software Engineer",
      capabilities: locale === "de"
        ? ["Shell Access", "Git", "Code Editing", "Netzwerkzugriff"]
        : ["Shell Access", "Git", "Code Editing", "Network Access"],
    },
    {
      id: "research-assistant",
      name: "Research Assistant",
      tag: "[RES]",
      description: locale === "de"
        ? "Dokumenten-Analyse und Recherche in sicherer Umgebung."
        : "Document analysis and research in a safe environment.",
      persona: "Research Analyst",
      capabilities: locale === "de"
        ? ["Dokumenten-lesen", "Websuche", "Zusammenfassung"]
        : ["Document Reading", "Web Search", "Summarization"],
    },
    {
      id: "meeting-synth",
      name: "Meeting Synthesizer",
      tag: "[MTG]",
      description: locale === "de"
        ? "Transkript-Analyse und Meeting-Zusammenfassungen."
        : "Transcript analysis and meeting summaries.",
      persona: "Executive Assistant",
      capabilities: locale === "de"
        ? ["Transkript Parsing", "Action Items", "Kein Code"]
        : ["Transcript Parsing", "Action Items", "No Code"],
    },
  ]

  const filteredWorkspaces = workspaces.filter((ws) => ws.name.toLowerCase().includes(searchTerm.toLowerCase()))
  const selectedWs = workspaces.find((ws) => ws.id === selectedWorkspace)

  const handleWorkspaceSelect = (workspaceId: string) => {
    setSelectedWorkspace(workspaceId)
    setMobileView("details")
  }

  // Workspace List Component
  const WorkspaceList = (
    <div className="flex flex-col h-full bg-background">
      {/* Search */}
      <div className="p-4">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <Input
            placeholder={t.searchPlaceholder}
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="pl-9 h-10 bg-transparent border-border text-foreground placeholder:text-muted-foreground"
          />
        </div>
      </div>

      {/* Create Agent Button */}
      <div className="px-4 pb-4">
        <button className="w-full flex items-center justify-between px-4 py-3 text-sm text-foreground hover:bg-primary/10 transition-colors">
          <span className="flex items-center gap-2">
            <Plus className="w-4 h-4" />
            {t.createAgent}
          </span>
        </button>
      </div>

      {/* Workspace List */}
      <div className="flex-1 overflow-y-auto px-4 space-y-2">
        {filteredWorkspaces.map((workspace) => {
          const isSelected = selectedWorkspace === workspace.id
          return (
            <button
              key={workspace.id}
              onClick={() => handleWorkspaceSelect(workspace.id)}
              className={cn(
                "w-full text-left p-3 transition-colors",
                isSelected 
                  ? "bg-primary text-primary-foreground" 
                  : "text-foreground hover:bg-primary/10"
              )}
            >
              <div className="font-medium text-sm">{workspace.name}</div>
              <div className={cn(
                "text-xs mt-1",
                isSelected ? "text-primary-foreground/70" : "text-muted-foreground"
              )}>
                {workspace.lastModified}
              </div>
            </button>
          )
        })}
      </div>
    </div>
  )

  // Workspace Details + Templates Component
  const WorkspaceDetails = (
    <div className="flex flex-col h-full bg-background">
      {selectedWs ? (
        <>
          {/* Workspace Header */}
          <div className="p-4 md:p-6 border-b border-border">
            <div className="flex items-center gap-3">
              <button
                onClick={() => setMobileView("list")}
                className="md:hidden p-2 -ml-2 text-muted-foreground hover:text-foreground"
              >
                <ChevronLeft className="w-5 h-5" />
              </button>
              <div>
                <h1 className="text-lg md:text-xl font-semibold text-foreground">{selectedWs.name}</h1>
                <div className="text-sm text-muted-foreground mt-1">
                  {t.lastModified}: {selectedWs.lastModified}
                </div>
                <div className="text-sm text-muted-foreground">
                  {t.scope}: {selectedWs.files} {t.files}, {selectedWs.size}
                </div>
              </div>
            </div>
          </div>

          {/* Templates Section */}
          <div className="flex-1 overflow-y-auto p-4 md:p-6">
            <h2 className="text-sm font-medium text-muted-foreground mb-4">{t.templates}</h2>
            
            <div className="space-y-4 md:space-y-6">
              {templates.map((template) => (
                <div
                  key={template.id}
                  className="border border-border bg-card p-4"
                >
                  <div className="flex items-start gap-2 mb-2">
                    <span className="text-primary font-mono text-sm">{template.tag}</span>
                    <span className="text-foreground font-medium">{template.name}</span>
                  </div>
                  
                  <p className="text-sm text-muted-foreground mb-3">
                    {template.description}
                  </p>
                  
                  <div className="text-sm mb-3">
                    <span className="text-muted-foreground">{t.persona}: </span>
                    <span className="text-primary">{template.persona}</span>
                  </div>
                  
                  <div className="flex flex-wrap gap-2 mb-4">
                    {template.capabilities.map((cap) => (
                      <span 
                        key={cap} 
                        className="text-xs px-2 py-1 bg-muted text-muted-foreground"
                      >
                        {cap}
                      </span>
                    ))}
                  </div>

                  <Button 
                    className="w-full bg-primary hover:bg-primary/90 text-primary-foreground"
                  >
                    {t.startAgent}
                  </Button>
                </div>
              ))}
            </div>
          </div>
        </>
      ) : (
        /* Empty State */
        <div className="flex-1 flex items-center justify-center p-6">
          <div className="text-center text-muted-foreground">
            <p className="text-sm">{t.selectWorkspace}</p>
          </div>
        </div>
      )}
    </div>
  )

  return (
    <>
      {/* Mobile Layout - Show one view at a time */}
      <div className="md:hidden h-full w-full overflow-hidden">
        {mobileView === "list" ? WorkspaceList : WorkspaceDetails}
      </div>

      {/* Desktop Layout - Side by side */}
      <div className="hidden md:flex h-full w-full overflow-hidden">
        {/* Column 1: Workspace List */}
        <div className="w-[280px] min-w-[280px] border-r border-border">
          {WorkspaceList}
        </div>

        {/* Column 2: Workspace Details + Templates */}
        <div className="flex-1">
          {WorkspaceDetails}
        </div>
      </div>
    </>
  )
}

export default WorkspacesApp
