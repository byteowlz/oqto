"use client"

import { useMemo } from "react"
import { Plus, Search } from "lucide-react"
import { useApp } from "@/components/app-context"

const listBlocks = Array.from({ length: 5 })
const funnelStages = [
  { id: "akquise", label: "Akquise" },
  { id: "angebot", label: "Angebot" },
  { id: "kickoff", label: "Kickoff" },
  { id: "meilenstein-1", label: "Meilenstein" },
  { id: "zwischen", label: "Zwischenergebnisse" },
  { id: "meilenstein-2", label: "Meilenstein" },
  { id: "abschluss", label: "Abschluss" },
  { id: "verwertung", label: "Verwertung" },
]

export function ProjectsApp() {
  const { locale } = useApp()
  const copy = useMemo(
    () => ({
      de: { search: "Projekte durchsuchen", create: "Projekt anlegen", funnel: "Dokumenten-Funnel" },
      en: { search: "Search projects", create: "Create project", funnel: "Document Funnel" },
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
      <div className="w-full px-4 md:px-8 py-6">
        <div className="max-w-6xl mx-auto flex flex-col gap-6">
          <div className="flex flex-row gap-6 items-stretch">
            {/* Projektliste */}
            <section
              className="lg:flex-[2] lg:min-w-[300px] flex flex-col gap-4 rounded-[14px] p-4 md:p-5 shadow-[0_18px_40px_rgba(0,0,0,0.55)]"
              style={{ backgroundColor: colors.surface, border: `1px solid ${colors.border}` }}
            >
              <header className="flex flex-col gap-3">
                <div className="flex items-center justify-between gap-3">
                  <h2 className="text-sm font-semibold tracking-wide text-[#dbe1dd]">Projekte</h2>
                </div>
                <div className="flex flex-wrap items-center gap-3">
                  <div className="flex-1 min-w-[220px]">
                  <div
                    className="flex items-center gap-3 rounded-[10px] px-4 h-11 border"
                    style={{ backgroundColor: colors.input, borderColor: colors.border }}
                  >
                    <Search className="w-4 h-4 text-[#b2b9b5]" />
                      <input
                        placeholder={copy.search}
                    className="bg-transparent border-none outline-none text-[color:var(--foreground)] placeholder:text-[#b2b9b5] text-sm flex-1"
                      />
                    </div>
                  </div>
                  <button
                  className="h-11 px-4 rounded-[10px] font-semibold flex items-center gap-2 transition-transform duration-150 hover:-translate-y-[1px] focus-visible:outline-none shadow-none"
                  style={{ backgroundColor: colors.accent, color: colors.bgMain }}
                >
                    <Plus className="w-4 h-4" />
                    {copy.create}
                  </button>
                </div>
              </header>

              <div className="pt-2 border-t border-[#3b403d]/60 mt-2">
                <div className="flex flex-col gap-3 mt-3">
                  <div
                    className="h-14 rounded-[10px] border flex items-center px-4 text-sm"
                    style={{ backgroundColor: colors.card, borderColor: colors.accent }}
                  >
                    <div className="flex flex-col gap-0.5">
                      <span className="font-medium" style={{ color: colors.text }}>
                        Ausgewähltes Projekt
                      </span>
                      <span className="text-xs text-[#a3aaa7]">Kurzinfo / Kunde / Status</span>
                    </div>
                  </div>

                  {listBlocks.slice(0, 4).map((_, idx) => (
                    <div
                      key={idx}
                      className="h-12 rounded-[10px] border flex items-center px-4"
                      style={{ backgroundColor: colors.card, borderColor: colors.border }}
                    >
                      <div className="w-full h-[6px] rounded-full bg-[#262a28]" />
                    </div>
                  ))}
                </div>
              </div>
            </section>

            {/* Dokumenten-Funnel */}
            <section
              className="lg:flex-[3] lg:min-w-[360px] flex flex-col gap-4 rounded-[14px] p-4 md:p-5 shadow-[0_18px_40px_rgba(0,0,0,0.55)]"
              style={{ backgroundColor: colors.surface, border: `1px solid ${colors.border}` }}
            >
              <header className="flex items-center justify-between">
                <h2 className="text-sm font-semibold tracking-wide text-[#dbe1dd]">{copy.funnel}</h2>
              </header>

              <div className="flex flex-col gap-4">
                {funnelStages.map((stage) => (
                  <div key={stage.id} className="flex flex-col gap-2">
                    <span className="text-xs text-[#a3aaa7]">{stage.label}</span>
                    <div className="grid grid-cols-[repeat(3,minmax(0,1fr))_auto] gap-2">
                      <div
                        className="h-8 rounded-[10px] border"
                        style={{ backgroundColor: colors.input, borderColor: colors.border }}
                      />
                      <div
                        className="h-8 rounded-[10px] border"
                        style={{ backgroundColor: colors.input, borderColor: colors.border }}
                      />
                      <div
                        className="h-8 rounded-[10px] border"
                        style={{ backgroundColor: colors.input, borderColor: colors.border }}
                      />
                      <button
                        className="h-8 w-8 rounded-[10px] border flex items-center justify-center text-[#a3aaa7] transition-transform duration-150 hover:-translate-y-[1px]"
                        style={{ backgroundColor: colors.input, borderColor: colors.border }}
                      >
                        <Plus className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </section>
          </div>
        </div>
      </div>
    </div>
  )
}

export default ProjectsApp
