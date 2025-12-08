import { Eye } from "lucide-react"

export function PreviewView() {
  return (
    <div className="h-full bg-muted rounded p-4 flex items-center justify-center">
      <div className="text-center text-muted-foreground">
        <Eye className="w-12 h-12 mx-auto mb-2 opacity-50" />
        <p className="text-sm">No preview available</p>
        <p className="text-xs mt-1">Select a file to preview</p>
      </div>
    </div>
  )
}
