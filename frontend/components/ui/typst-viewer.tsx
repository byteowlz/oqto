"use client"

import { useCallback, useState } from "react"
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter"
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism"
import { Check, Copy, FileText } from "lucide-react"
import { cn } from "@/lib/utils"

interface TypstViewerProps {
  content: string
  filename?: string
  className?: string
}

function CopyButton({ text, className }: { text: string; className?: string }) {
  const [copied, setCopied] = useState(false)

  const handleCopy = useCallback(async () => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }, [text])

  return (
    <button
      onClick={handleCopy}
      className={cn(
        "p-1.5 rounded-md transition-colors",
        "text-muted-foreground hover:text-foreground hover:bg-muted",
        className,
      )}
      title="Copy to clipboard"
    >
      {copied ? <Check className="w-4 h-4 text-green-500" /> : <Copy className="w-4 h-4" />}
    </button>
  )
}

export function TypstViewer({ content, filename, className }: TypstViewerProps) {
  // Use latex for syntax highlighting as it's the closest match to Typst
  const highlightLanguage = "latex"

  return (
    <div className={cn("flex flex-col h-full rounded-lg overflow-hidden border border-border", className)}>
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 bg-muted border-b border-border shrink-0">
        <div className="flex items-center gap-2">
          <FileText className="w-4 h-4 text-muted-foreground" />
          {filename && <span className="text-sm font-medium truncate max-w-[200px]">{filename}</span>}
          <span className="text-xs text-muted-foreground font-mono bg-primary/10 px-1.5 py-0.5 rounded">typst</span>
        </div>
        <CopyButton text={content} />
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto">
        <SyntaxHighlighter
          style={oneDark as Record<string, React.CSSProperties>}
          language={highlightLanguage}
          showLineNumbers
          wrapLines
          wrapLongLines
          customStyle={{
            margin: 0,
            padding: "1rem",
            background: "var(--background)",
            fontSize: "0.875rem",
            minHeight: "100%",
          }}
          lineNumberStyle={{
            minWidth: "3em",
            paddingRight: "1em",
            color: "var(--muted-foreground)",
            opacity: 0.5,
          }}
        >
          {content}
        </SyntaxHighlighter>
      </div>
    </div>
  )
}
