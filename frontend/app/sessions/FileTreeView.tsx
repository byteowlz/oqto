"use client"

import { useEffect, useState } from "react"
import { hasFileServer } from "@/lib/config"
import { fetchFileTree, type FileNode, fetchFileContent } from "@/lib/file-service"

export function FileTreeView() {
  const [tree, setTree] = useState<FileNode[]>([])
  const [expanded, setExpanded] = useState<Record<string, boolean>>({})
  const [error, setError] = useState<string>("")
  const [loading, setLoading] = useState(false)
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [filePreview, setFilePreview] = useState<string>("")
  const [previewState, setPreviewState] = useState<"idle" | "loading">("idle")

  useEffect(() => {
    if (!hasFileServer) return
    setLoading(true)
    fetchFileTree(".")
      .then((data) => setTree(data))
      .catch((err) => setError(err.message ?? "Unable to load file tree"))
      .finally(() => setLoading(false))
  }, [])

  useEffect(() => {
    if (!selectedFile) return
    setPreviewState("loading")
    fetchFileContent(selectedFile)
      .then((content) => {
        setFilePreview(content)
        setPreviewState("idle")
      })
      .catch((err) => {
        setFilePreview(err.message ?? "Failed to read file")
        setPreviewState("idle")
      })
  }, [selectedFile])

  const toggle = (path: string) => {
    setExpanded((prev) => ({ ...prev, [path]: !prev[path] }))
  }

  if (!hasFileServer) {
    return (
      <div className="h-full border border-dashed border-border rounded p-4 text-sm text-muted-foreground">
        Configure <code className="font-bold">NEXT_PUBLIC_FILE_SERVER_URL</code> to browse workspace files.
      </div>
    )
  }

  if (loading) {
    return <div className="p-4 text-sm text-muted-foreground">Loading workspace tree…</div>
  }

  if (error) {
    return <div className="p-4 text-sm text-destructive">{error}</div>
  }

  return (
    <div className="h-full flex gap-4">
      <div className="flex-1 overflow-y-auto border border-border rounded p-4">
        {tree.length === 0 ? (
          <div className="text-sm text-muted-foreground">No files returned from server.</div>
        ) : (
          <ul className="space-y-1 text-sm font-mono">
            {tree.map((node) => (
              <TreeRow
                key={node.path}
                node={node}
                level={0}
                expanded={expanded}
                onToggle={toggle}
                onSelectFile={setSelectedFile}
                selectedFile={selectedFile}
              />
            ))}
          </ul>
        )}
      </div>
      <div className="w-1/2 border border-border rounded p-4 bg-background/80 overflow-y-auto">
        <div className="text-xs text-muted-foreground mb-2 uppercase tracking-wider">File Preview</div>
        {selectedFile ? (
          <pre className="text-xs whitespace-pre-wrap font-mono">
            {previewState === "loading" ? "Loading…" : filePreview}
          </pre>
        ) : (
          <div className="text-sm text-muted-foreground">Select a file to preview.</div>
        )}
      </div>
    </div>
  )
}

function TreeRow({
  node,
  level,
  expanded,
  onToggle,
  onSelectFile,
  selectedFile,
}: {
  node: FileNode
  level: number
  expanded: Record<string, boolean>
  onToggle: (path: string) => void
  onSelectFile: (path: string) => void
  selectedFile: string | null
}) {
  const isDir = node.type === "directory"
  const isExpanded = expanded[node.path]
  const padding = `${level * 12}px`

  return (
    <li>
      <div
        className={`flex items-center gap-2 cursor-pointer hover:text-primary ${
          !isDir && selectedFile === node.path ? "text-primary" : "text-muted-foreground"
        }`}
        style={{ paddingLeft: padding }}
        onClick={() => {
          if (isDir) {
            onToggle(node.path)
          } else {
            onSelectFile(node.path)
          }
        }}
      >
        <span className="text-xs uppercase tracking-widest">
          {isDir ? (isExpanded ? "[OPEN]" : "[DIR]") : "[FILE]"}
        </span>
        <span>{node.name}</span>
      </div>
      {isDir && isExpanded && node.children && node.children.length > 0 && (
        <ul>
          {node.children.map((child) => (
            <TreeRow
              key={child.path}
              node={child}
              level={level + 1}
              expanded={expanded}
              onToggle={onToggle}
              onSelectFile={onSelectFile}
              selectedFile={selectedFile}
            />
          ))}
        </ul>
      )}
    </li>
  )
}
