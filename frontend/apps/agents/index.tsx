"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Save, Search } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { useApp } from "@/components/app-context"
import { fileserverProxyBaseUrl } from "@/lib/control-plane-client"
import {
  fetchAgents,
  sendPartsAsync,
  type OpenCodeAgent,
} from "@/lib/opencode-client"
import { cn } from "@/lib/utils"
import { PersonaBuilderChat } from "@/components/persona-builder-chat"

type AgentFormState = {
  id: string
  name: string
  description: string
  mode: "primary" | "subagent"
  model: string
  prompt: string
  tools: string[]
  permissions: string[]
}

const toolOptions = ["bash", "browser", "filesystem", "terminal", "git", "search"]
const permissionOptions = ["filesystem", "network", "shell", "clipboard", "browser"]

const emptyForm: AgentFormState = {
  id: "",
  name: "",
  description: "",
  mode: "primary",
  model: "",
  prompt: "",
  tools: [],
  permissions: [],
}

function buildAgentMarkdown(form: AgentFormState): string {
  const tools = form.tools.length
    ? `tools:\n${form.tools.map((tool) => `  - ${tool}`).join("\n")}`
    : "tools: []"
  const permissions = form.permissions.length
    ? `permissions:\n${form.permissions.map((perm) => `  - ${perm}`).join("\n")}`
    : "permissions: []"
  return `---\nname: "${form.name}"\ndescription: "${form.description}"\nmode: "${form.mode}"\nmodel: "${form.model}"\n${tools}\n${permissions}\n---\n\n${form.prompt.trim()}\n`
}

function parseAgentMarkdown(content: string): AgentFormState {
  const trimmed = content.trimStart()
  if (!trimmed.startsWith("---")) {
    return { ...emptyForm, prompt: content.trim() }
  }

  const endIndex = trimmed.indexOf("\n---", 3)
  if (endIndex === -1) {
    return { ...emptyForm, prompt: content.trim() }
  }

  const frontmatter = trimmed.slice(3, endIndex).trim()
  const body = trimmed.slice(endIndex + 4).trim()
  const lines = frontmatter.split("\n")
  const parsed: Record<string, string | string[]> = {}
  let currentList: string | null = null

  for (const line of lines) {
    const trimmedLine = line.trim()
    if (!trimmedLine) continue
    if (trimmedLine.startsWith("-")) {
      if (currentList) {
        const value = trimmedLine.replace(/^-\s*/, "")
        const list = (parsed[currentList] as string[]) ?? []
        list.push(value)
        parsed[currentList] = list
      }
      continue
    }
    const [key, ...rest] = trimmedLine.split(":")
    const value = rest.join(":").trim().replace(/^"|"$/g, "")
    if (value) {
      if (value === "[]") {
        parsed[key] = []
        currentList = null
        continue
      }
      parsed[key] = value
      currentList = null
    } else {
      parsed[key] = []
      currentList = key
    }
  }

  return {
    id: "",
    name: (parsed.name as string) || "",
    description: (parsed.description as string) || "",
    mode: (parsed.mode as "primary" | "subagent") || "primary",
    model: (parsed.model as string) || "",
    prompt: body,
    tools: Array.isArray(parsed.tools) ? (parsed.tools as string[]) : [],
    permissions: Array.isArray(parsed.permissions) ? (parsed.permissions as string[]) : [],
  }
}

