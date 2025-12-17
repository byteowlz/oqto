"use client"

import { useEffect, useState } from "react"
import { Eye, Loader2 } from "lucide-react"
import { useApp } from "@/components/app-context"
import { fileserverProxyBaseUrl } from "@/lib/control-plane-client"
import { cn } from "@/lib/utils"

interface PreviewViewProps {
  filePath?: string | null
  className?: string
}

async function fetchFileContent(baseUrl: string, path: string): Promise<string> {
  const url = new URL(`${baseUrl}/file`, window.location.origin)
  url.searchParams.set("path", path)
  const res = await fetch(url.toString(), { cache: "no-store", credentials: "include" })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `Unable to fetch ${path}`)
  }
  return res.text()
}

export function PreviewView({ filePath, className }: PreviewViewProps) {
  const { selectedWorkspaceSessionId } = useApp()
  const [content, setContent] = useState<string>("")
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string>("")

  const fileserverBaseUrl = selectedWorkspaceSessionId 
    ? fileserverProxyBaseUrl(selectedWorkspaceSessionId) 
    : null

  useEffect(() => {
    if (!filePath || !fileserverBaseUrl) {
      setContent("")
      return
    }

    setLoading(true)
    setError("")
    fetchFileContent(fileserverBaseUrl, filePath)
      .then((data) => {
        setContent(data)
        setLoading(false)
      })
      .catch((err) => {
        setError(err.message ?? "Failed to load file")
        setLoading(false)
      })
  }, [filePath, fileserverBaseUrl])

  // No file selected
  if (!filePath) {
    return (
      <div className={cn("h-full bg-muted/30 rounded flex items-center justify-center", className)}>
        <div className="text-center text-muted-foreground">
          <Eye className="w-12 h-12 mx-auto mb-2 opacity-50" />
          <p className="text-sm">No preview available</p>
          <p className="text-xs mt-1">Select a file to preview</p>
        </div>
      </div>
    )
  }

  // Loading state
  if (loading) {
    return (
      <div className={cn("h-full bg-muted/30 rounded flex items-center justify-center", className)}>
        <div className="text-center text-muted-foreground">
          <Loader2 className="w-8 h-8 mx-auto mb-2 animate-spin" />
          <p className="text-sm">Loading...</p>
        </div>
      </div>
    )
  }

  // Error state
  if (error) {
    return (
      <div className={cn("h-full bg-muted/30 rounded flex items-center justify-center", className)}>
        <div className="text-center text-destructive">
          <p className="text-sm">{error}</p>
        </div>
      </div>
    )
  }

  // Get filename from path
  const filename = filePath.split("/").pop() || filePath

  return (
    <div className={cn("h-full flex flex-col overflow-hidden", className)}>
      <div className="flex-shrink-0 px-3 py-2 border-b border-border bg-muted/30">
        <p className="text-xs font-mono text-muted-foreground truncate" title={filePath}>
          {filename}
        </p>
      </div>
      <div className="flex-1 overflow-auto p-3">
        <pre className="text-xs font-mono whitespace-pre-wrap break-words">
          {content}
        </pre>
      </div>
    </div>
  )
}
