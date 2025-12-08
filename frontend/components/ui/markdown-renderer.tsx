"use client"

import { useCallback, useState, memo } from "react"
import ReactMarkdown, { Components } from "react-markdown"
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter"
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism"
import remarkGfm from "remark-gfm"
import { Check, Copy } from "lucide-react"
import { cn } from "@/lib/utils"

interface MarkdownRendererProps {
  content: string
  className?: string
}

const CopyButton = memo(function CopyButton({ text, className }: { text: string; className?: string }) {
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
        "text-[#6b7974] hover:text-[#d5f0e4] hover:bg-[#2a3632]",
        className
      )}
      title="Copy to clipboard"
    >
      {copied ? (
        <Check className="w-4 h-4 text-[#3ba77c]" />
      ) : (
        <Copy className="w-4 h-4" />
      )}
    </button>
  )
})

// Memoized code block component to prevent expensive re-renders
const CodeBlock = memo(function CodeBlock({ 
  className, 
  children 
}: { 
  className?: string
  children: React.ReactNode 
}) {
  const match = /language-(\w+)/.exec(className || "")
  const codeString = String(children).replace(/\n$/, "")
  const isInline = !match && !codeString.includes("\n")

  if (isInline) {
    return (
      <code className="px-1.5 py-0.5 rounded bg-[#1a221f] text-[#6ee7b7] text-sm font-mono">
        {children}
      </code>
    )
  }

  return (
    <div className="relative group my-3 rounded-lg overflow-hidden border border-[#2a3632]">
      <div className="flex items-center justify-between px-3 py-2 bg-[#1a221f] border-b border-[#2a3632]">
        <span className="text-xs text-[#6b7974] font-mono">
          {match ? match[1] : "plaintext"}
        </span>
        <CopyButton text={codeString} />
      </div>
      <SyntaxHighlighter
        style={oneDark as Record<string, React.CSSProperties>}
        language={match ? match[1] : "text"}
        PreTag="div"
        customStyle={{
          margin: 0,
          padding: "1rem",
          background: "#0f1412",
          fontSize: "0.875rem",
        }}
      >
        {codeString}
      </SyntaxHighlighter>
    </div>
  )
})

// Static components object - defined once, never recreated
const markdownComponents: Components = {
  code({ className, children }) {
    return <CodeBlock className={className}>{children}</CodeBlock>
  },
  p({ children }) {
    return <p className="mb-3 last:mb-0 leading-relaxed">{children}</p>
  },
  h1({ children }) {
    return <h1 className="text-xl font-bold mb-3 mt-4 first:mt-0 text-[#d5f0e4]">{children}</h1>
  },
  h2({ children }) {
    return <h2 className="text-lg font-bold mb-2 mt-3 first:mt-0 text-[#d5f0e4]">{children}</h2>
  },
  h3({ children }) {
    return <h3 className="text-base font-semibold mb-2 mt-3 first:mt-0 text-[#d5f0e4]">{children}</h3>
  },
  h4({ children }) {
    return <h4 className="text-sm font-semibold mb-2 mt-2 first:mt-0 text-[#d5f0e4]">{children}</h4>
  },
  ul({ children }) {
    return <ul className="list-disc list-inside mb-3 space-y-1 pl-2">{children}</ul>
  },
  ol({ children }) {
    return <ol className="list-decimal list-inside mb-3 space-y-1 pl-2">{children}</ol>
  },
  li({ children }) {
    return <li className="text-[#d5f0e4]">{children}</li>
  },
  blockquote({ children }) {
    return (
      <blockquote className="border-l-2 border-[#3ba77c] pl-4 py-1 my-3 text-[#9aa8a3] italic">
        {children}
      </blockquote>
    )
  },
  a({ href, children }) {
    return (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className="text-[#3ba77c] hover:text-[#6ee7b7] underline underline-offset-2"
      >
        {children}
      </a>
    )
  },
  table({ children }) {
    return (
      <div className="overflow-x-auto my-3">
        <table className="min-w-full border border-[#2a3632] rounded-lg overflow-hidden">
          {children}
        </table>
      </div>
    )
  },
  thead({ children }) {
    return <thead className="bg-[#1a221f]">{children}</thead>
  },
  tbody({ children }) {
    return <tbody className="divide-y divide-[#2a3632]">{children}</tbody>
  },
  tr({ children }) {
    return <tr className="divide-x divide-[#2a3632]">{children}</tr>
  },
  th({ children }) {
    return <th className="px-3 py-2 text-left text-sm font-semibold text-[#d5f0e4]">{children}</th>
  },
  td({ children }) {
    return <td className="px-3 py-2 text-sm text-[#9aa8a3]">{children}</td>
  },
  hr() {
    return <hr className="my-4 border-[#2a3632]" />
  },
  strong({ children }) {
    return <strong className="font-semibold text-[#d5f0e4]">{children}</strong>
  },
  em({ children }) {
    return <em className="italic text-[#9aa8a3]">{children}</em>
  },
}

// remarkPlugins array - defined once
const remarkPlugins = [remarkGfm]

export const MarkdownRenderer = memo(function MarkdownRenderer({ content, className }: MarkdownRendererProps) {
  return (
    <div className={cn("markdown-content", className)}>
      <ReactMarkdown
        remarkPlugins={remarkPlugins}
        components={markdownComponents}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
})

export { CopyButton }