export function AgentsApp() {
  const { locale, opencodeBaseUrl, selectedWorkspaceSession, createNewChat, refreshOpencodeSessions, setActiveAppId, authToken } = useApp()
  const [agents, setAgents] = useState<OpenCodeAgent[]>([])
  const [search, setSearch] = useState("")
  const [loading, setLoading] = useState(false)
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null)
  const [form, setForm] = useState<AgentFormState>(emptyForm)
  const [saving, setSaving] = useState(false)
  const [startingAgentId, setStartingAgentId] = useState<string | null>(null)

  const copy = useMemo(
    () => ({
      de: {
        title: "Agenten",
        subtitle: "OpenCode-Agenten erstellen und bearbeiten.",
        search: "Agenten suchen",
        noAgents: "Keine Agenten gefunden",
        start: "Chat starten",
        starting: "Starte...",
        create: "Neu",
        save: "Speichern",
      },
      en: {
        title: "Agents",
        subtitle: "Create and edit OpenCode agents.",
        search: "Search agents",
        noAgents: "No agents found",
        start: "Start chat",
        starting: "Starting...",
        create: "New",
        save: "Save",
      },
    }),
    [],
  )
  const t = copy[locale]

  useEffect(() => {
    if (!opencodeBaseUrl) return
    setLoading(true)
    fetchAgents(opencodeBaseUrl)
      .then((list) => setAgents(list))
      .catch((err) => {
        console.error("Failed to fetch agents:", err)
        setAgents([])
      })
      .finally(() => setLoading(false))
  }, [opencodeBaseUrl])

  const filteredAgents = useMemo(() => {
    const term = search.trim().toLowerCase()
    if (!term) return agents
    return agents.filter((agent) => {
      return (
        agent.id.toLowerCase().includes(term) ||
        agent.name?.toLowerCase().includes(term) ||
        agent.description?.toLowerCase().includes(term)
      )
    })
  }, [agents, search])

  const loadAgent = useCallback(
    async (agentId: string) => {
      if (!selectedWorkspaceSession?.id || typeof window === "undefined") return
      const baseUrl = fileserverProxyBaseUrl(selectedWorkspaceSession.id)
      const url = new URL(`${baseUrl}/file`, window.location.origin)
      url.searchParams.set("path", `.opencode/agent/${agentId}.md`)

      try {
        const res = await fetch(url.toString(), { cache: "no-store", credentials: "include" })
        if (!res.ok) {
          throw new Error(await res.text().catch(() => res.statusText))
        }
        const content = await res.text()
        const parsed = parseAgentMarkdown(content)
        setForm({ ...parsed, id: agentId })
      } catch (err) {
        console.error("Failed to load agent file:", err)
        setForm((prev) => ({ ...prev, id: agentId, name: agentId }))
      }
    },
    [selectedWorkspaceSession?.id],
  )

  useEffect(() => {
    if (!selectedAgentId) return
    void loadAgent(selectedAgentId)
  }, [loadAgent, selectedAgentId])

  const handleStartChat = useCallback(
    async (agentId: string) => {
      if (!opencodeBaseUrl) return
      setStartingAgentId(agentId)
      try {
        const session = await createNewChat()
        if (session) {
          await sendPartsAsync(opencodeBaseUrl, session.id, [{ type: "agent", name: agentId }])
          await refreshOpencodeSessions()
          setActiveAppId("sessions")
        }
      } catch (err) {
        console.error("Failed to start agent chat:", err)
      } finally {
        setStartingAgentId(null)
      }
    },
    [createNewChat, opencodeBaseUrl, refreshOpencodeSessions, setActiveAppId],
  )

  const handleSave = useCallback(async () => {
    if (!selectedWorkspaceSession?.id || typeof window === "undefined") return
    if (!form.id.trim()) return
    setSaving(true)
    const baseUrl = fileserverProxyBaseUrl(selectedWorkspaceSession.id)
    const url = new URL(`${baseUrl}/file`, window.location.origin)
    url.searchParams.set("path", `.opencode/agent/${form.id.trim()}.md`)
    url.searchParams.set("mkdir", "true")

    const content = buildAgentMarkdown({ ...form, id: form.id.trim() })
    const data = new FormData()
    data.append("file", new Blob([content], { type: "text/markdown" }), `${form.id.trim()}.md`)

    try {
      const res = await fetch(url.toString(), { method: "POST", credentials: "include", body: data })
      if (!res.ok) {
        const text = await res.text().catch(() => res.statusText)
        throw new Error(text || `Save failed (${res.status})`)
      }
      if (opencodeBaseUrl) {
        await fetchAgents(opencodeBaseUrl)
          .then((list) => setAgents(list))
          .catch(() => null)
      }
    } catch (err) {
      console.error("Failed to save agent:", err)
    } finally {
      setSaving(false)
    }
  }, [form, opencodeBaseUrl, selectedWorkspaceSession?.id])

  const toggleListValue = (values: string[], value: string) =>
    values.includes(value) ? values.filter((item) => item !== value) : [...values, value]

  return (
    <div className="p-6 space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">{t.title}</h1>
        <p className="text-sm text-muted-foreground">{t.subtitle}</p>
      </div>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(0,0.7fr)]">
        <div className="space-y-4">
          <div className="flex items-center gap-2">
            <div className="relative flex-1">
              <Search className="w-4 h-4 text-muted-foreground absolute left-3 top-1/2 -translate-y-1/2" />
              <Input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder={t.search}
                className="pl-9"
              />
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setSelectedAgentId(null)
                setForm(emptyForm)
              }}
            >
              {t.create}
            </Button>
          </div>

          <div className="space-y-3">
            {loading ? (
              <div className="text-sm text-muted-foreground">Loading...</div>
            ) : filteredAgents.length === 0 ? (
              <div className="text-sm text-muted-foreground">{t.noAgents}</div>
            ) : (
              filteredAgents.map((agent) => {
                const isSelected = selectedAgentId === agent.id
                const isStarting = startingAgentId === agent.id
                return (
                  <div
                    key={agent.id}
                    className={cn(
                      "border rounded-lg p-3 flex items-center justify-between gap-3",
                      isSelected ? "border-primary" : "border-border",
                    )}
                  >
                    <button
                      onClick={() => setSelectedAgentId(agent.id)}
                      className="text-left flex-1 min-w-0"
                    >
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-semibold truncate">{agent.name || agent.id}</span>
                        {agent.mode && (
                          <span className="text-[10px] px-2 py-0.5 rounded-full bg-muted text-muted-foreground">
                            {agent.mode}
                          </span>
                        )}
                      </div>
                      <div className="text-xs text-muted-foreground truncate">
                        {agent.description || agent.id}
                      </div>
                    </button>
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => handleStartChat(agent.id)}
                      disabled={isStarting}
                    >
                      {isStarting ? t.starting : t.start}
                    </Button>
                  </div>
                )
              })
            )}
          </div>
        </div>

        <div className="space-y-4">
          <div className="grid gap-3">
            <div className="grid gap-2">
              <label className="text-xs uppercase text-muted-foreground">Agent ID</label>
              <Input
                value={form.id}
                onChange={(event) => setForm((prev) => ({ ...prev, id: event.target.value }))}
              />
            </div>
            <div className="grid gap-2">
              <label className="text-xs uppercase text-muted-foreground">Name</label>
              <Input
                value={form.name}
                onChange={(event) => setForm((prev) => ({ ...prev, name: event.target.value }))}
              />
            </div>
            <div className="grid gap-2">
              <label className="text-xs uppercase text-muted-foreground">Description</label>
              <Input
                value={form.description}
                onChange={(event) => setForm((prev) => ({ ...prev, description: event.target.value }))}
              />
            </div>
            <div className="grid gap-2">
              <label className="text-xs uppercase text-muted-foreground">Mode</label>
              <select
                value={form.mode}
                onChange={(event) => setForm((prev) => ({ ...prev, mode: event.target.value as "primary" | "subagent" }))}
                className="w-full text-sm bg-muted border border-border rounded px-2 py-1"
              >
                <option value="primary">Primary</option>
                <option value="subagent">Subagent</option>
              </select>
            </div>
            <div className="grid gap-2">
              <label className="text-xs uppercase text-muted-foreground">Model</label>
              <Input
                value={form.model}
                onChange={(event) => setForm((prev) => ({ ...prev, model: event.target.value }))}
                placeholder="provider/model"
              />
            </div>
            <div className="grid gap-2">
              <label className="text-xs uppercase text-muted-foreground">Prompt</label>
              <Textarea
                value={form.prompt}
                onChange={(event) => setForm((prev) => ({ ...prev, prompt: event.target.value }))}
                rows={8}
              />
            </div>
          </div>

          <div className="grid gap-3">
            <div className="text-xs uppercase text-muted-foreground">Tools</div>
            <div className="flex flex-wrap gap-2">
              {toolOptions.map((tool) => (
                <button
                  key={tool}
                  type="button"
                  onClick={() => setForm((prev) => ({ ...prev, tools: toggleListValue(prev.tools, tool) }))}
                  className={cn(
                    "text-xs px-2 py-1 rounded border",
                    form.tools.includes(tool) ? "border-primary text-primary" : "border-border text-muted-foreground",
                  )}
                >
                  {tool}
                </button>
              ))}
            </div>
          </div>

          <div className="grid gap-3">
            <div className="text-xs uppercase text-muted-foreground">Permissions</div>
            <div className="flex flex-wrap gap-2">
              {permissionOptions.map((permission) => (
                <button
                  key={permission}
                  type="button"
                  onClick={() => setForm((prev) => ({ ...prev, permissions: toggleListValue(prev.permissions, permission) }))}
                  className={cn(
                    "text-xs px-2 py-1 rounded border",
                    form.permissions.includes(permission) ? "border-primary text-primary" : "border-border text-muted-foreground",
                  )}
                >
                  {permission}
                </button>
              ))}
            </div>
          </div>

          <Button onClick={handleSave} disabled={saving} className="self-start">
            <Save className="w-4 h-4 mr-2" />
            {t.save}
          </Button>
        </div>

        <div className="border border-border rounded-lg bg-muted/40 overflow-hidden h-[500px]">
          <PersonaBuilderChat
            opencodeBaseUrl={opencodeBaseUrl}
            authToken={authToken}
            onPersonaCreated={(personaId) => {
              // Could refresh agents list or navigate to persona
              console.log("Persona created:", personaId)
            }}
          />
        </div>
      </div>
    </div>
  )
}
