"use client"

import { useMemo, useState } from "react"
import { Plus, Search, X, Users, CalendarRange, Layers, CheckCircle2, PauseCircle, PlayCircle, Trash2, ChevronLeft } from "lucide-react"
import { useApp } from "@/components/app-context"
import { cn } from "@/lib/utils"

type DocumentItem = { id: string; label: string; completed: boolean }
type FunnelStep = { id: string; label: string; documents: DocumentItem[] }
type ProjectType = "Industrieprojekt" | "Forschungsprojekt"
type ProjectStatus = "aktiv" | "pausiert" | "beendet"

type Project = {
  id: string
  name: string
  customer: string
  type: ProjectType
  startDate: string
  endDate: string
  status: ProjectStatus
  steps: FunnelStep[]
}

const funnelTemplateDe: Omit<FunnelStep, "documents">[] = [
  { id: "akquise", label: "Akquise" },
  { id: "angebot", label: "Angebot" },
  { id: "kickoff", label: "Kickoff" },
  { id: "meilenstein-1", label: "Meilenstein 1" },
  { id: "zwischen", label: "Zwischenergebnisse" },
  { id: "meilenstein-2", label: "Meilenstein 2" },
  { id: "abschluss", label: "Abschluss" },
  { id: "verwertung", label: "Verwertung" },
]

const funnelTemplateEn: Omit<FunnelStep, "documents">[] = [
  { id: "akquise", label: "Acquisition" },
  { id: "angebot", label: "Proposal" },
  { id: "kickoff", label: "Kickoff" },
  { id: "meilenstein-1", label: "Milestone 1" },
  { id: "zwischen", label: "Interim results" },
  { id: "meilenstein-2", label: "Milestone 2" },
  { id: "abschluss", label: "Closure" },
  { id: "verwertung", label: "Exploitation" },
]

const documentTemplatesDe: Record<string, string[]> = {
  akquise: ["Lead-Notiz", "Bedarfsskizze"],
  angebot: ["Informelles Angebot", "Formelles Angebot"],
  kickoff: ["Kickoff-Agenda", "Kickoff-Protokoll"],
  "meilenstein-1": ["Meilensteinbericht", "Review-Protokoll"],
  zwischen: ["Zwischenbericht", "Risikoupdate"],
  "meilenstein-2": ["Meilensteinabschluss", "Lessons Learned"],
  abschluss: ["Abschlussbericht", "Abschlussrechnung"],
  verwertung: ["Transfer-Plan", "Nutzungskonzept"],
}

const documentTemplatesEn: Record<string, string[]> = {
  akquise: ["Lead note", "Needs sketch"],
  angebot: ["Informal proposal", "Formal proposal"],
  kickoff: ["Kickoff agenda", "Kickoff minutes"],
  "meilenstein-1": ["Milestone report", "Review minutes"],
  zwischen: ["Interim report", "Risk update"],
  "meilenstein-2": ["Milestone closure", "Lessons learned"],
  abschluss: ["Closure report", "Final invoice"],
  verwertung: ["Transfer plan", "Exploitation concept"],
}

function buildSteps(locale: string): FunnelStep[] {
  const template = locale === "en" ? funnelTemplateEn : funnelTemplateDe
  const docs = locale === "en" ? documentTemplatesEn : documentTemplatesDe
  return template.map((step) => ({
    ...step,
    documents: (docs[step.id] || []).map((label) => ({
      id: `${step.id}-${label.toLowerCase().replace(/\s+/g, "-")}`,
      label,
      completed: false,
    })),
  }))
}

