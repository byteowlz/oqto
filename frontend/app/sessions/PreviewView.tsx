import { Eye } from "lucide-react"
import { FilePreview } from "@/components/ui/file-preview"
import { cn } from "@/lib/utils"

interface PreviewViewProps {
  filename?: string
  content?: string
  contentUrl?: string
  className?: string
}

export function PreviewView({ filename, content, contentUrl, className }: PreviewViewProps) {
  // If no file is selected, show empty state
  if (!filename && !content && !contentUrl) {
    return (
      <div className={cn("h-full bg-muted rounded flex items-center justify-center", className)}>
        <div className="text-center text-muted-foreground">
          <Eye className="w-12 h-12 mx-auto mb-2 opacity-50" />
          <p className="text-sm">No preview available</p>
          <p className="text-xs mt-1">Select a file to preview</p>
        </div>
      </div>
    )
  }

  return (
    <div className={cn("h-full rounded overflow-hidden border border-border", className)}>
      <FilePreview filename={filename || "file"} content={content} contentUrl={contentUrl} />
    </div>
  )
}
