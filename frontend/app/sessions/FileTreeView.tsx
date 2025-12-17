"use client"

import { useEffect, useState } from "react"
import { useApp } from "@/components/app-context"
import { fileserverProxyBaseUrl } from "@/lib/control-plane-client"
import { FileIcon } from "@/components/ui/file-icon"
import { List, LayoutGrid, ChevronRight, ChevronDown } from "lucide-react"
import { cn } from "@/lib/utils"

export type FileNode = {
  name: string
  path: string
  type: "file" | "directory"
  size?: number
  modified?: number
  children?: FileNode[]
}

async function fetchFileTree(baseUrl: string, path = "."): Promise<FileNode[]> {
  const url = new URL(`${baseUrl}/tree`, window.location.origin)
  url.searchParams.set("path", path)
  const res = await fetch(url.toString(), { cache: "no-store", credentials: "include" })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `File server error (${res.status})`)
  }
  return res.json()
}

// File extensions that can be previewed
const PREVIEWABLE_EXTENSIONS = new Set([
  ".txt", ".md", ".json", ".xml", ".yaml", ".yml", ".toml",
  ".js", ".ts", ".jsx", ".tsx", ".css", ".scss", ".html",
  ".py", ".rb", ".go", ".rs", ".java", ".c", ".cpp", ".h",
  ".sh", ".bash", ".zsh", ".fish", ".sql", ".graphql",
  ".env", ".gitignore", ".dockerignore", ".config", ".conf", ".ini", ".cfg", ".log",
])

function isPreviewable(filename: string): boolean {
  const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase()
  return PREVIEWABLE_EXTENSIONS.has(ext) || !filename.includes(".")
}

function formatFileSize(bytes?: number): string {
  if (bytes === undefined) return "-"
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function formatDate(timestamp?: number): string {
  if (!timestamp) return "-"
  const date = new Date(timestamp * 1000)
  return date.toLocaleDateString("de-DE", { day: "2-digit", month: "2-digit", year: "numeric" })
}

// Flatten tree for grid/list view (only files, no directories in flat view)
function flattenTree(nodes: FileNode[], includeDirectories = false): FileNode[] {
  const result: FileNode[] = []
  for (const node of nodes) {
    if (node.type === "file" || includeDirectories) {
      result.push(node)
    }
    if (node.children) {
      result.push(...flattenTree(node.children, includeDirectories))
    }
  }
  return result
}

interface FileTreeViewProps {
  onPreviewFile?: (filePath: string) => void
}

type ViewMode = "tree" | "list" | "grid"

export function FileTreeView({ onPreviewFile }: FileTreeViewProps) {
  const { selectedWorkspaceSessionId } = useApp()
  const [tree, setTree] = useState<FileNode[]>([])
  const [expanded, setExpanded] = useState<Record<string, boolean>>({})
  const [error, setError] = useState<string>("")
  const [loading, setLoading] = useState(false)
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [viewMode, setViewMode] = useState<ViewMode>("list")

  const fileserverBaseUrl = selectedWorkspaceSessionId 
    ? fileserverProxyBaseUrl(selectedWorkspaceSessionId) 
    : null

  useEffect(() => {
    if (!fileserverBaseUrl) return
    setLoading(true)
    setError("")
    fetchFileTree(fileserverBaseUrl, ".")
      .then((data) => setTree(data))
      .catch((err) => setError(err.message ?? "Unable to load file tree"))
      .finally(() => setLoading(false))
  }, [fileserverBaseUrl])

  const toggle = (path: string) => {
    setExpanded((prev) => ({ ...prev, [path]: !prev[path] }))
  }

  const handleSelectFile = (path: string, name: string) => {
    setSelectedFile(path)
    if (onPreviewFile && isPreviewable(name)) {
      onPreviewFile(path)
    }
  }

  if (!selectedWorkspaceSessionId) {
    return (
      <div className="h-full flex items-center justify-center p-4 text-sm text-muted-foreground">
        Select a workspace session to browse files.
      </div>
    )
  }

  if (loading) {
    return <div className="p-4 text-sm text-muted-foreground">Loading workspace tree...</div>
  }

  if (error) {
    return <div className="p-4 text-sm text-destructive">{error}</div>
  }

  const flatFiles = flattenTree(tree, true)

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* View mode toggle */}
      <div className="flex-shrink-0 flex items-center justify-end gap-1 p-2 border-b border-border">
        <button
          onClick={() => setViewMode("list")}
          className={cn(
            "p-1.5 rounded transition-colors",
            viewMode === "list" ? "bg-primary/20 text-primary" : "text-muted-foreground hover:text-foreground hover:bg-muted"
          )}
          title="List view"
        >
          <List className="w-4 h-4" />
        </button>
        <button
          onClick={() => setViewMode("grid")}
          className={cn(
            "p-1.5 rounded transition-colors",
            viewMode === "grid" ? "bg-primary/20 text-primary" : "text-muted-foreground hover:text-foreground hover:bg-muted"
          )}
          title="Grid view"
        >
          <LayoutGrid className="w-4 h-4" />
        </button>
      </div>

      {/* File content */}
      <div className="flex-1 overflow-auto">
        {tree.length === 0 ? (
          <div className="text-sm text-muted-foreground p-4">No files found.</div>
        ) : viewMode === "list" ? (
          <ListView 
            files={flatFiles} 
            selectedFile={selectedFile} 
            onSelectFile={handleSelectFile} 
          />
        ) : (
          <GridView 
            files={flatFiles} 
            selectedFile={selectedFile} 
            onSelectFile={handleSelectFile} 
          />
        )}
      </div>
    </div>
  )
}

