"use client"

import { useEffect, useState, useCallback, useRef } from "react"
import { useApp } from "@/components/app-context"
import { fileserverProxyBaseUrl } from "@/lib/control-plane-client"
import { FileIcon } from "@/components/ui/file-icon"
import { 
  List, LayoutGrid, ChevronRight, ChevronDown, FolderUp, Home, Folder, 
  Upload, Download, Trash2, FolderPlus, Loader2
} from "lucide-react"
import { cn } from "@/lib/utils"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"

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

async function uploadFile(baseUrl: string, destPath: string, file: File): Promise<void> {
  const url = new URL(`${baseUrl}/file`, window.location.origin)
  url.searchParams.set("path", destPath)
  url.searchParams.set("mkdir", "true")
  
  const formData = new FormData()
  formData.append("file", file)
  
  const res = await fetch(url.toString(), {
    method: "POST",
    credentials: "include",
    body: formData,
  })
  
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `Upload failed (${res.status})`)
  }
}

async function deleteFile(baseUrl: string, path: string): Promise<void> {
  const url = new URL(`${baseUrl}/file`, window.location.origin)
  url.searchParams.set("path", path)
  
  const res = await fetch(url.toString(), {
    method: "DELETE",
    credentials: "include",
  })
  
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `Delete failed (${res.status})`)
  }
}

async function createDirectory(baseUrl: string, path: string): Promise<void> {
  const url = new URL(`${baseUrl}/mkdir`, window.location.origin)
  url.searchParams.set("path", path)
  
  const res = await fetch(url.toString(), {
    method: "PUT",
    credentials: "include",
  })
  
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `Create directory failed (${res.status})`)
  }
}

function getDownloadUrl(baseUrl: string, path: string): string {
  const url = new URL(`${baseUrl}/download`, window.location.origin)
  url.searchParams.set("path", path)
  return url.toString()
}

function getDownloadZipUrl(baseUrl: string, paths: string[], name?: string): string {
  const url = new URL(`${baseUrl}/download-zip`, window.location.origin)
  url.searchParams.set("paths", paths.join(","))
  if (name) {
    url.searchParams.set("name", name)
  }
  return url.toString()
}

