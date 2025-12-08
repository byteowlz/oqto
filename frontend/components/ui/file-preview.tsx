"use client"

import { useEffect, useState, useMemo } from "react"
import {
  FileCode,
  FileText,
  FileImage,
  FileSpreadsheet,
  FileQuestion,
  Loader2,
  AlertTriangle,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { getFileTypeInfo, type FileCategory } from "@/lib/file-types"
import { CodeViewer } from "@/components/ui/code-viewer"
import { ImageViewer } from "@/components/ui/image-viewer"
import { CSVViewer } from "@/components/ui/csv-viewer"
import { MarkdownRenderer } from "@/components/ui/markdown-renderer"
import { ScrollArea } from "@/components/ui/scroll-area"

interface FilePreviewProps {
  filename: string
  content?: string
  contentUrl?: string
  className?: string
  onError?: (error: Error) => void
}

// Icons for different file categories
const categoryIcons: Record<FileCategory, React.ComponentType<{ className?: string }>> = {
  code: FileCode,
  markdown: FileText,
  image: FileImage,
  pdf: FileText,
  csv: FileSpreadsheet,
  json: FileCode,
  yaml: FileCode,
  xml: FileCode,
  text: FileText,
  binary: FileQuestion,
  unknown: FileQuestion,
}

function LoadingState() {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-2 text-muted-foreground">
      <Loader2 className="w-8 h-8 animate-spin" />
      <p className="text-sm">Loading preview...</p>
    </div>
  )
}

function ErrorState({ message }: { message: string }) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-2 text-muted-foreground">
      <AlertTriangle className="w-8 h-8 text-destructive" />
      <p className="text-sm">{message}</p>
    </div>
  )
}

function BinaryState({ filename }: { filename: string }) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-2 text-muted-foreground">
      <FileQuestion className="w-12 h-12 opacity-50" />
      <p className="text-sm">Cannot preview binary file</p>
      <p className="text-xs">{filename}</p>
    </div>
  )
}

function PlainTextViewer({ content, filename }: { content: string; filename?: string }) {
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 bg-muted border-b border-border shrink-0">
        <div className="flex items-center gap-2">
          <FileText className="w-4 h-4 text-muted-foreground" />
          {filename && <span className="text-sm font-medium truncate max-w-[200px]">{filename}</span>}
        </div>
      </div>
      <ScrollArea className="flex-1">
        <pre className="p-4 text-sm font-mono whitespace-pre-wrap break-words">{content}</pre>
      </ScrollArea>
    </div>
  )
}

export function FilePreview({
  filename,
  content,
  contentUrl,
  className,
  onError,
}: FilePreviewProps) {
  const [loadedContent, setLoadedContent] = useState<string | null>(content ?? null)
  const [isLoading, setIsLoading] = useState(!content && !!contentUrl)
  const [error, setError] = useState<string | null>(null)

  const fileInfo = useMemo(() => getFileTypeInfo(filename), [filename])

  // Fetch content from URL if needed
  useEffect(() => {
    if (content) {
      setLoadedContent(content)
      setIsLoading(false)
      setError(null)
      return
    }

    if (!contentUrl) {
      setIsLoading(false)
      return
    }

    // Don't fetch binary files as text
    if (fileInfo.category === "binary") {
      setIsLoading(false)
      return
    }

    // Don't fetch images as text
    if (fileInfo.category === "image") {
      setIsLoading(false)
      return
    }

    setIsLoading(true)
    setError(null)

    fetch(contentUrl, { cache: "no-store" })
      .then(async (res) => {
        if (!res.ok) {
          throw new Error(`Failed to load file: ${res.statusText}`)
        }
        return res.text()
      })
      .then((text) => {
        setLoadedContent(text)
        setIsLoading(false)
      })
      .catch((err) => {
        const errorMessage = err instanceof Error ? err.message : "Failed to load file"
        setError(errorMessage)
        setIsLoading(false)
        onError?.(err instanceof Error ? err : new Error(errorMessage))
      })
  }, [content, contentUrl, fileInfo.category, onError])

  // Loading state
  if (isLoading) {
    return (
      <div className={cn("h-full", className)}>
        <LoadingState />
      </div>
    )
  }

  // Error state
  if (error) {
    return (
      <div className={cn("h-full", className)}>
        <ErrorState message={error} />
      </div>
    )
  }

  // Binary files
  if (fileInfo.category === "binary") {
    return (
      <div className={cn("h-full", className)}>
        <BinaryState filename={filename} />
      </div>
    )
  }

  // Image files
  if (fileInfo.category === "image") {
    const imageUrl = contentUrl || (content ? `data:${fileInfo.mimeType};base64,${content}` : "")
    return (
      <div className={cn("h-full", className)}>
        <ImageViewer src={imageUrl} filename={filename} />
      </div>
    )
  }

  // No content available
  if (!loadedContent) {
    return (
      <div className={cn("h-full", className)}>
        <ErrorState message="No content to display" />
      </div>
    )
  }

  // Markdown files
  if (fileInfo.category === "markdown") {
    return (
      <div className={cn("flex flex-col h-full", className)}>
        <div className="flex items-center gap-2 px-3 py-2 bg-muted border-b border-border shrink-0">
          <FileText className="w-4 h-4 text-muted-foreground" />
          <span className="text-sm font-medium truncate max-w-[200px]">{filename}</span>
        </div>
        <ScrollArea className="flex-1">
          <div className="p-4">
            <MarkdownRenderer content={loadedContent} />
          </div>
        </ScrollArea>
      </div>
    )
  }

  // CSV files
  if (fileInfo.category === "csv") {
    return (
      <div className={cn("h-full", className)}>
        <CSVViewer content={loadedContent} filename={filename} />
      </div>
    )
  }

  // Code files (including JSON, YAML, XML)
  if (["code", "json", "yaml", "xml"].includes(fileInfo.category)) {
    return (
      <div className={cn("h-full", className)}>
        <CodeViewer content={loadedContent} filename={filename} language={fileInfo.language} />
      </div>
    )
  }

  // Plain text and unknown files
  return (
    <div className={cn("h-full", className)}>
      <PlainTextViewer content={loadedContent} filename={filename} />
    </div>
  )
}

// Export icon getter for use in file trees
export function getFileIcon(filename: string): React.ComponentType<{ className?: string }> {
  const info = getFileTypeInfo(filename)
  return categoryIcons[info.category] || FileQuestion
}
