"use client"

import { useMemo, useState } from "react"
import { Plus, Search, FileText, ChevronDown, Upload, Calendar, Users, CheckCircle2 } from "lucide-react"
import { useApp } from "@/components/app-context"

const listBlocks = Array.from({ length: 5 })
const funnelStages = [
  { id: "akquise", label: "Akquise", icon: Users, docs: 3, status: "complete" },
  { id: "angebot", label: "Angebot", icon: FileText, docs: 2, status: "complete" },
  { id: "kickoff", label: "Kickoff", icon: Calendar, docs: 1, status: "active" },
  { id: "meilenstein-1", label: "Meilenstein 1", icon: CheckCircle2, docs: 0, status: "pending" },
  { id: "zwischen", label: "Zwischenergebnisse", icon: FileText, docs: 0, status: "pending" },
  { id: "meilenstein-2", label: "Meilenstein 2", icon: CheckCircle2, docs: 0, status: "pending" },
  { id: "abschluss", label: "Abschluss", icon: FileText, docs: 0, status: "pending" },
  { id: "verwertung", label: "Verwertung", icon: FileText, docs: 0, status: "pending" },
]

function FunnelStageCard({ stage, isOpen, onToggle, colors }: { 
  stage: typeof funnelStages[0], 
  isOpen: boolean, 
  onToggle: () => void,
  colors: Record<string, string>
}) {
  const Icon = stage.icon
  const statusColors = {
    complete: { bg: "rgba(59, 167, 124, 0.15)", border: "#3ba77c", text: "#3ba77c" },
    active: { bg: "rgba(59, 167, 124, 0.08)", border: "#3ba77c", text: "#dbe1dd" },
    pending: { bg: "transparent", border: colors.border, text: "#a3aaa7" },
  }
  const status = statusColors[stage.status as keyof typeof statusColors]

  return (
    <div
      className="border transition-all duration-200"
      style={{ 
        backgroundColor: status.bg, 
        borderColor: isOpen ? "#3ba77c" : status.border,
      }}
    >
      <button
        onClick={onToggle}
        className="w-full flex items-center justify-between p-4 text-left transition-colors hover:bg-white/5"
      >
        <div className="flex items-center gap-3">
          <div 
            className="w-8 h-8 flex items-center justify-center border"
            style={{ 
              backgroundColor: stage.status === "complete" ? "rgba(59, 167, 124, 0.2)" : colors.input,
              borderColor: stage.status === "complete" ? "#3ba77c" : colors.border
            }}
          >
            <Icon className="w-4 h-4" style={{ color: stage.status === "complete" ? "#3ba77c" : "#a3aaa7" }} />
          </div>
          <div className="flex flex-col">
            <span className="text-sm font-medium" style={{ color: status.text }}>{stage.label}</span>
            <span className="text-xs" style={{ color: "#6b7280" }}>
              {stage.docs} {stage.docs === 1 ? "Dokument" : "Dokumente"}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-3">
          {stage.status === "complete" && (
            <span className="text-xs px-2 py-1 bg-[#3ba77c]/20 text-[#3ba77c] font-medium">
              Abgeschlossen
            </span>
          )}
          {stage.status === "active" && (
            <span className="text-xs px-2 py-1 bg-[#3ba77c]/10 text-[#3ba77c] font-medium">
              Aktiv
            </span>
          )}
          <ChevronDown 
            className={`w-4 h-4 transition-transform duration-200 ${isOpen ? "rotate-180" : ""}`}
            style={{ color: "#a3aaa7" }}
          />
        </div>
      </button>
      
      <div 
        className={`overflow-hidden transition-all duration-200 ${isOpen ? "max-h-96" : "max-h-0"}`}
      >
        <div className="px-4 pb-4 pt-0">
          <div className="border-t pt-4" style={{ borderColor: colors.border }}>
            {stage.docs > 0 ? (
              <div className="flex flex-col gap-2">
                {Array.from({ length: stage.docs }).map((_, i) => (
                  <div 
                    key={i}
                    className="flex items-center gap-3 p-3 border transition-colors hover:bg-white/5 cursor-pointer"
                    style={{ backgroundColor: colors.input, borderColor: colors.border }}
                  >
                    <FileText className="w-4 h-4 text-[#a3aaa7]" />
                    <div className="flex-1">
                      <div className="text-sm text-[#dbe1dd]">Dokument {i + 1}.pdf</div>
                      <div className="text-xs text-[#6b7280]">Hochgeladen am 12.12.2025</div>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="flex flex-col items-center justify-center py-6 text-center">
                <Upload className="w-8 h-8 text-[#6b7280] mb-2" />
                <span className="text-sm text-[#a3aaa7]">Keine Dokumente vorhanden</span>
                <span className="text-xs text-[#6b7280]">Klicken Sie auf + um Dokumente hinzuzufugen</span>
              </div>
            )}
            <button
              className="w-full mt-3 h-10 border border-dashed flex items-center justify-center gap-2 text-sm transition-colors hover:bg-white/5 hover:border-[#3ba77c]"
              style={{ borderColor: colors.border, color: "#a3aaa7" }}
            >
              <Plus className="w-4 h-4" />
              Dokument hinzufugen
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

export function ProjectsApp() {
  const { locale } = useApp()
  const [openStages, setOpenStages] = useState<string[]>(["kickoff"])
  
  const toggleStage = (id: string) => {
    setOpenStages(prev => 
      prev.includes(id) ? prev.filter(s => s !== id) : [...prev, id]
    )
  }
  
  const copy = useMemo(
    () => ({
      de: { search: "Projekte durchsuchen", create: "Projekt anlegen", funnel: "Projektdateien" },
      en: { search: "Search projects", create: "Create project", funnel: "Project Files" },
    }),
    [],
  )[locale]

  const colors = {
    bgMain: "var(--background)",
    surface: "var(--card)",
    card: "var(--muted)",
    input: "var(--input)",
    border: "var(--border)",
    accent: "var(--primary)",
    text: "var(--foreground)",
  }

  return (
    <div className="h-full w-full overflow-auto" style={{ backgroundColor: colors.bgMain, color: colors.text }}>
      <div className="w-full h-full px-4 md:px-6 py-4 md:py-6 pb-16">
        <div className="h-full flex flex-col lg:flex-row gap-4 md:gap-6">
          {/* Projektliste */}
          <section
            className="flex-1 lg:flex-[2] lg:min-w-[300px] flex flex-col gap-4 p-4 md:p-5 shadow-[0_18px_40px_rgba(0,0,0,0.55)] min-h-[300px] lg:min-h-0"
            style={{ backgroundColor: colors.surface, border: `1px solid ${colors.border}` }}
          >
            <header className="flex flex-col gap-3">
              <div className="flex items-center justify-between gap-3">
                <h2 className="text-sm font-semibold tracking-wide text-[#dbe1dd]">Projekte</h2>
              </div>
              <div className="flex flex-col gap-3">
                <div
                  className="flex items-center gap-3 px-4 h-11 border"
                  style={{ backgroundColor: colors.input, borderColor: colors.border }}
                >
                  <Search className="w-4 h-4 text-[#b2b9b5]" />
                  <input
                    placeholder={copy.search}
                    className="bg-transparent border-none outline-none text-[color:var(--foreground)] placeholder:text-[#b2b9b5] text-sm flex-1"
                  />
                </div>
                <button
                  className="h-11 px-4 font-semibold flex items-center justify-center gap-2 transition-transform duration-150 hover:-translate-y-[1px] focus-visible:outline-none shadow-none shrink-0"
                  style={{ backgroundColor: colors.accent, color: colors.bgMain }}
                >
                  <Plus className="w-4 h-4" />
                  {copy.create}
                </button>
              </div>
            </header>

            <div className="flex-1 pt-2 border-t border-[#3b403d]/60 mt-2 overflow-auto">
              <div className="flex flex-col gap-3 mt-3">
                <div
                  className="h-14 border flex items-center px-4 text-sm"
                  style={{ backgroundColor: colors.card, borderColor: colors.accent }}
                >
                  <div className="flex flex-col gap-0.5">
                    <span className="font-medium" style={{ color: colors.text }}>
                      Ausgewahltes Projekt
                    </span>
                    <span className="text-xs text-[#a3aaa7]">Kurzinfo / Kunde / Status</span>
                  </div>
                </div>

                {listBlocks.slice(0, 4).map((_, idx) => (
                  <div
                    key={idx}
                    className="h-12 border flex items-center px-4"
                    style={{ backgroundColor: colors.card, borderColor: colors.border }}
                  >
                    <div className="w-full h-[6px] bg-[#262a28]" />
                  </div>
                ))}
              </div>
            </div>
          </section>

          {/* Dokumenten-Funnel */}
          <section
            className="flex-1 lg:flex-[3] lg:min-w-[360px] flex flex-col gap-4 p-4 md:p-5 shadow-[0_18px_40px_rgba(0,0,0,0.55)] min-h-[400px] lg:min-h-0"
            style={{ backgroundColor: colors.surface, border: `1px solid ${colors.border}` }}
          >
            <header className="flex items-center justify-between">
              <h2 className="text-sm font-semibold tracking-wide text-[#dbe1dd]">{copy.funnel}</h2>
              <div className="flex items-center gap-2">
                <span className="text-xs text-[#6b7280]">
                  {funnelStages.filter(s => s.status === "complete").length}/{funnelStages.length} abgeschlossen
                </span>
              </div>
            </header>

            {/* Progress bar */}
            <div className="h-1 w-full bg-[#262a28] overflow-hidden">
              <div 
                className="h-full bg-[#3ba77c] transition-all duration-500"
                style={{ width: `${(funnelStages.filter(s => s.status === "complete").length / funnelStages.length) * 100}%` }}
              />
            </div>

            <div className="flex-1 flex flex-col gap-2 overflow-auto">
              {funnelStages.map((stage) => (
                <FunnelStageCard 
                  key={stage.id}
                  stage={stage}
                  isOpen={openStages.includes(stage.id)}
                  onToggle={() => toggleStage(stage.id)}
                  colors={colors}
                />
              ))}
            </div>
          </section>
        </div>
      </div>
    </div>
  )
}

export default ProjectsApp