// File extensions that can be previewed
const PREVIEWABLE_EXTENSIONS = new Set([
  ".txt", ".md", ".json", ".xml", ".yaml", ".yml", ".toml",
  ".js", ".ts", ".jsx", ".tsx", ".css", ".scss", ".html",
  ".py", ".rb", ".go", ".rs", ".java", ".c", ".cpp", ".h",
  ".sh", ".bash", ".zsh", ".fish", ".sql", ".graphql",
  ".env", ".gitignore", ".dockerignore", ".config", ".conf", ".ini", ".cfg", ".log",
  // Images
  ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".ico",
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

export type ViewMode = "tree" | "list" | "grid"

export interface FileTreeState {
  currentPath: string
  expanded: Record<string, boolean>
  selectedFile: string | null
  selectedFiles: Set<string>
  viewMode: ViewMode
}

export const initialFileTreeState: FileTreeState = {
  currentPath: ".",
  expanded: {},
  selectedFile: null,
  selectedFiles: new Set(),
  viewMode: "tree",
}

interface FileTreeViewProps {
  onPreviewFile?: (filePath: string) => void
  /** External state for persistence across view switches */
  state?: FileTreeState
  /** Callback to update external state */
  onStateChange?: (state: FileTreeState) => void
}

export function FileTreeView({ onPreviewFile, state, onStateChange }: FileTreeViewProps) {
  const { selectedWorkspaceSessionId } = useApp()
  const [tree, setTree] = useState<FileNode[]>([])
  const [error, setError] = useState<string>("")
  const [loading, setLoading] = useState(false)
  const [uploading, setUploading] = useState(false)
  const [newFolderName, setNewFolderName] = useState<string | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const newFolderInputRef = useRef<HTMLInputElement>(null)
  
  // Use external state if provided, otherwise use internal state
  const [internalExpanded, setInternalExpanded] = useState<Record<string, boolean>>({})
  const [internalSelectedFile, setInternalSelectedFile] = useState<string | null>(null)
  const [internalSelectedFiles, setInternalSelectedFiles] = useState<Set<string>>(new Set())
  const [internalViewMode, setInternalViewMode] = useState<ViewMode>("tree")
  const [internalCurrentPath, setInternalCurrentPath] = useState<string>(".")
  
  const expanded = state?.expanded ?? internalExpanded
  const selectedFile = state?.selectedFile ?? internalSelectedFile
  const selectedFiles = state?.selectedFiles ?? internalSelectedFiles
  const viewMode = state?.viewMode ?? internalViewMode
  const currentPath = state?.currentPath ?? internalCurrentPath
  
  const updateState = useCallback((updates: Partial<FileTreeState>) => {
    if (onStateChange && state) {
      onStateChange({ ...state, ...updates })
    } else {
      if (updates.expanded !== undefined) setInternalExpanded(updates.expanded)
      if (updates.selectedFile !== undefined) setInternalSelectedFile(updates.selectedFile)
      if (updates.selectedFiles !== undefined) setInternalSelectedFiles(updates.selectedFiles)
      if (updates.viewMode !== undefined) setInternalViewMode(updates.viewMode)
      if (updates.currentPath !== undefined) setInternalCurrentPath(updates.currentPath)
    }
  }, [onStateChange, state])

  const fileserverBaseUrl = selectedWorkspaceSessionId 
    ? fileserverProxyBaseUrl(selectedWorkspaceSessionId) 
    : null

  const loadTree = useCallback(async (path: string, preserveState = false) => {
    if (!fileserverBaseUrl) return
    setLoading(true)
    setError("")
    try {
      const data = await fetchFileTree(fileserverBaseUrl, path)
      setTree(data)
      if (!preserveState) {
        updateState({ currentPath: path })
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to load file tree")
    } finally {
      setLoading(false)
    }
  }, [fileserverBaseUrl, updateState])

  const refreshTree = useCallback(() => {
    loadTree(currentPath, true)
  }, [loadTree, currentPath])

  useEffect(() => {
    // Load tree for current path (which may be restored from state)
    loadTree(currentPath, true)
  }, [loadTree, currentPath])

  // Focus new folder input when it appears
  useEffect(() => {
    if (newFolderName !== null && newFolderInputRef.current) {
      newFolderInputRef.current.focus()
    }
  }, [newFolderName])

  const toggle = (path: string) => {
    updateState({ expanded: { ...expanded, [path]: !expanded[path] } })
  }

  const handleSelectFile = (path: string, name: string, event?: React.MouseEvent) => {
    const isMultiSelect = event?.ctrlKey || event?.metaKey
    const isRangeSelect = event?.shiftKey
    
    if (isMultiSelect) {
      // Toggle selection
      const newSelection = new Set(selectedFiles)
      if (newSelection.has(path)) {
        newSelection.delete(path)
      } else {
        newSelection.add(path)
      }
      updateState({ selectedFiles: newSelection, selectedFile: path })
    } else if (isRangeSelect && selectedFile) {
      // Range select - for simplicity, just add to selection
      const newSelection = new Set(selectedFiles)
      newSelection.add(path)
      updateState({ selectedFiles: newSelection, selectedFile: path })
    } else {
      // Single select
      updateState({ selectedFile: path, selectedFiles: new Set([path]) })
      if (onPreviewFile && isPreviewable(name)) {
        onPreviewFile(path)
      }
    }
  }

  const handleNavigateToFolder = (path: string) => {
    updateState({ currentPath: path, expanded: {}, selectedFiles: new Set() })
  }

  const handleGoUp = () => {
    if (currentPath === ".") return
    const parts = currentPath.split("/")
    parts.pop()
    const parentPath = parts.length === 0 ? "." : parts.join("/")
    handleNavigateToFolder(parentPath)
  }

  const handleGoHome = () => {
    handleNavigateToFolder(".")
  }
  
  const setViewMode = (mode: ViewMode) => {
    updateState({ viewMode: mode })
  }

  const handleUploadClick = () => {
    fileInputRef.current?.click()
  }

  const handleFileChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = event.target.files
    if (!files || files.length === 0 || !fileserverBaseUrl) return

    setUploading(true)
    setError("")

    try {
      for (const file of Array.from(files)) {
        const destPath = currentPath === "." ? file.name : `${currentPath}/${file.name}`
        await uploadFile(fileserverBaseUrl, destPath, file)
      }
      await refreshTree()
    } catch (err) {
      setError(err instanceof Error ? err.message : "Upload failed")
    } finally {
      setUploading(false)
      // Reset input
      if (fileInputRef.current) {
        fileInputRef.current.value = ""
      }
    }
  }

  const handleDownload = (path: string, isDirectory: boolean) => {
    if (!fileserverBaseUrl) return
    const url = getDownloadUrl(fileserverBaseUrl, path)
    window.open(url, "_blank")
  }

  const handleDownloadSelected = () => {
    if (!fileserverBaseUrl || selectedFiles.size === 0) return
    
    if (selectedFiles.size === 1) {
      const path = Array.from(selectedFiles)[0]
      handleDownload(path, false) // We don't know if it's a directory
    } else {
      const url = getDownloadZipUrl(fileserverBaseUrl, Array.from(selectedFiles), "selected-files.zip")
      window.open(url, "_blank")
    }
  }

  const handleDelete = async (path: string) => {
    if (!fileserverBaseUrl) return
    
    try {
      await deleteFile(fileserverBaseUrl, path)
      await refreshTree()
      // Clear selection if deleted file was selected
      if (selectedFiles.has(path)) {
        const newSelection = new Set(selectedFiles)
        newSelection.delete(path)
        updateState({ selectedFiles: newSelection })
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Delete failed")
    }
  }

  const handleDeleteSelected = async () => {
    if (!fileserverBaseUrl || selectedFiles.size === 0) return
    
    try {
      for (const path of selectedFiles) {
        await deleteFile(fileserverBaseUrl, path)
      }
      await refreshTree()
      updateState({ selectedFiles: new Set() })
    } catch (err) {
      setError(err instanceof Error ? err.message : "Delete failed")
    }
  }

  const handleNewFolder = () => {
    setNewFolderName("")
  }

  const handleCreateFolder = async () => {
    if (!fileserverBaseUrl || !newFolderName?.trim()) {
      setNewFolderName(null)
      return
    }
    
    try {
      const folderPath = currentPath === "." ? newFolderName.trim() : `${currentPath}/${newFolderName.trim()}`
      await createDirectory(fileserverBaseUrl, folderPath)
      await refreshTree()
    } catch (err) {
      setError(err instanceof Error ? err.message : "Create folder failed")
    } finally {
      setNewFolderName(null)
    }
  }

  const clearSelection = () => {
    updateState({ selectedFiles: new Set(), selectedFile: null })
  }

  // Get breadcrumb parts from current path
  const getBreadcrumbs = () => {
    if (currentPath === ".") return [{ name: "Home", path: "." }]
    const parts = currentPath.split("/")
    const breadcrumbs = [{ name: "Home", path: "." }]
    let accumulated = ""
    for (const part of parts) {
      accumulated = accumulated ? `${accumulated}/${part}` : part
      breadcrumbs.push({ name: part, path: accumulated })
    }
    return breadcrumbs
  }

  if (!selectedWorkspaceSessionId) {
    return (
      <div className="h-full flex items-center justify-center p-4 text-sm text-muted-foreground">
        Select a workspace session to browse files.
      </div>
    )
  }

  if (loading && tree.length === 0) {
    return <div className="p-4 text-sm text-muted-foreground">Loading workspace tree...</div>
  }

  if (error && tree.length === 0) {
    return <div className="p-4 text-sm text-destructive">{error}</div>
  }

  const breadcrumbs = getBreadcrumbs()
  const hasSelection = selectedFiles.size > 0

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* Hidden file input */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        className="hidden"
        onChange={handleFileChange}
      />

      {/* Navigation bar */}
      <div className="flex-shrink-0 flex items-center gap-1 p-2 border-b border-border">
        <button
          onClick={handleGoHome}
          disabled={currentPath === "."}
          className={cn(
            "p-1.5 rounded transition-colors",
            currentPath === "." 
              ? "text-muted-foreground/50 cursor-not-allowed" 
              : "text-muted-foreground hover:text-foreground hover:bg-muted"
          )}
          title="Go to root"
        >
          <Home className="w-4 h-4" />
        </button>
        <button
          onClick={handleGoUp}
          disabled={currentPath === "."}
          className={cn(
            "p-1.5 rounded transition-colors",
            currentPath === "." 
              ? "text-muted-foreground/50 cursor-not-allowed" 
              : "text-muted-foreground hover:text-foreground hover:bg-muted"
          )}
          title="Go up"
        >
          <FolderUp className="w-4 h-4" />
        </button>
        
        {/* Breadcrumbs */}
        <div className="flex-1 flex items-center gap-1 overflow-x-auto text-sm ml-2">
          {breadcrumbs.map((crumb, index) => (
            <span key={crumb.path} className="flex items-center gap-1 whitespace-nowrap">
              {index > 0 && <ChevronRight className="w-3 h-3 text-muted-foreground" />}
              <button
                onClick={() => handleNavigateToFolder(crumb.path)}
                className={cn(
                  "hover:text-primary transition-colors",
                  index === breadcrumbs.length - 1 
                    ? "text-foreground font-medium" 
                    : "text-muted-foreground"
                )}
              >
                {crumb.name}
              </button>
            </span>
          ))}
        </div>

        {/* Actions */}
        <div className="flex items-center gap-1 ml-2">
          <button
            onClick={handleUploadClick}
            disabled={uploading}
            className="p-1.5 rounded transition-colors text-muted-foreground hover:text-foreground hover:bg-muted"
            title="Upload files"
          >
            {uploading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Upload className="w-4 h-4" />}
          </button>
          <button
            onClick={handleNewFolder}
            className="p-1.5 rounded transition-colors text-muted-foreground hover:text-foreground hover:bg-muted"
            title="New folder"
          >
            <FolderPlus className="w-4 h-4" />
          </button>
          {hasSelection && (
            <>
              <button
                onClick={handleDownloadSelected}
                className="p-1.5 rounded transition-colors text-muted-foreground hover:text-foreground hover:bg-muted"
                title={`Download ${selectedFiles.size} item(s)`}
              >
                <Download className="w-4 h-4" />
              </button>
              <button
                onClick={handleDeleteSelected}
                className="p-1.5 rounded transition-colors text-muted-foreground hover:text-destructive hover:bg-muted"
                title={`Delete ${selectedFiles.size} item(s)`}
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </>
          )}
        </div>

        {/* View mode toggle */}
        <div className="flex items-center gap-1 ml-2 border-l border-border pl-2">
          <button
            onClick={() => setViewMode("tree")}
            className={cn(
              "p-1.5 rounded transition-colors",
              viewMode === "tree" ? "bg-primary/20 text-primary" : "text-muted-foreground hover:text-foreground hover:bg-muted"
            )}
            title="Tree view"
          >
            <Folder className="w-4 h-4" />
          </button>
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
      </div>

      {/* Selection info bar */}
      {hasSelection && (
        <div className="flex-shrink-0 flex items-center justify-between px-3 py-1.5 bg-primary/10 border-b border-border text-xs">
          <span>{selectedFiles.size} item(s) selected</span>
          <button
            onClick={clearSelection}
            className="text-muted-foreground hover:text-foreground"
          >
            Clear selection
          </button>
        </div>
      )}

      {/* Error message */}
      {error && (
        <div className="flex-shrink-0 px-3 py-2 bg-destructive/10 text-destructive text-xs">
          {error}
        </div>
      )}

      {/* New folder input */}
      {newFolderName !== null && (
        <div className="flex-shrink-0 flex items-center gap-2 px-3 py-2 border-b border-border bg-muted/30">
          <FolderPlus className="w-4 h-4 text-muted-foreground" />
          <input
            ref={newFolderInputRef}
            type="text"
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleCreateFolder()
              if (e.key === "Escape") setNewFolderName(null)
            }}
            onBlur={handleCreateFolder}
            placeholder="New folder name..."
            className="flex-1 bg-transparent border-none outline-none text-sm"
          />
        </div>
      )}

      {/* File content */}
      <div className="flex-1 overflow-auto" onClick={(e) => {
        // Clear selection when clicking empty space
        if (e.target === e.currentTarget) {
          clearSelection()
        }
      }}>
        {tree.length === 0 ? (
          <div className="text-sm text-muted-foreground p-4">No files found.</div>
        ) : viewMode === "tree" ? (
          <TreeView
            nodes={tree}
            expanded={expanded}
            onToggle={toggle}
            selectedFiles={selectedFiles}
            onSelectFile={handleSelectFile}
            onNavigateToFolder={handleNavigateToFolder}
            onDownload={handleDownload}
            onDelete={handleDelete}
            fileserverBaseUrl={fileserverBaseUrl}
          />
        ) : viewMode === "list" ? (
          <ListView 
            files={tree} 
            selectedFiles={selectedFiles}
            onSelectFile={handleSelectFile}
            onNavigateToFolder={handleNavigateToFolder}
            onDownload={handleDownload}
            onDelete={handleDelete}
            fileserverBaseUrl={fileserverBaseUrl}
          />
        ) : (
          <GridView 
            files={tree} 
            selectedFiles={selectedFiles}
            onSelectFile={handleSelectFile}
            onNavigateToFolder={handleNavigateToFolder}
            onDownload={handleDownload}
            onDelete={handleDelete}
            fileserverBaseUrl={fileserverBaseUrl}
          />
        )}
      </div>
    </div>
  )
}

