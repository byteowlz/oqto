"use client"

import { useCallback, useEffect, useMemo, useState, useRef } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { FileText, Terminal, Eye, Send, RefreshCw, ChevronDown, User, Bot, Clock, ArrowDown, ListTodo, Square, CheckSquare, CircleDot, XCircle, MessageSquare } from "lucide-react"
import { useApp } from "@/components/app-context"
import { FileTreeView } from "@/app/sessions/FileTreeView"
import { TerminalView } from "@/app/sessions/TerminalView"
import { PreviewView } from "@/app/sessions/PreviewView"
import { hasOpencode } from "@/lib/config"
import { MarkdownRenderer, CopyButton } from "@/components/ui/markdown-renderer"
import { ToolCallCard } from "@/components/ui/tool-call-card"
import { cn } from "@/lib/utils"
import {
  fetchMessages,
  sendMessageAsync,
  subscribeToEvents,
  type OpenCodeMessageWithParts,
  type OpenCodePart,
} from "@/lib/opencode-client"

// Todo item structure
interface TodoItem {
  id: string
  content: string
  status: "pending" | "in_progress" | "completed" | "cancelled"
  priority: "high" | "medium" | "low"
}

// Group consecutive messages from the same role
type MessageGroup = {
  role: "user" | "assistant"
  messages: OpenCodeMessageWithParts[]
  startIndex: number
}

function groupMessages(messages: OpenCodeMessageWithParts[]): MessageGroup[] {
  const groups: MessageGroup[] = []
  let currentGroup: MessageGroup | null = null

  messages.forEach((msg, index) => {
    const role = msg.info.role
    if (!currentGroup || currentGroup.role !== role) {
      if (currentGroup) {
        groups.push(currentGroup)
      }
      currentGroup = {
        role,
        messages: [msg],
        startIndex: index,
      }
    } else {
      currentGroup.messages.push(msg)
    }
  })

  if (currentGroup) {
    groups.push(currentGroup)
  }

  return groups
}

