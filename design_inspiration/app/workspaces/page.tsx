"use client"

import { useState } from "react"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Search, Plus, FolderOpen, Clock, MoreHorizontal } from "lucide-react"

export default function WorkspacesPage() {
  const [searchTerm, setSearchTerm] = useState("")
  const [selectedWorkspace, setSelectedWorkspace] = useState<string | null>(null)

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

  const filteredWorkspaces = workspaces.filter((ws) => ws.name.toLowerCase().includes(searchTerm.toLowerCase()))

  return (
    <div className="p-6 space-y-6 bg-background min-h-screen">
      {/* Header */}
      <div className="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4">
        <div>
          <h1 className="text-2xl font-bold text-foreground tracking-wider">MY WORKSPACES</h1>
          <p className="text-sm text-muted-foreground">Select a workspace to start an agent session</p>
        </div>
        <Button className="bg-primary hover:bg-primary/90 text-primary-foreground">
          <Plus className="w-4 h-4 mr-2" />
          New Workspace
        </Button>
      </div>

      {/* Search */}
      <Card className="bg-card border-border">
        <CardContent className="p-4">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <Input
              placeholder="Search workspaces..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="pl-10 bg-background border-input text-foreground placeholder-muted-foreground"
            />
          </div>
        </CardContent>
      </Card>

      {/* Workspace Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {filteredWorkspaces.map((workspace) => (
          <Card
            key={workspace.id}
            className="bg-card border-border hover:border-primary transition-all cursor-pointer group"
            onClick={() => setSelectedWorkspace(workspace.id)}
          >
            <CardHeader className="pb-3">
              <div className="flex items-start justify-between">
                <FolderOpen className="w-8 h-8 text-primary mb-2" />
                <Button
                  variant="ghost"
                  size="icon"
                  className="text-muted-foreground hover:text-primary opacity-0 group-hover:opacity-100 transition-opacity"
                  onClick={(e) => {
                    e.stopPropagation()
                  }}
                >
                  <MoreHorizontal className="w-4 h-4" />
                </Button>
              </div>
              <CardTitle className="text-base text-foreground">{workspace.name}</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-2 text-sm text-muted-foreground">
                <div className="flex items-center gap-2">
                  <Clock className="w-3 h-3" />
                  <span className="text-xs">{workspace.lastModified}</span>
                </div>
                <div className="flex justify-between text-xs">
                  <span>{workspace.files} files</span>
                  <span className="text-muted-foreground">{workspace.size}</span>
                </div>
              </div>
              <Button
                className="w-full mt-4 bg-primary hover:bg-primary/90 text-primary-foreground"
                onClick={(e) => {
                  e.stopPropagation()
                  setSelectedWorkspace(workspace.id)
                }}
              >
                Open Workspace
              </Button>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* Agent Template Selection Modal */}
      {selectedWorkspace && (
        <AgentTemplateSelector workspaceId={selectedWorkspace} onClose={() => setSelectedWorkspace(null)} />
      )}
    </div>
  )
}

function AgentTemplateSelector({ workspaceId, onClose }: { workspaceId: string; onClose: () => void }) {
  const templates = [
    {
      id: "coding-copilot",
      name: "Coding Copilot",
      description: "Full development environment with terminal access, Git, and code editing tools",
      persona: "Senior Software Engineer",
      capabilities: ["Shell Access", "Git Operations", "Code Editing", "Full Network Access"],
      icon: "💻",
    },
    {
      id: "research-assistant",
      name: "Research Assistant",
      description: "Document analysis and research tool with read-only access and safe environment",
      persona: "Research Analyst",
      capabilities: ["Document Reading", "Web Search", "Summarization", "Safe Environment"],
      icon: "📚",
    },
    {
      id: "meeting-synth",
      name: "Meeting Synthesizer",
      description: "Transcript analysis and meeting summary generation",
      persona: "Executive Secretary",
      capabilities: ["Transcript Parsing", "Summarization", "Action Items", "No Code Execution"],
      icon: "📝",
    },
  ]

  return (
    <div className="fixed inset-0 bg-black/70 flex items-center justify-center p-4 z-50">
      <Card className="bg-card border-border w-full max-w-4xl max-h-[90vh] overflow-y-auto">
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="text-xl font-bold text-foreground tracking-wider">SELECT AGENT TEMPLATE</CardTitle>
              <p className="text-sm text-muted-foreground mt-1">Choose an AI agent configuration for this workspace</p>
            </div>
            <Button variant="ghost" onClick={onClose} className="text-muted-foreground hover:text-foreground">
              ✕
            </Button>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {templates.map((template) => (
            <Card
              key={template.id}
              className="bg-muted border-border hover:border-primary transition-all cursor-pointer"
            >
              <CardContent className="p-6">
                <div className="flex items-start gap-4">
                  <div className="text-4xl">{template.icon}</div>
                  <div className="flex-1">
                    <h3 className="text-lg font-bold text-foreground mb-1">{template.name}</h3>
                    <p className="text-sm text-muted-foreground mb-3">{template.description}</p>
                    <div className="mb-3">
                      <span className="text-xs text-muted-foreground uppercase tracking-wider">Persona: </span>
                      <span className="text-xs text-primary">{template.persona}</span>
                    </div>
                    <div className="flex flex-wrap gap-2 mb-4">
                      {template.capabilities.map((cap) => (
                        <span
                          key={cap}
                          className="text-xs px-2 py-1 bg-accent text-accent-foreground rounded tracking-wider"
                        >
                          {cap}
                        </span>
                      ))}
                    </div>
                    <Button className="bg-primary hover:bg-primary/90 text-primary-foreground">Start Session</Button>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </CardContent>
      </Card>
    </div>
  )
}