// Context menu wrapper for file items
function FileContextMenu({ 
  children, 
  node, 
  onDownload, 
  onDelete 
}: { 
  children: React.ReactNode
  node: FileNode
  onDownload: (path: string, isDirectory: boolean) => void
  onDelete: (path: string) => void
}) {
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        {children}
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onClick={() => onDownload(node.path, node.type === "directory")}>
          <Download className="w-4 h-4 mr-2" />
          {node.type === "directory" ? "Download as ZIP" : "Download"}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem 
          onClick={() => onDelete(node.path)}
          className="text-destructive focus:text-destructive"
        >
          <Trash2 className="w-4 h-4 mr-2" />
          Delete
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}

// Tree View Component
function TreeView({
  nodes,
  expanded,
  onToggle,
  selectedFiles,
  onSelectFile,
  onNavigateToFolder,
  onDownload,
  onDelete,
  fileserverBaseUrl,
}: {
  nodes: FileNode[]
  expanded: Record<string, boolean>
  onToggle: (path: string) => void
  selectedFiles: Set<string>
  onSelectFile: (path: string, name: string, event?: React.MouseEvent) => void
  onNavigateToFolder: (path: string) => void
  onDownload: (path: string, isDirectory: boolean) => void
  onDelete: (path: string) => void
  fileserverBaseUrl: string | null
}) {
  // Sort: directories first, then files, both alphabetically
  const sortedNodes = [...nodes].sort((a, b) => {
    if (a.type === "directory" && b.type !== "directory") return -1
    if (a.type !== "directory" && b.type === "directory") return 1
    return a.name.localeCompare(b.name)
  })

  return (
    <ul className="py-1">
      {sortedNodes.map((node) => (
        <TreeRow
          key={node.path}
          node={node}
          level={0}
          expanded={expanded}
          onToggle={onToggle}
          onSelectFile={onSelectFile}
          selectedFiles={selectedFiles}
          onNavigateToFolder={onNavigateToFolder}
          onDownload={onDownload}
          onDelete={onDelete}
        />
      ))}
    </ul>
  )
}

