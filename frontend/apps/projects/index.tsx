"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { FolderKanban, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { useApp } from "@/components/app-context"
import { listWorkspaceDirectories } from "@/lib/control-plane-client"
import { fetchAgents, type OpenCodeAgent } from "@/lib/opencode-client"
import { formatSessionDate } from "@/lib/session-utils"
import { cn } from "@/lib/utils"

type ProjectSummary = {
  key: string
  name: string
  directory?: string
  sessionCount: number
  lastActive: number
}

export function ProjectsApp() {
  const { locale, opencodeSessions, opencodeBaseUrl, setActiveAppId } = useApp()
  const [workspaceDirectories, setWorkspaceDirectories] = useState<{ name: string; path: string }[]>([])
  const [availableAgents, setAvailableAgents] = useState<OpenCodeAgent[]>([])
  const [projectDefaultAgents, setProjectDefaultAgents] = useState<Record<string, string>>(() => {
    if (typeof window === "undefined") return {}
    try {
      const stored = localStorage.getItem("octo:projectDefaultAgents")
      return stored ? JSON.parse(stored) : {}
    } catch {
      return {}
    }
  })
  const [selectedProjectKey, setSelectedProjectKey] = useState<string | null>(null)

  const projectKeyForSession = useCallback(
    (session: { directory?: string | null; projectID?: string | null }) => {
      const directory = session.directory?.trim()
      if (directory) {
        const normalized = directory.replace(/\\/g, "/").replace(/\/+$/, "")
        const parts = normalized.split("/").filter(Boolean)
        return parts[parts.length - 1] ?? directory
      }
      const projectId = session.projectID?.trim()
      if (projectId) return projectId
      return "workspace"
    },
    [],
  )

  const projectLabelForSession = useCallback(
    (session: { directory?: string | null; projectID?: string | null }) => {
      const directory = session.directory?.trim()
      if (directory) {
        const normalized = directory.replace(/\\/g, "/")
        const parts = normalized.split("/").filter(Boolean)
        return parts[parts.length - 1] ?? directory
      }
      const projectId = session.projectID?.trim()
      if (projectId) return projectId
      return locale === "de" ? "Arbeitsbereich" : "Workspace"
    },
    [locale],
  )

  useEffect(() => {
    if (typeof window === "undefined") return
    listWorkspaceDirectories(".")
      .then((entries) => {
        const dirs = entries.map((entry) => ({ name: entry.name, path: entry.path }))
        setWorkspaceDirectories(dirs)
      })
      .catch((err) => {
        console.error("Failed to load workspace directories:", err)
        setWorkspaceDirectories([])
      })
  }, [])

  useEffect(() => {
    if (!opencodeBaseUrl) return
    fetchAgents(opencodeBaseUrl)
      .then((agents) => setAvailableAgents(agents))
      .catch((err) => {
        console.error("Failed to fetch agents:", err)
        setAvailableAgents([])
      })
  }, [opencodeBaseUrl])

  useEffect(() => {
    if (typeof window === "undefined") return
    localStorage.setItem("octo:projectDefaultAgents", JSON.stringify(projectDefaultAgents))
  }, [projectDefaultAgents])

  useEffect(() => {
    if (typeof window === "undefined") return
    const handleFilter = (event: Event) => {
      const customEvent = event as CustomEvent<string>
      if (typeof customEvent.detail === "string") {
        setSelectedProjectKey(customEvent.detail)
      }
    }
    const handleClear = () => setSelectedProjectKey(null)
    const handleDefaultAgent = (event: Event) => {
      const customEvent = event as CustomEvent<{ projectKey: string; agentId: string }>
      if (!customEvent.detail) return
      setProjectDefaultAgents((prev) => ({ ...prev, [customEvent.detail.projectKey]: customEvent.detail.agentId }))
    }

    window.addEventListener("octo:project-filter", handleFilter as EventListener)
    window.addEventListener("octo:project-filter-clear", handleClear as EventListener)
    window.addEventListener("octo:project-default-agent", handleDefaultAgent as EventListener)
    return () => {
      window.removeEventListener("octo:project-filter", handleFilter as EventListener)
      window.removeEventListener("octo:project-filter-clear", handleClear as EventListener)
      window.removeEventListener("octo:project-default-agent", handleDefaultAgent as EventListener)
    }
  }, [])

  const projectSummaries = useMemo(() => {
    const entries = new Map<string, ProjectSummary>()

    for (const directory of workspaceDirectories) {
      entries.set(directory.path, {
        key: directory.path,
        name: directory.name,
        directory: directory.path,
        sessionCount: 0,
        lastActive: 0,
      })
    }

    for (const session of opencodeSessions) {
      if (session.parentID) continue
      const key = projectKeyForSession(session)
      const name = projectLabelForSession(session)
      const lastActive = session.time?.updated ?? 0
      const existing = entries.get(key)
      if (existing) {
        existing.sessionCount += 1
        if (lastActive > existing.lastActive) existing.lastActive = lastActive
      } else {
        entries.set(key, {
          key,
          name,
          directory: session.directory ?? undefined,
          sessionCount: 1,
          lastActive,
        })
      }
    }

    if (!entries.has("workspace")) {
      entries.set("workspace", {
        key: "workspace",
        name: locale === "de" ? "Arbeitsbereich" : "Workspace",
        sessionCount: 0,
        lastActive: 0,
      })
    }

    return [...entries.values()].sort((a, b) => b.lastActive - a.lastActive)
  }, [locale, opencodeSessions, projectKeyForSession, projectLabelForSession, workspaceDirectories])

  const handleSelectProject = useCallback(
    (projectKey: string) => {
      setSelectedProjectKey(projectKey)
      window.dispatchEvent(new CustomEvent("octo:project-filter", { detail: projectKey }))
      setActiveAppId("sessions")
    },
    [setActiveAppId],
  )

  const handleClearFilter = useCallback(() => {
    setSelectedProjectKey(null)
    window.dispatchEvent(new CustomEvent("octo:project-filter-clear"))
  }, [])

  const handleDefaultAgentChange = useCallback((projectKey: string, agentId: string) => {
    setProjectDefaultAgents((prev) => {
      if (!agentId) {
        const next = { ...prev }
        delete next[projectKey]
        return next
      }
      return { ...prev, [projectKey]: agentId }
    })
    window.dispatchEvent(
      new CustomEvent("octo:project-default-agent", { detail: { projectKey, agentId } }),
    )
  }, [])

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">
            {locale === "de" ? "Projekte" : "Projects"}
          </h1>
          <p className="text-sm text-muted-foreground">
            {locale === "de"
              ? "Arbeitsverzeichnisse und zugeordnete Chats"
              : "Workspace directories and their chat activity"}
          </p>
        </div>
        {selectedProjectKey && (
          <Button variant="outline" size="sm" onClick={handleClearFilter}>
            <X className="w-4 h-4 mr-2" />
            {locale === "de" ? "Filter loschen" : "Clear filter"}
          </Button>
        )}
      </div>

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        {projectSummaries.length === 0 ? (
          <div className="col-span-full text-center text-muted-foreground py-12">
            {locale === "de" ? "Keine Projekte gefunden" : "No projects found"}
          </div>
        ) : (
          projectSummaries.map((project) => {
            const lastActiveLabel = project.lastActive
              ? formatSessionDate(project.lastActive)
              : locale === "de"
                ? "Nie"
                : "Never"
            const defaultAgent = projectDefaultAgents[project.key]
            const isSelected = selectedProjectKey === project.key
            return (
              <div
                key={project.key}
                className={cn(
                  "border rounded-lg p-4 space-y-3 bg-card",
                  isSelected ? "border-primary" : "border-border",
                )}
              >
                <div className="flex items-start gap-3">
                  <div className="w-10 h-10 rounded-md bg-primary/10 flex items-center justify-center">
                    <FolderKanban className="w-5 h-5 text-primary" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="text-base font-semibold truncate">{project.name}</div>
                    <div className="text-xs text-muted-foreground mt-1">
                      {project.sessionCount} {locale === "de" ? "Chats" : "chats"} · {lastActiveLabel}
                    </div>
                  </div>
                </div>

                <div className="text-xs text-muted-foreground">
                  {locale === "de" ? "Standard-Agent" : "Default agent"}: {defaultAgent || "-"}
                </div>

                <select
                  value={defaultAgent || ""}
                  onChange={(e) => handleDefaultAgentChange(project.key, e.target.value)}
                  className="w-full text-xs bg-muted border border-border rounded px-2 py-1"
                >
                  <option value="">
                    {locale === "de" ? "Standard-Agent setzen" : "Set default agent"}
                  </option>
                  {availableAgents.map((agent) => (
                    <option key={agent.id} value={agent.id}>
                      {agent.name || agent.id}
                    </option>
                  ))}
                </select>

                <Button variant="secondary" size="sm" onClick={() => handleSelectProject(project.key)}>
                  {locale === "de" ? "Chats filtern" : "Filter chats"}
                </Button>
              </div>
            )
          })
        )}
      </div>
    </div>
  )
}