export function ProjectsApp() {
  const { locale, setActiveAppId } = useApp()
  const [projects, setProjects] = useState<Project[]>([])
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null)
  const [search, setSearch] = useState("")
  const [showDialog, setShowDialog] = useState(false)
  const [mobileView, setMobileView] = useState<"list" | "funnel">("list")
  const [form, setForm] = useState<{
    name: string
    customer: string
    type: ProjectType
    startDate: string
    endDate: string
    status: ProjectStatus
  }>({
    name: "",
    customer: "",
    type: "Industrieprojekt",
    startDate: "",
    endDate: "",
    status: "aktiv",
  })

  const copy = useMemo(
    () => ({
      de: {
        title: "Projekte",
        search: "Projekte durchsuchen",
        create: "Neues Projekt erstellen",
        funnel: "Dokumenten-Funnel",
        newProject: "Neues Projekt erstellen",
        emptyProjects: "Noch keine Projekte angelegt.",
        projectLabel: "Projektbezeichnung",
        typeLabel: "Projektart",
        customerLabel: "Kunde",
        startLabel: "Startdatum",
        endLabel: "Enddatum",
        save: "Speichern",
        cancel: "Abbrechen",
        selectProject: "Wähle ein Projekt aus, um den Dokumenten-Funnel zu sehen.",
        status: "Status",
        progress: "Fortschritt",
        active: "Aktiv",
        paused: "Pausiert",
        finished: "Beendet",
        back: "Zurück",
      },
      en: {
        title: "Projects",
        search: "Search projects",
        create: "Create new project",
        funnel: "Document Funnel",
        newProject: "Create new project",
        emptyProjects: "No projects yet.",
        projectLabel: "Project name",
        typeLabel: "Project type",
        customerLabel: "Customer",
        startLabel: "Start date",
        endLabel: "End date",
        save: "Save",
        cancel: "Cancel",
        selectProject: "Select a project to view the document funnel.",
        status: "Status",
        progress: "Progress",
        active: "Active",
        paused: "Paused",
        finished: "Finished",
        back: "Back",
      },
    }),
    [],
  )[locale]

  const statusLabels = {
    aktiv: copy.active,
    pausiert: copy.paused,
    beendet: copy.finished,
  }

  const projectTypes = locale === "de"
    ? ["Industrieprojekt", "Forschungsprojekt"]
    : ["Industry project", "Research project"]

  const filteredProjects = projects.filter((project) =>
    [project.name, project.customer].some((value) => value.toLowerCase().includes(search.toLowerCase())),
  )

  const activeProject = projects.find((p) => p.id === activeProjectId) ?? null
  
  const statusCounts = useMemo(() => {
    return projects.reduce(
      (acc, project) => {
        if (project.status === "aktiv") acc.active += 1
        if (project.status === "pausiert") acc.paused += 1
        if (project.status === "beendet") acc.done += 1
        return acc
      },
      { active: 0, paused: 0, done: 0 },
    )
  }, [projects])

  const handleCreateProject = (e: React.FormEvent) => {
    e.preventDefault()
    if (!form.name.trim()) return
    const newProject: Project = {
      id: crypto.randomUUID(),
      name: form.name.trim(),
      customer: form.customer.trim(),
      type: form.type,
      startDate: form.startDate,
      endDate: form.endDate,
      status: form.status,
      steps: buildSteps(locale),
    }
    setProjects((prev) => [...prev, newProject])
    setActiveProjectId(newProject.id)
    setMobileView("funnel")
    setShowDialog(false)
    setForm({ name: "", customer: "", type: "Industrieprojekt", startDate: "", endDate: "", status: "aktiv" })
  }

  const getProgress = (project: Project) => {
    const total = project.steps.reduce((acc, step) => acc + step.documents.length, 0)
    const done = project.steps.reduce(
      (acc, step) => acc + step.documents.filter((doc) => doc.completed).length,
      0,
    )
    const percent = total === 0 ? 0 : Math.round((done / total) * 100)
    return { done, total, percent }
  }

  const toggleDocument = (projectId: string, stepId: string, documentId: string) => {
    setProjects((prev) =>
      prev.map((project) => {
        if (project.id !== projectId) return project
        return {
          ...project,
          steps: project.steps.map((step) => {
            if (step.id !== stepId) return step
            return {
              ...step,
              documents: step.documents.map((doc) =>
                doc.id === documentId ? { ...doc, completed: !doc.completed } : doc,
              ),
            }
          }),
        }
      }),
    )
  }

  const cycleStatus = (projectId: string) => {
    const order: ProjectStatus[] = ["aktiv", "pausiert", "beendet"]
    setProjects((prev) =>
      prev.map((project) => {
        if (project.id !== projectId) return project
        const nextIndex = (order.indexOf(project.status) + 1) % order.length
        return { ...project, status: order[nextIndex] }
      }),
    )
  }

  const deleteProject = (projectId: string) => {
    setProjects((prev) => prev.filter((p) => p.id !== projectId))
    if (activeProjectId === projectId) {
      setActiveProjectId(null)
      setMobileView("list")
    }
  }

  const handleProjectSelect = (projectId: string) => {
    setActiveProjectId(projectId)
    setMobileView("funnel")
  }

  // Project List Component
  const ProjectList = (
    <div className="flex flex-col h-full bg-background">
      {/* Header */}
      <div className="p-4">
        <h2 className="text-lg font-semibold text-foreground mb-3">{copy.title}</h2>
        
        {/* Status counts */}
        <div className="flex items-center gap-3 text-xs text-foreground mb-4">
          <span className="flex items-center gap-1">
            <PlayCircle className="w-3 h-3 text-primary" />
            {statusCounts.active} {copy.active}
          </span>
          <span className="flex items-center gap-1">
            <PauseCircle className="w-3 h-3 text-yellow-500" />
            {statusCounts.paused} {copy.paused}
          </span>
          <span className="flex items-center gap-1">
            <CheckCircle2 className="w-3 h-3 text-primary" />
            {statusCounts.done} {copy.finished}
          </span>
        </div>

        {/* Search */}
        <div className="relative">
          <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={copy.search}
            className="w-full pl-9 pr-3 py-2 bg-muted/30 border border-border text-foreground placeholder:text-muted-foreground text-sm outline-none focus:border-primary"
          />
        </div>
      </div>

      <div className="h-px bg-border mx-4" />

      {/* Project List */}
      <div className="flex-1 overflow-y-auto p-4 space-y-2">
        {/* Create button */}
        <button
          onClick={() => setShowDialog(true)}
          className="w-full flex items-center justify-between px-4 py-3 text-sm text-foreground hover:bg-primary/10 transition-colors"
        >
          <span>{copy.create}</span>
          <Plus className="w-4 h-4 text-muted-foreground" />
        </button>

        {filteredProjects.length === 0 ? (
          <div className="px-4 py-3 text-sm text-muted-foreground">
            {copy.emptyProjects}
          </div>
        ) : (
          filteredProjects.map((project) => {
            const isActive = project.id === activeProjectId
            const progress = getProgress(project)
            return (
              <div
                key={project.id}
                onClick={() => handleProjectSelect(project.id)}
                className={cn(
                  "w-full text-left p-4 transition cursor-pointer",
                  isActive 
                    ? "bg-primary text-primary-foreground" 
                    : "hover:bg-primary/10"
                )}
              >
                <div className="flex items-start justify-between gap-2 mb-2">
                  <span className={cn(
                    "text-sm font-medium",
                    isActive ? "text-primary-foreground" : "text-foreground"
                  )}>
                    {project.name}
                  </span>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation()
                      deleteProject(project.id)
                    }}
                    className={cn(
                      "p-1 transition-colors",
                      isActive ? "text-primary-foreground/70 hover:text-primary-foreground" : "text-muted-foreground hover:text-foreground"
                    )}
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
                
                <div className={cn(
                  "flex items-center gap-3 text-xs mb-2",
                  isActive ? "text-primary-foreground/70" : "text-muted-foreground"
                )}>
                  <span className="flex items-center gap-1">
                    <Users className="w-3.5 h-3.5" />
                    {project.customer || "-"}
                  </span>
                  <span className="flex items-center gap-1">
                    <Layers className="w-3.5 h-3.5" />
                    {project.type}
                  </span>
                </div>

                <div className={cn(
                  "flex items-center gap-2 text-xs mb-3",
                  isActive ? "text-primary-foreground/70" : "text-muted-foreground"
                )}>
                  <CalendarRange className="w-3.5 h-3.5" />
                  <span>{project.startDate || "?"} - {project.endDate || "?"}</span>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation()
                      cycleStatus(project.id)
                    }}
                    className={cn(
                      "px-2 py-0.5 text-xs uppercase tracking-wide",
                      isActive ? "bg-primary-foreground/20" : "bg-muted"
                    )}
                  >
                    {statusLabels[project.status]}
                  </button>
                </div>

                {/* Progress bar */}
                <div className={cn(
                  "border-t pt-2",
                  isActive ? "border-primary-foreground/30" : "border-border"
                )}>
                  <div className={cn(
                    "flex items-center justify-between text-xs mb-1",
                    isActive ? "text-primary-foreground/70" : "text-muted-foreground"
                  )}>
                    <span>{copy.progress}</span>
                    <span className={isActive ? "text-primary-foreground" : "text-foreground"}>
                      {progress.percent}% ({progress.done}/{progress.total})
                    </span>
                  </div>
                  <div className={cn(
                    "h-1.5 w-full",
                    isActive ? "bg-primary-foreground/30" : "bg-border"
                  )}>
                    <div
                      className={cn(
                        "h-full",
                        isActive ? "bg-primary-foreground" : "bg-primary"
                      )}
                      style={{ width: `${progress.percent}%` }}
                    />
                  </div>
                </div>
              </div>
            )
          })
        )}
      </div>
    </div>
  )

  // Document Funnel Component
  const DocumentFunnel = (
    <div className="flex flex-col h-full bg-background">
      {/* Header - with back button on mobile */}
      <div className="p-4 md:p-6 flex items-center gap-3">
        <button
          onClick={() => setMobileView("list")}
          className="md:hidden p-2 -ml-2 text-muted-foreground hover:text-foreground"
        >
          <ChevronLeft className="w-5 h-5" />
        </button>
        <h2 className="text-lg font-semibold text-foreground">{copy.funnel}</h2>
      </div>

      {activeProject ? (
        <div className="flex-1 overflow-y-auto px-4 md:px-6 pb-6 space-y-4">
          {/* Project name on mobile */}
          <div className="md:hidden text-sm text-muted-foreground mb-2">
            {activeProject.name}
          </div>
          
          {activeProject.steps.map((step) => (
            <div key={step.id}>
              <div className="text-sm font-medium text-foreground mb-2">{step.label}</div>
              <div className="grid grid-cols-2 md:grid-cols-3 gap-2">
                {step.documents.map((doc) => {
                  const isDone = doc.completed
                  return (
                    <button
                      key={doc.id}
                      onClick={() => {
                        if (!doc.completed) {
                          setActiveAppId("sessions")
                          return
                        }
                        toggleDocument(activeProject.id, step.id, doc.id)
                      }}
                      className={cn(
                        "h-10 border flex items-center px-3 text-sm transition",
                        isDone
                          ? "bg-primary text-primary-foreground border-primary"
                          : "bg-card text-foreground border-border hover:bg-primary/10"
                      )}
                    >
                      <span className="truncate">{doc.label}</span>
                    </button>
                  )
                })}
                <button
                  className="h-10 border border-border flex items-center justify-center text-muted-foreground hover:bg-primary/10 hover:border-primary/30 transition"
                >
                  <Plus className="w-4 h-4" />
                </button>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center px-6">
          <p className="text-sm text-muted-foreground">{copy.selectProject}</p>
        </div>
      )}
    </div>
  )

  return (
    <>
      {/* Mobile Layout - Show one view at a time */}
      <div className="md:hidden h-full w-full overflow-hidden">
        {mobileView === "list" ? ProjectList : DocumentFunnel}
      </div>

      {/* Desktop Layout - Side by side */}
      <div className="hidden md:flex h-full w-full overflow-hidden">
        {/* Column 1: Project List */}
        <div className="w-[320px] min-w-[320px] border-r border-border">
          {ProjectList}
        </div>

        {/* Column 2: Document Funnel */}
        <div className="flex-1">
          {DocumentFunnel}
        </div>
      </div>

      {/* Create Project Dialog */}
      {showDialog && (
        <div className="fixed inset-0 z-50 flex items-center justify-center px-4">
          <div className="absolute inset-0 bg-black/50" onClick={() => setShowDialog(false)} />
          <div className="relative w-full max-w-md border border-border p-6 space-y-4 shadow-lg bg-card">
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-semibold text-foreground">{copy.newProject}</h3>
              <button
                onClick={() => setShowDialog(false)}
                className="p-1 text-muted-foreground hover:text-foreground"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            <form className="space-y-4" onSubmit={handleCreateProject}>
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">{copy.projectLabel}</label>
                <input
                  required
                  value={form.name}
                  onChange={(e) => setForm((prev) => ({ ...prev, name: e.target.value }))}
                  className="w-full border border-border px-3 py-2 bg-transparent text-sm text-foreground outline-none focus:border-primary"
                />
              </div>
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">{copy.customerLabel}</label>
                <input
                  value={form.customer}
                  onChange={(e) => setForm((prev) => ({ ...prev, customer: e.target.value }))}
                  className="w-full border border-border px-3 py-2 bg-transparent text-sm text-foreground outline-none focus:border-primary"
                />
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground">{copy.startLabel}</label>
                  <input
                    type="date"
                    value={form.startDate}
                    onChange={(e) => setForm((prev) => ({ ...prev, startDate: e.target.value }))}
                    className="w-full border border-border px-3 py-2 bg-transparent text-sm text-foreground outline-none focus:border-primary"
                  />
                </div>
                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground">{copy.endLabel}</label>
                  <input
                    type="date"
                    value={form.endDate}
                    onChange={(e) => setForm((prev) => ({ ...prev, endDate: e.target.value }))}
                    className="w-full border border-border px-3 py-2 bg-transparent text-sm text-foreground outline-none focus:border-primary"
                  />
                </div>
                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground">{copy.typeLabel}</label>
                  <select
                    value={form.type}
                    onChange={(e) => setForm((prev) => ({ ...prev, type: e.target.value as ProjectType }))}
                    className="w-full border border-border px-3 py-2 bg-transparent text-sm text-foreground outline-none focus:border-primary"
                  >
                    <option value="Industrieprojekt">{projectTypes[0]}</option>
                    <option value="Forschungsprojekt">{projectTypes[1]}</option>
                  </select>
                </div>
                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground">{copy.status}</label>
                  <select
                    value={form.status}
                    onChange={(e) => setForm((prev) => ({ ...prev, status: e.target.value as ProjectStatus }))}
                    className="w-full border border-border px-3 py-2 bg-transparent text-sm text-foreground outline-none focus:border-primary"
                  >
                    <option value="aktiv">{statusLabels.aktiv}</option>
                    <option value="pausiert">{statusLabels.pausiert}</option>
                    <option value="beendet">{statusLabels.beendet}</option>
                  </select>
                </div>
              </div>

              <div className="flex items-center justify-end gap-2 pt-2">
                <button
                  type="button"
                  onClick={() => setShowDialog(false)}
                  className="h-10 px-4 border border-border text-sm text-muted-foreground hover:text-foreground"
                >
                  {copy.cancel}
                </button>
                <button
                  type="submit"
                  className="h-10 px-4 font-semibold text-sm bg-primary text-primary-foreground hover:bg-primary/90"
                >
                  {copy.save}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </>
  )
}

export default ProjectsApp
