"use client"

import { useState, useEffect, useMemo } from "react"
import { User, Search, X } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"
import { listPersonas, type Persona } from "@/lib/control-plane-client"
import { resolveAvatarUrl, getDefaultAvatarUrl } from "@/lib/avatar-utils"

interface AgentPickerProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSelect: (persona: Persona) => void
  locale: "de" | "en"
}

export function AgentPicker({ open, onOpenChange, onSelect, locale }: AgentPickerProps) {
  const [personas, setPersonas] = useState<Persona[]>([])
  const [loading, setLoading] = useState(true)
  const [searchTerm, setSearchTerm] = useState("")

  const t = useMemo(() => ({
    de: {
      title: "Agent auswahlen",
      searchPlaceholder: "Agenten suchen...",
      noAgents: "Keine Agenten gefunden",
      loading: "Laden...",
      defaultAgent: "Standard",
    },
    en: {
      title: "Select Agent",
      searchPlaceholder: "Search agents...",
      noAgents: "No agents found",
      loading: "Loading...",
      defaultAgent: "Default",
    },
  }), [])[locale]

  useEffect(() => {
    if (!open) return
    
    const fetchPersonas = async () => {
      setLoading(true)
      try {
        const data = await listPersonas()
        setPersonas(data)
      } catch (err) {
        console.error("Failed to fetch personas:", err)
      } finally {
        setLoading(false)
      }
    }
    
    fetchPersonas()
  }, [open])

  const filteredPersonas = useMemo(() => {
    // Only show standalone personas in the "New Chat" picker
    // Non-standalone personas (Builder, Planner) are shown in Projects view
    const standaloneOnly = personas.filter((p) => p.standalone)
    
    if (!searchTerm) return standaloneOnly
    const lower = searchTerm.toLowerCase()
    return standaloneOnly.filter(
      (p) =>
        p.name.toLowerCase().includes(lower) ||
        p.description.toLowerCase().includes(lower)
    )
  }, [personas, searchTerm])

  const handleSelect = (persona: Persona) => {
    onSelect(persona)
    onOpenChange(false)
    setSearchTerm("")
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{t.title}</DialogTitle>
        </DialogHeader>
        
        {/* Search */}
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <Input
            placeholder={t.searchPlaceholder}
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="pl-9 pr-9"
          />
          {searchTerm && (
            <button
              onClick={() => setSearchTerm("")}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>

        {/* Agent list */}
        <div className="max-h-[50vh] overflow-y-auto -mx-6 px-6 overscroll-contain touch-pan-y">
          {loading ? (
            <div className="text-center text-muted-foreground py-8">
              {t.loading}
            </div>
          ) : filteredPersonas.length === 0 ? (
            <div className="text-center text-muted-foreground py-8">
              {t.noAgents}
            </div>
          ) : (
            <div className="space-y-1">
              {filteredPersonas.map((persona) => {
                const avatarUrl = resolveAvatarUrl(persona.avatar) || getDefaultAvatarUrl(persona.id)
                return (
                <button
                  key={persona.id}
                  onClick={() => handleSelect(persona)}
                  className={cn(
                    "w-full text-left p-3 rounded-lg transition-colors flex items-start gap-3",
                    "hover:bg-accent"
                  )}
                >
                  {/* Avatar */}
                  <div
                    className="w-10 h-10 rounded-full flex items-center justify-center flex-shrink-0 overflow-hidden"
                    style={{ backgroundColor: persona.color || "#6366f1" }}
                  >
                    {avatarUrl ? (
                      <img
                        src={avatarUrl}
                        alt={persona.name}
                        className="w-full h-full object-cover"
                        onError={(e) => {
                          // Hide broken image, show fallback color
                          e.currentTarget.style.display = "none"
                        }}
                      />
                    ) : (
                      <User className="w-5 h-5 text-white" />
                    )}
                  </div>

                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-medium text-sm">{persona.name}</span>
                      {persona.is_default && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-primary/10 text-primary">
                          {t.defaultAgent}
                        </span>
                      )}
                    </div>
                    {persona.description && (
                      <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">
                        {persona.description}
                      </p>
                    )}
                  </div>
                </button>
              )})}
              
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