// Tree Row Component
function TreeRow({
  node,
  level,
  expanded,
  onToggle,
  onSelectFile,
  selectedFiles,
  onNavigateToFolder,
  onDownload,
  onDelete,
}: {
  node: FileNode
  level: number
  expanded: Record<string, boolean>
  onToggle: (path: string) => void
  onSelectFile: (path: string, name: string, event?: React.MouseEvent) => void
  selectedFiles: Set<string>
  onNavigateToFolder: (path: string) => void
  onDownload: (path: string, isDirectory: boolean) => void
  onDelete: (path: string) => void
}) {
  const isDir = node.type === "directory"
  const isExpanded = expanded[node.path]
  const isSelected = selectedFiles.has(node.path)

  // Sort children: directories first, then files
  const sortedChildren = node.children ? [...node.children].sort((a, b) => {
    if (a.type === "directory" && b.type !== "directory") return -1
    if (a.type !== "directory" && b.type === "directory") return 1
    return a.name.localeCompare(b.name)
  }) : []

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation()
    if (isDir && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
      onToggle(node.path)
    }
    onSelectFile(node.path, node.name, e)
  }

  const handleDoubleClick = () => {
    if (isDir) {
      onNavigateToFolder(node.path)
    }
  }

  return (
    <li>
      <FileContextMenu node={node} onDownload={onDownload} onDelete={onDelete}>
        <div
          className={cn(
            "flex items-center gap-1.5 py-1.5 px-2 cursor-pointer transition-colors",
            isSelected 
              ? "bg-primary/10 text-primary" 
              : "hover:bg-muted text-muted-foreground hover:text-foreground"
          )}
          style={{ paddingLeft: `${level * 16 + 8}px` }}
          onClick={handleClick}
          onDoubleClick={handleDoubleClick}
        >
          {isDir ? (
            <span className="flex-shrink-0 text-muted-foreground">
              {isExpanded ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
            </span>
          ) : (
            <span className="w-4 flex-shrink-0" />
          )}
          <FileIcon filename={node.name} isDirectory={isDir} size={18} className="flex-shrink-0" />
          <span className="truncate text-sm">{node.name}</span>
          {isDir && node.children && (
            <span className="text-xs text-muted-foreground/60 ml-auto pr-2">
              {node.children.length}
            </span>
          )}
        </div>
      </FileContextMenu>
      {isDir && isExpanded && sortedChildren.length > 0 && (
        <ul>
          {sortedChildren.map((child) => (
            <TreeRow
              key={child.path}
              node={child}
              level={level + 1}
              expanded={expanded}
              onToggle={onToggle}
              onSelectFile={onSelectFile}
              selectedFiles={selectedFiles}
              onNavigateToFolder={onNavigateToFolder}
              onDownload={onDownload}
              onDelete={onDelete}
            />
          ))}
        </ul>
      )}
    </li>
  )
}

