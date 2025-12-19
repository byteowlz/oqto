"use client"

import { useEffect, useState, useCallback } from "react"
import { Eye, Loader2, Pencil, Save, X, FileText, Download, ExternalLink, ZoomIn, ZoomOut } from "lucide-react"
import { useApp } from "@/components/app-context"
import { fileserverProxyBaseUrl } from "@/lib/control-plane-client"
import { cn } from "@/lib/utils"
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter"
import { oneDark, oneLight } from "react-syntax-highlighter/dist/esm/styles/prism"
import { Button } from "@/components/ui/button"
import CodeEditor from "@uiw/react-textarea-code-editor"

interface PreviewViewProps {
  filePath?: string | null
  className?: string
}

// File extensions that can be edited
const EDITABLE_EXTENSIONS = new Set([
  ".txt", ".md", ".mdx", ".markdown",
  ".json", ".jsonc",
  ".yaml", ".yml", ".toml", ".ini", ".cfg", ".conf",
  ".env", ".gitignore", ".dockerignore",
  ".js", ".jsx", ".ts", ".tsx",
  ".css", ".scss", ".sass", ".less",
  ".html", ".htm", ".xml",
  ".py", ".rb", ".go", ".rs", ".java", ".c", ".cpp", ".h",
  ".sh", ".bash", ".zsh", ".fish",
  ".sql", ".graphql",
])

// Image extensions
const IMAGE_EXTENSIONS = new Set([
  ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".ico",
])

// PDF extension
const PDF_EXTENSIONS = new Set([".pdf"])

// Typst extension
const TYPST_EXTENSIONS = new Set([".typ"])

// Map file extensions to syntax highlighter language
function getLanguage(filename: string): string {
  const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase()
  
  const languageMap: Record<string, string> = {
    ".js": "javascript",
    ".jsx": "jsx",
    ".ts": "typescript",
    ".tsx": "tsx",
    ".json": "json",
    ".jsonc": "json",
    ".md": "markdown",
    ".mdx": "markdown",
    ".markdown": "markdown",
    ".py": "python",
    ".rb": "ruby",
    ".go": "go",
    ".rs": "rust",
    ".java": "java",
    ".c": "c",
    ".cpp": "cpp",
    ".h": "c",
    ".hpp": "cpp",
    ".cs": "csharp",
    ".php": "php",
    ".swift": "swift",
    ".kt": "kotlin",
    ".scala": "scala",
    ".html": "html",
    ".htm": "html",
    ".xml": "xml",
    ".css": "css",
    ".scss": "scss",
    ".sass": "sass",
    ".less": "less",
    ".yaml": "yaml",
    ".yml": "yaml",
    ".toml": "toml",
    ".ini": "ini",
    ".cfg": "ini",
    ".conf": "ini",
    ".sh": "bash",
    ".bash": "bash",
    ".zsh": "bash",
    ".fish": "bash",
    ".sql": "sql",
    ".graphql": "graphql",
    ".vue": "vue",
    ".svelte": "svelte",
    ".txt": "text",
    ".log": "text",
    ".env": "bash",
    ".gitignore": "text",
    ".dockerignore": "text",
    ".typ": "latex", // Typst uses latex highlighting as closest match
  }
  
  return languageMap[ext] || "text"
}

function isEditable(filename: string): boolean {
  const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase()
  return EDITABLE_EXTENSIONS.has(ext)
}

function isImage(filename: string): boolean {
  const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase()
  return IMAGE_EXTENSIONS.has(ext)
}

function isPdf(filename: string): boolean {
  const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase()
  return PDF_EXTENSIONS.has(ext)
}

function isTypst(filename: string): boolean {
  const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase()
  return TYPST_EXTENSIONS.has(ext)
}

function getFileUrl(baseUrl: string, path: string): string {
  const url = new URL(`${baseUrl}/file`, window.location.origin)
  url.searchParams.set("path", path)
  return url.toString()
}

// Alias for backward compatibility
const getImageUrl = getFileUrl

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

async function saveFileContent(baseUrl: string, path: string, content: string): Promise<void> {
  const url = new URL(`${baseUrl}/file`, window.location.origin)
  url.searchParams.set("path", path)
  
  // Create form data with the file content
  const formData = new FormData()
  const blob = new Blob([content], { type: "text/plain" })
  const filename = path.split("/").pop() || "file"
  formData.append("file", blob, filename)
  
  const res = await fetch(url.toString(), { 
    method: "POST",
    credentials: "include",
    body: formData,
  })
  
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `Unable to save ${path}`)
  }
}