export function SessionsApp() {
  const { locale, sessions, selectedSessionId, setSelectedSessionId, refreshSessions } = useApp()
  const [messages, setMessages] = useState<OpenCodeMessageWithParts[]>([])
  const [messageInput, setMessageInput] = useState("")
  const [chatState, setChatState] = useState<"idle" | "sending">("idle")
  const [activeView, setActiveView] = useState<"chat" | "files" | "terminal" | "preview" | "tasks">("chat")
  const [status, setStatus] = useState<string>("")
  const [showScrollToBottom, setShowScrollToBottom] = useState(false)
  const messagesContainerRef = useRef<HTMLDivElement>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)

  const copy = useMemo(
    () => ({
      de: {
        title: "CHAT",
        sessionLabel: "Session",
        refresh: "Aktualisieren",
        noMessages: "Noch keine Nachrichten.",
        inputPlaceholder: "Nachricht eingeben...",
        send: "Senden",
        chat: "Chat",
        files: "Dateien",
        terminal: "Terminal",
        preview: "Vorschau",
        tasks: "Aufgaben",
        noSessions: "Keine Sessions verfugbar",
        statusPrefix: "Aktualisiert",
        configNotice: "NEXT_PUBLIC_OPENCODE_BASE_URL konfigurieren, um das Control Plane zu verbinden.",
        noTasks: "Keine Aufgaben vorhanden.",
      },
      en: {
        title: "CHAT",
        sessionLabel: "Session",
        refresh: "Refresh",
        noMessages: "No messages yet.",
        inputPlaceholder: "Type a message...",
        send: "Send",
        chat: "Chat",
        files: "Files",
        terminal: "Terminal",
        preview: "Preview",
        tasks: "Tasks",
        noSessions: "No sessions available",
        statusPrefix: "Updated",
        configNotice: "Configure NEXT_PUBLIC_OPENCODE_BASE_URL to connect to the control plane.",
        noTasks: "No tasks yet.",
      },
    }),
    [],
  )
  const t = copy[locale]

  const loadMessages = useCallback(async () => {
    if (!hasOpencode || !selectedSessionId) return
    try {
      const data = await fetchMessages(selectedSessionId)
      setMessages(data)
    } catch (err) {
      setStatus((err as Error).message)
    }
  }, [selectedSessionId])

  const scrollToBottom = useCallback((behavior: ScrollBehavior = "smooth") => {
    messagesEndRef.current?.scrollIntoView({ behavior })
  }, [])

  // Handle scroll events to show/hide scroll to bottom button
  const handleScroll = useCallback(() => {
    const container = messagesContainerRef.current
    if (!container) return

    const { scrollTop, scrollHeight, clientHeight } = container
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight
    setShowScrollToBottom(distanceFromBottom > 100)
  }, [])

  useEffect(() => {
    loadMessages()
  }, [loadMessages])

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    const container = messagesContainerRef.current
    if (!container) return

    const { scrollTop, scrollHeight, clientHeight } = container
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight
    
    // Only auto-scroll if user is near the bottom
    if (distanceFromBottom < 200) {
      scrollToBottom("smooth")
    }
  }, [messages, scrollToBottom])

  useEffect(() => {
    if (!hasOpencode) return
    const unsubscribe = subscribeToEvents((event) => {
      const eventType = event.type as string
      if (eventType?.startsWith("session")) {
        // Reset sending state when session becomes idle
        if (eventType === "session.idle" || eventType === "session.status") {
          setChatState("idle")
        }
      }
      if (eventType?.startsWith("message")) {
        loadMessages()
      }
    })
    return unsubscribe
  }, [loadMessages])

  const selectedSession = useMemo(
    () => sessions.find((session) => session.id === selectedSessionId),
    [sessions, selectedSessionId],
  )

  const messageGroups = useMemo(() => groupMessages(messages), [messages])

  // Extract the latest todo list from messages
  const latestTodos = useMemo(() => {
    // Go through all messages in reverse to find the most recent todowrite
    for (let i = messages.length - 1; i >= 0; i--) {
      const msg = messages[i]
      for (let j = msg.parts.length - 1; j >= 0; j--) {
        const part = msg.parts[j]
        if (part.type === "tool" && part.tool?.toLowerCase().includes("todo")) {
          const input = part.state?.input as Record<string, unknown> | undefined
          if (input?.todos && Array.isArray(input.todos)) {
            return input.todos as TodoItem[]
          }
        }
      }
    }
    return []
  }, [messages])

  const handleSend = async () => {
    if (!selectedSessionId || !messageInput.trim()) return
    setChatState("sending")
    setStatus("")
    try {
      // Use async send - the response will come via SSE events
      await sendMessageAsync(selectedSessionId, messageInput.trim())
      setMessageInput("")
      // Scroll to bottom after sending
      setTimeout(() => scrollToBottom(), 100)
    } catch (err) {
      setStatus((err as Error).message)
      setChatState("idle")
    }
    // Don't set idle here - wait for SSE session.idle event
  }

  if (!hasOpencode) {
    return (
      <div className="p-4 md:p-6">
        <div className="p-6 text-sm text-muted-foreground bg-[#161c1a] border border-[#1f2a27]">
          {t.configNotice} <code className="font-semibold">NEXT_PUBLIC_OPENCODE_BASE_URL</code>
        </div>
      </div>
    )
  }

  // Chat content component (reused in both layouts)
  const ChatContent = (
    <div className="flex-1 flex flex-col gap-4 min-h-0">
      <div className="flex flex-wrap items-center gap-3">
        <label className="text-xs uppercase tracking-wide text-muted-foreground">Session</label>
        <select
          className="bg-[#0f1412] border border-[#1f2a27] text-sm text-[#d5f0e4] rounded-md px-3 py-2 outline-none focus:border-[#3ba77c] flex-1 min-w-0 max-w-xs"
          value={selectedSessionId}
          onChange={(e) => setSelectedSessionId(e.target.value)}
        >
          {sessions.map((session) => (
            <option key={session.id} value={session.id}>
              {session.title || session.id}
            </option>
          ))}
          {sessions.length === 0 && <option value="">{t.noSessions}</option>}
        </select>
        <Button variant="outline" size="sm" onClick={refreshSessions} className="gap-2 text-muted-foreground hover:text-foreground">
          <RefreshCw className="w-4 h-4" />
          <span className="hidden sm:inline">{t.refresh}</span>
        </Button>
      </div>

      <div className="relative flex-1 min-h-0">
        <div 
          ref={messagesContainerRef}
          onScroll={handleScroll}
          className="h-full rounded-lg bg-[#0f1412] border border-[#1f2a27] p-4 overflow-y-auto space-y-6"
        >
          {messages.length === 0 && <div className="text-sm text-muted-foreground">{t.noMessages}</div>}
          {messageGroups.map((group, groupIndex) => (
            <MessageGroupCard key={`${group.role}-${group.startIndex}`} group={group} groupIndex={groupIndex} />
          ))}
          <div ref={messagesEndRef} />
        </div>

        {/* Jump to bottom button */}
        {showScrollToBottom && (
          <button
            onClick={() => scrollToBottom()}
            className="absolute bottom-4 right-4 flex items-center gap-2 px-3 py-2 rounded-full bg-[#2d5c47] hover:bg-[#2f6950] text-[#e3f6ed] text-sm font-medium shadow-lg transition-all duration-200 border border-[#3ba77c]/30"
          >
            <ArrowDown className="w-4 h-4" />
            <span className="hidden sm:inline">Jump to bottom</span>
          </button>
        )}
      </div>

      <div className="flex items-center gap-2">
        <Input
          placeholder={t.inputPlaceholder}
          value={messageInput}
          onChange={(e) => setMessageInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              handleSend()
            }
          }}
          className="flex-1 bg-[#111714] border-[#1f2a27] text-[#d5f0e4] placeholder:text-[#6b7974]"
        />
        <Button
          onClick={handleSend}
          disabled={chatState === "sending"}
          className="bg-[#2d5c47] hover:bg-[#2f6950] text-[#e3f6ed]"
        >
          <Send className="w-4 h-4 sm:mr-2" />
          <span className="hidden sm:inline">{t.send}</span>
        </Button>
      </div>
    </div>
  )

  // Tab button component for reuse
  const TabButton = ({ 
    view, 
    icon: Icon, 
    label, 
    badge 
  }: { 
    view: typeof activeView
    icon: React.ComponentType<{ className?: string }>
    label: string
    badge?: number 
  }) => (
    <Button
      variant="ghost"
      size="sm"
      onClick={() => setActiveView(view)}
      className={`flex-1 justify-center rounded-md px-2 relative ${
        activeView === view
          ? "bg-[#1b2d26] text-[#d5f0e4] border border-[#3ba77c]"
          : "text-[#9aa8a3] border border-transparent hover:border-[#264036] hover:bg-[#131a17]"
      }`}
    >
      <Icon className="w-4 h-4" />
      <span className="hidden sm:inline ml-1">{label}</span>
      {badge !== undefined && badge > 0 && (
        <span className="absolute -top-1 -right-1 w-4 h-4 bg-pink-500 text-white text-[10px] rounded-full flex items-center justify-center">
          {badge}
        </span>
      )}
    </Button>
  )

  const incompleteTasks = latestTodos.filter(t => t.status !== "completed" && t.status !== "cancelled").length

  return (
    <div className="flex flex-col h-full min-h-0 p-2 sm:p-4 md:p-6 gap-2 sm:gap-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="min-w-0">
          <h1 className="text-lg sm:text-xl font-semibold text-[#d5f0e4] tracking-wider truncate">{selectedSession?.title || t.title}</h1>
          {selectedSession && (
            <p className="text-xs sm:text-sm text-muted-foreground truncate">
              {t.statusPrefix} {selectedSession.time?.updated ? new Date(selectedSession.time.updated).toLocaleString() : "-"}
            </p>
          )}
        </div>
        {status && <span className="text-xs text-destructive flex-shrink-0">{status}</span>}
      </div>

      {/* Mobile layout: single panel with tabs */}
      <div className="flex-1 min-h-0 flex flex-col lg:hidden">
        {/* Mobile tabs */}
        <div className="flex gap-1 p-2 bg-[#161c1a] border border-[#1f2a27] rounded-t-xl">
          <TabButton view="chat" icon={MessageSquare} label={t.chat} />
          <TabButton view="files" icon={FileText} label={t.files} />
          <TabButton view="terminal" icon={Terminal} label={t.terminal} />
          <TabButton view="tasks" icon={ListTodo} label={t.tasks} badge={incompleteTasks} />
        </div>
        
        {/* Mobile content */}
        <div className="flex-1 min-h-0 bg-[#161c1a] border border-t-0 border-[#1f2a27] rounded-b-xl p-3 sm:p-4 overflow-hidden">
          {activeView === "chat" && ChatContent}
          {activeView === "files" && <FileTreeView />}
          {activeView === "terminal" && <TerminalView />}
          {activeView === "preview" && <PreviewView />}
          {activeView === "tasks" && <TodoListView todos={latestTodos} emptyMessage={t.noTasks} />}
        </div>
      </div>

      {/* Desktop layout: side by side */}
      <div className="hidden lg:flex flex-1 min-h-0 gap-4">
        {/* Chat panel */}
        <div className="flex-1 bg-[#161c1a] border border-[#1f2a27] rounded-xl p-4 xl:p-6 flex flex-col min-h-0">
          {ChatContent}
        </div>

        {/* Sidebar panel */}
        <div className="w-[320px] xl:w-[360px] shrink-0 bg-[#161c1a] border border-[#1f2a27] rounded-xl flex flex-col min-h-0">
          <div className="flex gap-1 p-2 border-b border-[#1f2a27]">
            <TabButton view="files" icon={FileText} label={t.files} />
            <TabButton view="terminal" icon={Terminal} label={t.terminal} />
            <TabButton view="preview" icon={Eye} label={t.preview} />
            <TabButton view="tasks" icon={ListTodo} label={t.tasks} badge={incompleteTasks} />
          </div>
          <div className="flex-1 min-h-0 overflow-hidden">
            {activeView === "files" && <FileTreeView />}
            {activeView === "terminal" && <TerminalView />}
            {activeView === "preview" && <PreviewView />}
            {activeView === "tasks" && <TodoListView todos={latestTodos} emptyMessage={t.noTasks} />}
            {/* If chat is selected on desktop (shouldn't happen normally), show files */}
            {activeView === "chat" && <FileTreeView />}
          </div>
        </div>
      </div>
    </div>
  )
}