// List View Component
function ListView({ 
  files, 
  selectedFiles,
  onSelectFile,
  onNavigateToFolder,
  onDownload,
  onDelete,
  fileserverBaseUrl,
}: { 
  files: FileNode[]
  selectedFiles: Set<string>
  onSelectFile: (path: string, name: string, event?: React.MouseEvent) => void
  onNavigateToFolder: (path: string) => void
  onDownload: (path: string, isDirectory: boolean) => void
  onDelete: (path: string) => void
  fileserverBaseUrl: string | null
}) {
  // Sort: directories first, then files
  const sortedFiles = [...files].sort((a, b) => {
    if (a.type === "directory" && b.type !== "directory") return -1
    if (a.type !== "directory" && b.type === "directory") return 1
    return a.name.localeCompare(b.name)
  })

  return (
    <div className="min-w-full">
      {/* Header */}
      <div className="sticky top-0 bg-card z-10 flex items-center gap-2 px-3 py-2 border-b border-border text-xs text-muted-foreground font-medium">
        <div className="flex-1 min-w-0">Name</div>
        <div className="w-24 text-right hidden sm:block">Modified</div>
        <div className="w-20 text-right hidden sm:block">Size</div>
      </div>
      
      {/* Files */}
      <div className="divide-y divide-border/50">
        {sortedFiles.map((file) => {
          const isSelected = selectedFiles.has(file.path)
          return (
            <FileContextMenu key={file.path} node={file} onDownload={onDownload} onDelete={onDelete}>
              <div
                onClick={(e) => {
                  if (file.type === "file" || e.ctrlKey || e.metaKey || e.shiftKey) {
                    onSelectFile(file.path, file.name, e)
                  } else {
                    onSelectFile(file.path, file.name, e)
                  }
                }}
                onDoubleClick={() => {
                  if (file.type === "directory") {
                    onNavigateToFolder(file.path)
                  }
                }}
                className={cn(
                  "flex items-center gap-2 px-3 py-2 transition-colors cursor-pointer",
                  isSelected ? "bg-primary/10" : "hover:bg-muted/50"
                )}
              >
                <div className="flex-1 min-w-0 flex items-center gap-2">
                  <FileIcon filename={file.name} isDirectory={file.type === "directory"} size={20} />
                  <span className="truncate text-sm">{file.name}</span>
                  {file.type === "directory" && file.children && (
                    <span className="text-xs text-muted-foreground/60">
                      ({file.children.length})
                    </span>
                  )}
                </div>
                <div className="w-24 text-right text-xs text-muted-foreground hidden sm:block">
                  {formatDate(file.modified)}
                </div>
                <div className="w-20 text-right text-xs text-muted-foreground hidden sm:block">
                  {file.type === "file" ? formatFileSize(file.size) : "-"}
                </div>
              </div>
            </FileContextMenu>
          )
        })}
      </div>
    </div>
  )
}

