"use client"

import { useMemo } from "react"
import dynamic from "next/dynamic"
import { appConfig, hasOpencode } from "@/lib/config"

const GhosttyTerminal = dynamic(
  () => import("@/components/terminal/ghostty-terminal").then((mod) => mod.GhosttyTerminal),
  { ssr: false },
)

interface TerminalViewProps {
  sessionId?: string
}

export function TerminalView({ sessionId }: TerminalViewProps) {
  const wsUrl = useMemo(() => {
    if (!sessionId) return ""
    // Use backend WS proxy: ws://backend/sessions/{sessionId}/terminal
    const base = appConfig.opencodeBaseUrl
    if (!base) return ""
    const wsBase = base.replace(/^http/, 'ws')
    return `${wsBase}/sessions/${sessionId}/terminal`
  }, [sessionId])

  if (!hasOpencode) {
    return (
      <div className="h-full bg-black/70 rounded p-4 text-sm font-mono text-red-300">
        Configure <code className="font-bold">NEXT_PUBLIC_OPENCODE_BASE_URL</code> to connect to the control plane.
      </div>
    )
  }

  if (!sessionId) {
    return (
      <div className="h-full bg-black/70 rounded p-4 text-sm font-mono text-red-300">
        Select a session to attach to the terminal.
      </div>
    )
  }

  return (
    <div className="h-full">
      <GhosttyTerminal wsUrl={wsUrl} className="border border-border" />
    </div>
  )
}
