"use client"

import { useState, useCallback } from "react"
import { FileText, Download, ExternalLink, ZoomIn, ZoomOut, Loader2 } from "lucide-react"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"

interface PDFViewerProps {
  src: string
  filename?: string
  className?: string
}

export function PDFViewer({ src, filename, className }: PDFViewerProps) {
  const [isLoading, setIsLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [zoom, setZoom] = useState(100)

  const handleLoad = useCallback(() => {
    setIsLoading(false)
    setError(null)
  }, [])

  const handleError = useCallback(() => {
    setIsLoading(false)
    setError("Failed to load PDF. Try opening in a new tab.")
  }, [])

  const handleDownload = useCallback(() => {
    const link = document.createElement("a")
    link.href = src
    link.download = filename || "document.pdf"
    link.click()
  }, [src, filename])

  const handleOpenInNewTab = useCallback(() => {
    window.open(src, "_blank")
  }, [src])

  const handleZoomIn = useCallback(() => {
    setZoom((z) => Math.min(z + 25, 200))
  }, [])

  const handleZoomOut = useCallback(() => {
    setZoom((z) => Math.max(z - 25, 50))
  }, [])

  // Construct the PDF URL with zoom parameter for the browser's built-in viewer
  const pdfUrl = `${src}#zoom=${zoom}`

  return (
    <div className={cn("flex flex-col h-full", className)}>
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-2 bg-muted border-b border-border shrink-0">
        <div className="flex items-center gap-2">
          <FileText className="w-4 h-4 text-muted-foreground" />
          {filename && <span className="text-sm font-medium truncate max-w-[200px]">{filename}</span>}
          <span className="text-xs text-muted-foreground">{zoom}%</span>
        </div>
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="sm" onClick={handleZoomOut} title="Zoom out" disabled={zoom <= 50}>
            <ZoomOut className="w-4 h-4" />
          </Button>
          <Button variant="ghost" size="sm" onClick={handleZoomIn} title="Zoom in" disabled={zoom >= 200}>
            <ZoomIn className="w-4 h-4" />
          </Button>
          <Button variant="ghost" size="sm" onClick={handleOpenInNewTab} title="Open in new tab">
            <ExternalLink className="w-4 h-4" />
          </Button>
          <Button variant="ghost" size="sm" onClick={handleDownload} title="Download">
            <Download className="w-4 h-4" />
          </Button>
        </div>
      </div>

      {/* PDF container */}
      <div className="flex-1 overflow-hidden relative bg-muted/30">
        {isLoading && (
          <div className="absolute inset-0 flex items-center justify-center bg-background/80 z-10">
            <div className="flex flex-col items-center gap-2">
              <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
              <p className="text-sm text-muted-foreground">Loading PDF...</p>
            </div>
          </div>
        )}
        
        {error ? (
          <div className="flex flex-col items-center justify-center h-full gap-4 p-4">
            <FileText className="w-12 h-12 text-muted-foreground opacity-50" />
            <p className="text-sm text-muted-foreground text-center">{error}</p>
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={handleOpenInNewTab}>
                <ExternalLink className="w-4 h-4 mr-2" />
                Open in new tab
              </Button>
              <Button variant="outline" size="sm" onClick={handleDownload}>
                <Download className="w-4 h-4 mr-2" />
                Download
              </Button>
            </div>
          </div>
        ) : (
          <iframe
            src={pdfUrl}
            className="w-full h-full border-0"
            title={filename || "PDF Preview"}
            onLoad={handleLoad}
            onError={handleError}
          />
        )}
      </div>
    </div>
  )
}
