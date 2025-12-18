"use client"

import { useMemo } from "react"
import dynamic from "next/dynamic"
import { useTheme } from "next-themes"
import { controlPlaneDirectBaseUrl, terminalProxyPath } from "@/lib/control-plane-client"
import { toAbsoluteWsUrl } from "@/lib/url"
import { useApp } from "@/components/app-context"

const GhosttyTerminal = dynamic(
  () => import("@/components/terminal/ghostty-terminal").then((mod) => mod.GhosttyTerminal),
  { ssr: false },
)

interface TerminalViewProps {
  sessionId?: string
}

export function TerminalView({ sessionId }: TerminalViewProps) {
  const { authToken, selectedWorkspaceSession } = useApp()
  const { resolvedTheme } = useTheme()
  
  const wsUrl = useMemo(() => {
    if (!sessionId) return ""
    const directBase = controlPlaneDirectBaseUrl()
    if (directBase) {
      return toAbsoluteWsUrl(`${directBase}${terminalProxyPath(sessionId)}`)
    }
    return toAbsoluteWsUrl(`/api${terminalProxyPath(sessionId)}`)
  }, [sessionId])

  // Don't render terminal if no session selected
  if (!sessionId) {
    return (
      <div className="h-full bg-black/70 rounded p-4 text-sm font-mono text-red-300">
        Select a session to attach to the terminal.
      </div>
    )
  }

  // Don't render terminal if session is not running (failed, stopped, pending, etc.)
  if (!selectedWorkspaceSession || selectedWorkspaceSession.status !== "running") {
    const statusMsg = selectedWorkspaceSession 
      ? `Session is ${selectedWorkspaceSession.status}...`
      : "Loading session..."
    return (
      <div className="h-full bg-black/70 rounded p-4 text-sm font-mono text-yellow-300">
        {statusMsg}
      </div>
    )
  }

  // Pass theme to terminal so it can include it in its session key
  return (
    <div className="h-full">
      <GhosttyTerminal 
        key={`${sessionId}-${resolvedTheme}`}
        wsUrl={wsUrl} 
        authToken={authToken ?? undefined} 
        className="border border-border"
        theme={resolvedTheme}
      />
    </div>
  )
}