// List View Component
function ListView({ 
  files, 
  selectedFile, 
  onSelectFile 
}: { 
  files: FileNode[]
  selectedFile: string | null
  onSelectFile: (path: string, name: string) => void 
}) {
  return (
    <div className="min-w-full">
      {/* Header */}
      <div className="sticky top-0 bg-card z-10 flex items-center gap-2 px-3 py-2 border-b border-border text-xs text-muted-foreground font-medium">
        <div className="flex-1 min-w-0">Name</div>
        <div className="w-24 text-right hidden sm:block">Modified</div>
        <div className="w-20 text-right hidden sm:block">Size</div>
        <div className="w-8">Type</div>
      </div>
      
      {/* Files */}
      <div className="divide-y divide-border/50">
        {files.map((file) => (
          <div
            key={file.path}
            onClick={() => file.type === "file" && onSelectFile(file.path, file.name)}
            className={cn(
              "flex items-center gap-2 px-3 py-2 transition-colors",
              file.type === "file" ? "cursor-pointer hover:bg-muted/50" : "opacity-60",
              selectedFile === file.path && "bg-primary/10"
            )}
          >
            <div className="flex-1 min-w-0 flex items-center gap-2">
              <FileIcon filename={file.name} isDirectory={file.type === "directory"} size={20} />
              <span className="truncate text-sm">{file.name}</span>
            </div>
            <div className="w-24 text-right text-xs text-muted-foreground hidden sm:block">
              {formatDate(file.modified)}
            </div>
            <div className="w-20 text-right text-xs text-muted-foreground hidden sm:block">
              {file.type === "file" ? formatFileSize(file.size) : "-"}
            </div>
            <div className="w-8 flex justify-center">
              <FileIcon filename={file.name} isDirectory={file.type === "directory"} size={18} />
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

// Grid View Component
function GridView({ 
  files, 
  selectedFile, 
  onSelectFile 
}: { 
  files: FileNode[]
  selectedFile: string | null
  onSelectFile: (path: string, name: string) => void 
}) {
  // Filter to only show files in grid view
  const fileNodes = files.filter(f => f.type === "file")
  
  return (
    <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2 p-2">
      {fileNodes.map((file) => (
        <div
          key={file.path}
          onClick={() => onSelectFile(file.path, file.name)}
          className={cn(
            "flex flex-col items-center gap-2 p-3 rounded-lg cursor-pointer transition-colors hover:bg-muted/50",
            selectedFile === file.path && "bg-primary/10 ring-1 ring-primary/30"
          )}
        >
          <FileIcon filename={file.name} size={48} />
          <span className="text-xs text-center truncate w-full" title={file.name}>
            {file.name}
          </span>
        </div>
      ))}
    </div>
  )
}

// Tree Row Component (keeping for potential tree view mode)
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
  onSelectFile: (path: string, name: string) => void
  selectedFile: string | null
}) {
  const isDir = node.type === "directory"
  const isExpanded = expanded[node.path]
  const isSelected = !isDir && selectedFile === node.path

  return (
    <li>
      <div
        className={cn(
          "flex items-center gap-1.5 py-1 px-2 rounded cursor-pointer transition-colors",
          isSelected 
            ? "bg-primary/10 text-primary" 
            : "hover:bg-muted text-muted-foreground hover:text-foreground"
        )}
        style={{ paddingLeft: `${level * 16 + 8}px` }}
        onClick={() => {
          if (isDir) {
            onToggle(node.path)
          } else {
            onSelectFile(node.path, node.name)
          }
        }}
      >
        {isDir && (
          <span className="flex-shrink-0">
            {isExpanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
          </span>
        )}
        {!isDir && <span className="w-4" />}
        <FileIcon filename={node.name} isDirectory={isDir} size={18} className="flex-shrink-0" />
        <span className="truncate text-sm">{node.name}</span>
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
