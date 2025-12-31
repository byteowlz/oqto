"use client"

import { useEffect, useState, useCallback, useRef } from "react"
import { Eye, Loader2, Pencil, Save, X, FileText, Download, ExternalLink, ZoomIn, ZoomOut } from "lucide-react"
import { useApp } from "@/components/app-context"
import { fileserverProxyBaseUrl } from "@/lib/control-plane-client"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import CodeEditor from "@uiw/react-textarea-code-editor"
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter"
import { oneDark, oneLight } from "react-syntax-highlighter/dist/cjs/styles/prism"

interface PreviewViewProps {
  filePath?: string | null
  className?: string
}

// Simple LRU cache for file contents
const fileCache = new Map<string, { content: string; timestamp: number }>()
const CACHE_MAX_SIZE = 50
const CACHE_TTL_MS = 30000 // 30 seconds

function getCachedContent(key: string): string | null {
  const entry = fileCache.get(key)
  if (!entry) return null
  if (Date.now() - entry.timestamp > CACHE_TTL_MS) {
    fileCache.delete(key)
    return null
  }
  return entry.content
}

function setCachedContent(key: string, content: string) {
  // Evict oldest entries if cache is full
  if (fileCache.size >= CACHE_MAX_SIZE) {
    const oldestKey = fileCache.keys().next().value
    if (oldestKey) fileCache.delete(oldestKey)
  }
  fileCache.set(key, { content, timestamp: Date.now() })
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

async function fetchHighlightedContent(baseUrl: string, path: string): Promise<string> {
  const url = new URL(`${baseUrl}/file`, window.location.origin)
  url.searchParams.set("path", path)
  url.searchParams.set("highlight", "true")
  const res = await fetch(url.toString(), { credentials: "include" })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `Unable to fetch highlighted ${path}`)
  }
  const html = await res.text()
  // Validate that we got actual HTML, not just raw text
  if (!html.includes("<div") && !html.includes("<span")) {
    throw new Error("Server returned plain text instead of highlighted HTML")
  }
  return html
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
  const [highlightedContent, setHighlightedContent] = useState<string>("")
  const [editedContent, setEditedContent] = useState<string>("")
  const [showLoading, setShowLoading] = useState(false) // Delayed loading indicator
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string>("")
  const [isEditing, setIsEditing] = useState(false)
  const [isDarkMode, setIsDarkMode] = useState(false)
  const loadingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  
  // Check mobile at render time (safe because we only use it client-side in effects)
  const isMobileRef = useRef(false)
  if (typeof window !== "undefined") {
    isMobileRef.current = window.innerWidth < 640
  }

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
    // Clear any pending loading timer
    if (loadingTimerRef.current) {
      clearTimeout(loadingTimerRef.current)
      loadingTimerRef.current = null
    }

    if (!filePath || !fileserverBaseUrl) {
      setContent("")
      setEditedContent("")
      setIsEditing(false)
      setShowLoading(false)
      return
    }

    // Don't fetch content for PDF or image files - they render via URL
    const filename = filePath.split("/").pop() || filePath
    if (isPdf(filename) || isImage(filename)) {
      setContent("")
      setEditedContent("")
      setIsEditing(false)
      setShowLoading(false)
      return
    }

    // Reset highlighted content on file change
    setHighlightedContent("")

    // Check cache first for instant preview
    const cacheKey = `${selectedWorkspaceSessionId}:${filePath}`
    const highlightCacheKey = `${cacheKey}:highlighted`
    const cached = getCachedContent(cacheKey)
    const cachedHighlighted = getCachedContent(highlightCacheKey)
    
    if (cached !== null) {
      setContent(cached)
      setEditedContent(cached)
      setError("")
      setIsEditing(false)
      setShowLoading(false)
      if (cachedHighlighted !== null) {
        setHighlightedContent(cachedHighlighted)
      } else {
        // Fetch highlighted version in background
        fetchHighlightedContent(fileserverBaseUrl, filePath)
          .then((html) => {
            setCachedContent(highlightCacheKey, html)
            setHighlightedContent(html)
          })
          .catch((err) => {
            console.warn("[PreviewView] Failed to fetch highlighted content:", err)
          }) // Fallback to raw content on error
      }
      return
    }

    // No cache hit - fetch from server
    // Only show loading spinner after 150ms to avoid flicker for fast responses
    setError("")
    setIsEditing(false)
    loadingTimerRef.current = setTimeout(() => {
      setShowLoading(true)
    }, 150)

    // Fetch raw content first (for editing), then highlighted version
    fetchFileContent(fileserverBaseUrl, filePath)
      .then((data) => {
        // Cache and set raw content immediately
        setCachedContent(cacheKey, data)
        setContent(data)
        setEditedContent(data)
        
        // Then fetch highlighted version in background
        fetchHighlightedContent(fileserverBaseUrl, filePath)
          .then((html) => {
            console.log("[PreviewView] Got highlighted content, length:", html.length)
            setCachedContent(highlightCacheKey, html)
            setHighlightedContent(html)
          })
          .catch((err) => {
            console.warn("[PreviewView] Failed to fetch highlighted content:", err)
          }) // Fallback to raw content on error
      })
      .catch((err) => {
        setError(err.message ?? "Failed to load file")
      })
      .finally(() => {
        if (loadingTimerRef.current) {
          clearTimeout(loadingTimerRef.current)
          loadingTimerRef.current = null
        }
        setShowLoading(false)
      })

    return () => {
      if (loadingTimerRef.current) {
        clearTimeout(loadingTimerRef.current)
        loadingTimerRef.current = null
      }
    }
  }, [filePath, fileserverBaseUrl, selectedWorkspaceSessionId])

  const handleSave = useCallback(async () => {
    if (!fileserverBaseUrl || !filePath || !selectedWorkspaceSessionId) return
    
    setSaving(true)
    setError("")
    try {
      await saveFileContent(fileserverBaseUrl, filePath, editedContent)
      setContent(editedContent)
      // Update the cache with the new content
      const cacheKey = `${selectedWorkspaceSessionId}:${filePath}`
      setCachedContent(cacheKey, editedContent)
      setIsEditing(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save file")
    } finally {
      setSaving(false)
    }
  }, [fileserverBaseUrl, filePath, editedContent, selectedWorkspaceSessionId])

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

  // Loading state (only shown after delay to avoid flicker)
  if (showLoading) {
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
        <div className="flex-shrink-0 flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
          <div className="flex items-center gap-1.5 flex-1 min-w-0">
            <FileText className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
            <p className="text-xs font-mono text-muted-foreground truncate" title={filePath}>
              {filename}
            </p>
          </div>
          <div className="flex items-center gap-0.5 ml-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => window.open(fileUrl, "_blank")}
              className="h-6 px-1.5 text-xs"
              title="Open in new tab"
            >
              <ExternalLink className="w-3 h-3" />
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
              className="h-6 px-1.5 text-xs"
              title="Download"
            >
              <Download className="w-3 h-3" />
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
        <div className="flex-shrink-0 flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
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
      <div className="flex-shrink-0 flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
        <p className="text-xs font-mono text-muted-foreground truncate flex-1" title={filePath}>
          {filename}
          {isEditing && <span className="ml-2 text-primary">(editing)</span>}
        </p>
        <div className="flex items-center gap-0.5 ml-2">
          {isEditing ? (
            <>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleCancel}
                disabled={saving}
                className="h-6 px-1.5 text-xs"
              >
                <X className="w-3 h-3 mr-1" />
                Cancel
              </Button>
              <Button
                variant="default"
                size="sm"
                onClick={handleSave}
                disabled={saving}
                className="h-6 px-1.5 text-xs"
              >
                {saving ? (
                  <Loader2 className="w-3 h-3 mr-1 animate-spin" />
                ) : (
                  <Save className="w-3 h-3 mr-1" />
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
                className="h-6 px-1.5 text-xs"
              >
                <Pencil className="w-3 h-3 mr-1" />
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
        ) : highlightedContent ? (
          // Server-rendered syntax highlighting with line numbers (table-based)
          <div 
            className="overflow-auto py-1"
            style={{ minHeight: "100%" }}
            dangerouslySetInnerHTML={{ __html: highlightedContent }}
          />
        ) : content ? (
          // Fallback: plain text with line numbers using table layout
          <div 
            className="overflow-auto"
            style={{ 
              minHeight: "100%",
              fontSize: "12px",
              lineHeight: "1.5",
              fontFamily: "ui-monospace, SFMono-Regular, SF Mono, Consolas, Liberation Mono, Menlo, monospace",
            }}
          >
            <table className="w-full border-collapse" style={{ minWidth: "100%" }}>
              <tbody>
                {content.split("\n").map((line, i) => (
                  <tr key={i}>
                    <td 
                      className="select-none text-right text-muted-foreground align-top px-1"
                      style={{ 
                        minWidth: "2.5em", 
                        width: "1%",
                        whiteSpace: "nowrap",
                        opacity: 0.5,
                      }}
                    >
                      {i + 1}
                    </td>
                    <td className="align-top">
                      <pre 
                        className="m-0"
                        style={{ 
                          fontFamily: "inherit", 
                          whiteSpace: "pre",
                        }}
                      >
                        {line || " "}
                      </pre>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : null}
      </div>
    </div>
  )
}