export function PreviewView({ filePath, className }: PreviewViewProps) {
  const { selectedWorkspaceSessionId } = useApp()
  const [content, setContent] = useState<string>("")
  const [editedContent, setEditedContent] = useState<string>("")
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string>("")
  const [isEditing, setIsEditing] = useState(false)
  const [isDarkMode, setIsDarkMode] = useState(false)

  const fileserverBaseUrl = selectedWorkspaceSessionId 
    ? fileserverProxyBaseUrl(selectedWorkspaceSessionId) 
    : null

  // Detect dark mode
  useEffect(() => {
    const checkDarkMode = () => {
      setIsDarkMode(document.documentElement.classList.contains("dark"))
    }
    checkDarkMode()
    
    const observer = new MutationObserver(checkDarkMode)
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })
    
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    if (!filePath || !fileserverBaseUrl) {
      setContent("")
      setEditedContent("")
      setIsEditing(false)
      return
    }

    // Don't fetch content for PDF files - they render via URL
    const filename = filePath.split("/").pop() || filePath
    if (isPdf(filename)) {
      setContent("")
      setEditedContent("")
      setIsEditing(false)
      setLoading(false)
      return
    }

    setLoading(true)
    setError("")
    setIsEditing(false)
    fetchFileContent(fileserverBaseUrl, filePath)
      .then((data) => {
        setContent(data)
        setEditedContent(data)
        setLoading(false)
      })
      .catch((err) => {
        setError(err.message ?? "Failed to load file")
        setLoading(false)
      })
  }, [filePath, fileserverBaseUrl])

  const handleSave = useCallback(async () => {
    if (!fileserverBaseUrl || !filePath) return
    
    setSaving(true)
    setError("")
    try {
      await saveFileContent(fileserverBaseUrl, filePath, editedContent)
      setContent(editedContent)
      setIsEditing(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save file")
    } finally {
      setSaving(false)
    }
  }, [fileserverBaseUrl, filePath, editedContent])

  const handleCancel = useCallback(() => {
    setEditedContent(content)
    setIsEditing(false)
    setError("")
  }, [content])

  const handleStartEdit = useCallback(() => {
    setEditedContent(content)
    setIsEditing(true)
  }, [content])

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

  // Get filename from path
  const filename = filePath.split("/").pop() || filePath
  const language = getLanguage(filename)
  const canEdit = isEditable(filename)
  const isImageFile = isImage(filename)
  const isPdfFile = isPdf(filename)
  const isTypstFile = isTypst(filename)
  const fileUrl = fileserverBaseUrl ? getFileUrl(fileserverBaseUrl, filePath) : null
  const imageUrl = isImageFile && fileserverBaseUrl ? getImageUrl(fileserverBaseUrl, filePath) : null

  // For PDF files, render with iframe
  if (isPdfFile && fileUrl) {
    return (
      <div className={cn("h-full flex flex-col overflow-hidden", className)}>
        {/* Header */}
        <div className="flex-shrink-0 flex items-center justify-between px-3 py-2 border-b border-border bg-muted/30">
          <div className="flex items-center gap-2 flex-1 min-w-0">
            <FileText className="w-4 h-4 text-muted-foreground flex-shrink-0" />
            <p className="text-xs font-mono text-muted-foreground truncate" title={filePath}>
              {filename}
            </p>
          </div>
          <div className="flex items-center gap-1 ml-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => window.open(fileUrl, "_blank")}
              className="h-7 px-2 text-xs"
              title="Open in new tab"
            >
              <ExternalLink className="w-3.5 h-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                const link = document.createElement("a")
                link.href = fileUrl
                link.download = filename
                link.click()
              }}
              className="h-7 px-2 text-xs"
              title="Download"
            >
              <Download className="w-3.5 h-3.5" />
            </Button>
          </div>
        </div>

        {/* PDF content */}
        <div className="flex-1 overflow-hidden bg-muted/30">
          <iframe
            src={fileUrl}
            className="w-full h-full border-0"
            title={filename}
          />
        </div>
      </div>
    )
  }

  // For images, render a different view
  if (isImageFile) {
    return (
      <div className={cn("h-full flex flex-col overflow-hidden", className)}>
        {/* Header */}
        <div className="flex-shrink-0 flex items-center justify-between px-3 py-2 border-b border-border bg-muted/30">
          <p className="text-xs font-mono text-muted-foreground truncate flex-1" title={filePath}>
            {filename}
          </p>
        </div>

        {/* Image content */}
        <div className="flex-1 overflow-auto flex items-center justify-center p-4 bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMjAiIGhlaWdodD0iMjAiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+PGRlZnM+PHBhdHRlcm4gaWQ9ImdyaWQiIHdpZHRoPSIyMCIgaGVpZ2h0PSIyMCIgcGF0dGVyblVuaXRzPSJ1c2VyU3BhY2VPblVzZSI+PHJlY3QgZmlsbD0iIzgwODA4MCIgeD0iMCIgeT0iMCIgd2lkdGg9IjEwIiBoZWlnaHQ9IjEwIiBvcGFjaXR5PSIwLjEiLz48cmVjdCBmaWxsPSIjODA4MDgwIiB4PSIxMCIgeT0iMTAiIHdpZHRoPSIxMCIgaGVpZ2h0PSIxMCIgb3BhY2l0eT0iMC4xIi8+PC9wYXR0ZXJuPjwvZGVmcz48cmVjdCBmaWxsPSJ1cmwoI2dyaWQpIiB3aWR0aD0iMTAwJSIgaGVpZ2h0PSIxMDAlIi8+PC9zdmc+')]">
          {imageUrl && (
            <img
              src={imageUrl}
              alt={filename}
              className="max-w-full max-h-full object-contain"
              style={{ imageRendering: "auto" }}
            />
          )}
        </div>
      </div>
    )
  }

  return (
    <div className={cn("h-full flex flex-col overflow-hidden", className)}>
      {/* Header */}
      <div className="flex-shrink-0 flex items-center justify-between px-3 py-2 border-b border-border bg-muted/30">
        <p className="text-xs font-mono text-muted-foreground truncate flex-1" title={filePath}>
          {filename}
          {isEditing && <span className="ml-2 text-primary">(editing)</span>}
        </p>
        <div className="flex items-center gap-1 ml-2">
          {isEditing ? (
            <>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleCancel}
                disabled={saving}
                className="h-7 px-2 text-xs"
              >
                <X className="w-3.5 h-3.5 mr-1" />
                Cancel
              </Button>
              <Button
                variant="default"
                size="sm"
                onClick={handleSave}
                disabled={saving}
                className="h-7 px-2 text-xs"
              >
                {saving ? (
                  <Loader2 className="w-3.5 h-3.5 mr-1 animate-spin" />
                ) : (
                  <Save className="w-3.5 h-3.5 mr-1" />
                )}
                Save
              </Button>
            </>
          ) : (
            canEdit && (
              <Button
                variant="ghost"
                size="sm"
                onClick={handleStartEdit}
                className="h-7 px-2 text-xs"
              >
                <Pencil className="w-3.5 h-3.5 mr-1" />
                Edit
              </Button>
            )
          )}
        </div>
      </div>

      {/* Error message */}
      {error && (
        <div className="flex-shrink-0 px-3 py-2 bg-destructive/10 text-destructive text-xs">
          {error}
        </div>
      )}

      {/* Content */}
      <div className="flex-1 overflow-auto">
        {isEditing ? (
          <CodeEditor
            value={editedContent}
            language={language}
            onChange={(e) => setEditedContent(e.target.value)}
            padding={12}
            data-color-mode={isDarkMode ? "dark" : "light"}
            style={{
              fontSize: 12,
              fontFamily: "ui-monospace, SFMono-Regular, SF Mono, Consolas, Liberation Mono, Menlo, monospace",
              minHeight: "100%",
              backgroundColor: isDarkMode ? "#1e1e1e" : "#ffffff",
            }}
          />
        ) : (
          <SyntaxHighlighter
            language={language}
            style={isDarkMode ? oneDark : oneLight}
            customStyle={{
              margin: 0,
              padding: "12px",
              fontSize: "12px",
              lineHeight: "1.5",
              background: "transparent",
              minHeight: "100%",
            }}
            showLineNumbers
            lineNumberStyle={{
              minWidth: "3em",
              paddingRight: "1em",
              textAlign: "right",
              opacity: 0.5,
            }}
            wrapLongLines
          >
            {content}
          </SyntaxHighlighter>
        )}
      </div>
    </div>
  )
}