// Grid View Component
function GridView({ 
  files, 
  selectedFiles,
  onSelectFile,
  onNavigateToFolder,
  onDownload,
  onDelete,
  fileserverBaseUrl,
}: { 
  files: FileNode[]
  selectedFiles: Set<string>
  onSelectFile: (path: string, name: string, event?: React.MouseEvent) => void
  onNavigateToFolder: (path: string) => void
  onDownload: (path: string, isDirectory: boolean) => void
  onDelete: (path: string) => void
  fileserverBaseUrl: string | null
}) {
  // Sort: directories first, then files
  const sortedFiles = [...files].sort((a, b) => {
    if (a.type === "directory" && b.type !== "directory") return -1
    if (a.type !== "directory" && b.type === "directory") return 1
    return a.name.localeCompare(b.name)
  })
  
  return (
    <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2 p-2">
      {sortedFiles.map((file) => {
        const isSelected = selectedFiles.has(file.path)
        return (
          <FileContextMenu key={file.path} node={file} onDownload={onDownload} onDelete={onDelete}>
            <div
              onClick={(e) => onSelectFile(file.path, file.name, e)}
              onDoubleClick={() => {
                if (file.type === "directory") {
                  onNavigateToFolder(file.path)
                }
              }}
              className={cn(
                "flex flex-col items-center gap-2 p-3 rounded-lg cursor-pointer transition-colors hover:bg-muted/50",
                isSelected && "bg-primary/10 ring-1 ring-primary/30"
              )}
            >
              <FileIcon filename={file.name} isDirectory={file.type === "directory"} size={48} />
              <span className="text-xs text-center truncate w-full" title={file.name}>
                {file.name}
              </span>
            </div>
          </FileContextMenu>
        )
      })}
    </div>
  )
}
