"use client"

import { useMemo } from "react"
import dynamic from "next/dynamic"
import { controlPlaneDirectBaseUrl, terminalProxyPath } from "@/lib/control-plane-client"
import { toAbsoluteWsUrl } from "@/lib/url"

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
    const directBase = controlPlaneDirectBaseUrl()
    if (directBase) {
      return toAbsoluteWsUrl(`${directBase}${terminalProxyPath(sessionId)}`)
    }
    return toAbsoluteWsUrl(`/api${terminalProxyPath(sessionId)}`)
  }, [sessionId])

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
