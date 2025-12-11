"use client"

import { useMemo } from "react"
import dynamic from "next/dynamic"
import { appConfig, hasTerminal } from "@/lib/config"

const GhosttyTerminal = dynamic(
  () => import("@/components/terminal/ghostty-terminal").then((mod) => mod.GhosttyTerminal),
  { ssr: false },
)

export function TerminalView() {
  const wsUrl = useMemo(() => appConfig.terminalWsUrl, [])

  if (!hasTerminal) {
    return (
      <div className="h-full bg-black/70 rounded p-4 text-sm font-mono text-red-300">
        Configure <code className="font-bold">NEXT_PUBLIC_TERMINAL_WS_URL</code> to attach to a remote PTY.
      </div>
    )
  }

  return (
    <div className="h-full">
      <GhosttyTerminal wsUrl={wsUrl} className="border border-border" />
    </div>
  )
}