function MessageGroupCard({ group, groupIndex }: { group: MessageGroup; groupIndex: number }) {
  const isUser = group.role === "user"
  
  // Get created time from first message
  const firstMessage = group.messages[0]
  const createdAt = firstMessage.info.time?.created ? new Date(firstMessage.info.time.created) : null

  // Get all parts from all messages in order, preserving their sequence
  const allParts = group.messages.flatMap(msg => msg.parts)

  // Group consecutive text parts together, but keep tool and other parts separate
  // This creates "segments" that maintain the original order
  type Segment = 
    | { type: "text"; content: string }
    | { type: "tool"; part: OpenCodePart }
    | { type: "other"; part: OpenCodePart }

  const segments: Segment[] = []
  let currentTextBuffer: string[] = []

  const flushTextBuffer = () => {
    if (currentTextBuffer.length > 0) {
      segments.push({ type: "text", content: currentTextBuffer.join("\n\n") })
      currentTextBuffer = []
    }
  }

  allParts.forEach(part => {
    if (part.type === "text" && typeof part.text === "string") {
      currentTextBuffer.push(part.text)
    } else if (part.type === "tool") {
      flushTextBuffer()
      segments.push({ type: "tool", part })
    } else {
      flushTextBuffer()
      segments.push({ type: "other", part })
    }
  })
  flushTextBuffer()

  // Get all text content for copy button
  const allTextContent = allParts
    .filter((p): p is OpenCodePart & { type: "text"; text: string } => 
      p.type === "text" && typeof p.text === "string"
    )
    .map(p => p.text)
    .join("\n\n")

  return (
    <div
      className={cn(
        "rounded-xl transition-all duration-200",
        isUser 
          ? "ml-8 bg-[#182922] border border-[#2d5c47]" 
          : "mr-8 bg-[#141a18] border border-[#1f2a27]"
      )}
    >
      {/* Header */}
      <div className={cn(
        "flex items-center gap-3 px-4 py-3 border-b",
        isUser ? "border-[#2d5c47]/50" : "border-[#1f2a27]"
      )}>
        <div
          className={cn(
            "p-2 rounded-lg",
            isUser ? "bg-[#2d5c47]/50" : "bg-[#1f2a27]"
          )}
        >
          {isUser ? (
            <User className="w-4 h-4 text-[#6ee7b7]" />
          ) : (
            <Bot className="w-4 h-4 text-[#3ba77c]" />
          )}
        </div>
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-[#d5f0e4]">
              {isUser ? "You" : "Assistant"}
            </span>
            {group.messages.length > 1 && (
              <Badge
                variant="outline"
                className={cn(
                  "text-[10px] px-1.5 py-0",
                  isUser
                    ? "border-[#2d5c47] text-[#6ee7b7]"
                    : "border-[#2a3632] text-[#9aa8a3]"
                )}
              >
                {group.messages.length} messages
              </Badge>
            )}
          </div>
          {createdAt && !isNaN(createdAt.getTime()) && (
            <div className="text-xs text-[#6b7974] flex items-center gap-1 mt-0.5">
              <Clock className="w-3 h-3" />
              {createdAt.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
            </div>
          )}
        </div>
        
        {/* Copy button for entire message content */}
        {allTextContent && (
          <CopyButton text={allTextContent} className="opacity-0 group-hover:opacity-100" />
        )}
      </div>

      {/* Content - render segments in order */}
      <div className="px-4 py-3 group space-y-3">
        {segments.length === 0 && (
          <span className="text-[#6b7974] italic text-sm">No content</span>
        )}
        
        {segments.map((segment, idx) => {
          if (segment.type === "text") {
            return (
              <div key={`text-${idx}`} className="relative group/text">
                <MarkdownRenderer 
                  content={segment.content} 
                  className="text-sm text-[#d5f0e4] leading-relaxed pr-8"
                />
                {/* Floating copy button - positioned to not overlap text */}
                <div className="absolute top-0 right-0 opacity-0 group-hover/text:opacity-100 transition-opacity">
                  <CopyButton text={segment.content} />
                </div>
              </div>
            )
          }
          
          if (segment.type === "tool") {
            return (
              <ToolCallCard 
                key={segment.part.id || `tool-${idx}`} 
                part={segment.part} 
                defaultCollapsed={true}
                hideTodoTools={true}
              />
            )
          }
          
          if (segment.type === "other") {
            return (
              <OtherPartCard 
                key={segment.part.id || `other-${idx}`} 
                part={segment.part} 
              />
            )
          }
          
          return null
        })}
      </div>
    </div>
  )
}

function OtherPartCard({ part }: { part: OpenCodePart }) {
  const [isOpen, setIsOpen] = useState(false)
  
  const getPartLabel = () => {
    switch (part.type) {
      case "reasoning": return "Reasoning"
      case "file": return "File"
      case "snapshot": return "Snapshot"
      case "patch": return "Patch"
      case "agent": return "Agent"
      case "step-start": return "Step Start"
      case "step-finish": return "Step Finish"
      case "retry": return "Retry"
      case "compaction": return "Compaction"
      case "subtask": return "Subtask"
      default: return part.type
    }
  }

  const content = part.text || (part.metadata ? JSON.stringify(part.metadata, null, 2) : null)
  if (!content) return null

  return (
    <div className="rounded-lg border border-[#2a3632] bg-[#141a18] overflow-hidden">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-[#1a221f] transition-colors"
      >
        <ChevronDown 
          className={cn(
            "w-4 h-4 text-[#6b7974] transition-transform",
            isOpen && "rotate-180"
          )}
        />
        <span className="text-xs uppercase tracking-wide text-[#6b7974]">{getPartLabel()}</span>
      </button>
      
      {isOpen && (
        <div className="px-3 pb-3 border-t border-[#2a3632]">
          <pre className="text-xs text-[#9aa8a3] mt-2 whitespace-pre-wrap overflow-x-auto">
            {content}
          </pre>
        </div>
      )}
    </div>
  )
}

function getPriorityColor(priority: string) {
  switch (priority) {
    case "high": return "text-red-400"
    case "medium": return "text-yellow-400"
    case "low": return "text-[#6b7974]"
    default: return "text-[#9aa8a3]"
  }
}

function TodoListView({ todos, emptyMessage }: { todos: TodoItem[]; emptyMessage: string }) {
  // Group todos by status for summary
  const summary = useMemo(() => {
    const pending = todos.filter(t => t.status === "pending").length
    const inProgress = todos.filter(t => t.status === "in_progress").length
    const completed = todos.filter(t => t.status === "completed").length
    const cancelled = todos.filter(t => t.status === "cancelled").length
    return { pending, inProgress, completed, cancelled, total: todos.length }
  }, [todos])

  if (todos.length === 0) {
    return (
      <div className="flex items-center justify-center h-full p-4">
        <div className="text-center">
          <ListTodo className="w-12 h-12 text-[#2a3632] mx-auto mb-3" />
          <p className="text-sm text-[#6b7974]">{emptyMessage}</p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {/* Summary header */}
      <div className="p-3 border-b border-[#1f2a27] bg-[#0f1412]">
        <div className="flex items-center justify-between text-xs">
          <span className="text-[#6b7974]">{summary.total} tasks</span>
          <div className="flex items-center gap-3">
            {summary.inProgress > 0 && (
              <span className="flex items-center gap-1 text-[#3ba77c]">
                <CircleDot className="w-3 h-3" />
                {summary.inProgress}
              </span>
            )}
            {summary.pending > 0 && (
              <span className="flex items-center gap-1 text-[#6b7974]">
                <Square className="w-3 h-3" />
                {summary.pending}
              </span>
            )}
            {summary.completed > 0 && (
              <span className="flex items-center gap-1 text-[#3ba77c]">
                <CheckSquare className="w-3 h-3" />
                {summary.completed}
              </span>
            )}
          </div>
        </div>
      </div>

      {/* Todo list */}
      <div className="flex-1 overflow-y-auto p-2 space-y-1">
        {todos.map((todo, idx) => (
          <div 
            key={todo.id || idx} 
            className={cn(
              "flex items-start gap-2 p-2 rounded-lg transition-colors",
              todo.status === "in_progress" && "bg-[#182922] border border-[#2d5c47]",
              todo.status === "completed" && "opacity-50",
              todo.status === "cancelled" && "opacity-40",
              todo.status === "pending" && "bg-[#141a18] border border-[#1f2a27]"
            )}
          >
            {/* Status icon */}
            <div className="flex-shrink-0 mt-0.5">
              {todo.status === "completed" ? (
                <CheckSquare className="w-4 h-4 text-[#3ba77c]" />
              ) : todo.status === "in_progress" ? (
                <CircleDot className="w-4 h-4 text-[#3ba77c] animate-pulse" />
              ) : todo.status === "cancelled" ? (
                <XCircle className="w-4 h-4 text-[#6b7974]" />
              ) : (
                <Square className="w-4 h-4 text-[#6b7974]" />
              )}
            </div>
            
            {/* Content */}
            <div className="flex-1 min-w-0">
              <p className={cn(
                "text-sm leading-relaxed",
                todo.status === "completed" ? "text-[#6b7974] line-through" : "text-[#d5f0e4]",
                todo.status === "cancelled" && "line-through"
              )}>
                {todo.content}
              </p>
            </div>
            
            {/* Priority badge */}
            {todo.priority && (
              <span className={cn(
                "text-[10px] uppercase tracking-wide flex-shrink-0 px-1.5 py-0.5 rounded",
                todo.priority === "high" && "bg-red-400/10 text-red-400",
                todo.priority === "medium" && "bg-yellow-400/10 text-yellow-400",
                todo.priority === "low" && "bg-[#2a3632] text-[#6b7974]"
              )}>
                {todo.priority}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}

export default SessionsApp
