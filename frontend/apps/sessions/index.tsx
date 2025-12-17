"use client"

import { useCallback, useEffect, useMemo, useState, useRef, memo } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { FileText, Terminal, Eye, Send, ChevronDown, User, Bot, Clock, ArrowDown, ListTodo, Square, CheckSquare, CircleDot, XCircle, MessageSquare, Loader2 } from "lucide-react"
import { useApp } from "@/components/app-context"
import { FileTreeView, type FileTreeState, initialFileTreeState } from "@/app/sessions/FileTreeView"
import { TerminalView } from "@/app/sessions/TerminalView"
import { PreviewView } from "@/app/sessions/PreviewView"
import { useIsMobile } from "@/hooks/use-mobile"
import { MarkdownRenderer, CopyButton } from "@/components/ui/markdown-renderer"
import { ToolCallCard } from "@/components/ui/tool-call-card"
import { cn } from "@/lib/utils"
import {
  fetchMessages,
  sendMessageAsync,
  subscribeToEvents,
  invalidateMessageCache,
  type OpenCodeMessageWithParts,
  type OpenCodePart,
} from "@/lib/opencode-client"
import { controlPlaneDirectBaseUrl } from "@/lib/control-plane-client"

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

type ActiveView = "chat" | "files" | "terminal" | "preview" | "tasks"

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

function TabButton({
  activeView,
  onSelect,
  view,
  icon: Icon,
  label,
  badge,
  hideLabel,
}: {
  activeView: ActiveView
  onSelect: (view: ActiveView) => void
  view: ActiveView
  icon: React.ComponentType<{ className?: string }>
  label: string
  badge?: number
  hideLabel?: boolean
}) {
  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={() => onSelect(view)}
      className={cn(
        "flex-1 justify-center px-2 relative",
        activeView === view
          ? "bg-primary/15 text-foreground border border-primary"
          : "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50"
      )}
      title={label}
    >
      <Icon className="w-4 h-4" />
      {!hideLabel && <span className="hidden sm:inline ml-1">{label}</span>}
      {badge !== undefined && badge > 0 && (
        <span className="absolute -top-1 -right-1 w-4 h-4 bg-pink-500 text-white text-[10px] rounded-full flex items-center justify-center">
          {badge}
        </span>
      )}
    </Button>
  )
}

export function SessionsApp() {
  const {
    locale,
    workspaceSessions,
    selectedWorkspaceSessionId,
    opencodeBaseUrl,
    selectedChatSessionId,
    selectedChatSession,
    refreshOpencodeSessions,
    authToken,
  } = useApp()
  const [messages, setMessages] = useState<OpenCodeMessageWithParts[]>([])
  const [messageInput, setMessageInput] = useState("")
  const [chatState, setChatState] = useState<"idle" | "sending">("idle")
  const [activeView, setActiveView] = useState<ActiveView>("chat")
  const [status, setStatus] = useState<string>("")
  const [showScrollToBottom, setShowScrollToBottom] = useState(false)
  const [previewFilePath, setPreviewFilePath] = useState<string | null>(null)
  const [fileTreeState, setFileTreeState] = useState<FileTreeState>(initialFileTreeState)
  const messagesContainerRef = useRef<HTMLDivElement>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  
  // Track if we're on mobile layout (below lg breakpoint = 1024px)
  const isMobileLayout = useIsMobile()
  
  // Handler for previewing a file from FileTreeView
  const handlePreviewFile = useCallback((filePath: string) => {
    setPreviewFilePath(filePath)
    setActiveView("preview")
  }, [])
  
  // Handler for file tree state changes (for persistence)
  const handleFileTreeStateChange = useCallback((newState: FileTreeState) => {
    setFileTreeState(newState)
  }, [])

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
        configNotice: "Control Plane Backend starten, um Sessions zu laden.",
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
        configNotice: "Start the control plane backend to load sessions.",
        noTasks: "No tasks yet.",
      },
    }),
    [],
  )
  const t = copy[locale]

  const loadMessages = useCallback(async () => {
    if (!opencodeBaseUrl || !selectedChatSessionId) return
    try {
      const data = await fetchMessages(opencodeBaseUrl, selectedChatSessionId)
      setMessages(data)
    } catch (err) {
      setStatus((err as Error).message)
    }
  }, [opencodeBaseUrl, selectedChatSessionId])

  const [eventsTransportMode, setEventsTransportMode] = useState<"sse" | "polling">("sse")
  const messageRefreshStateRef = useRef<{
    timer: ReturnType<typeof setTimeout> | null
    inFlight: boolean
    pending: boolean
    lastStartAt: number
  }>({
    timer: null,
    inFlight: false,
    pending: false,
    lastStartAt: 0,
  })

  const requestMessageRefresh = useCallback(
    (maxFrequencyMs: number) => {
      const state = messageRefreshStateRef.current
      state.pending = true

      if (state.inFlight) return

      const run = async () => {
        const current = messageRefreshStateRef.current
        if (current.timer) {
          clearTimeout(current.timer)
          current.timer = null
        }
        if (current.inFlight || !current.pending) return

        current.pending = false
        current.inFlight = true
        current.lastStartAt = Date.now()
        try {
          await loadMessages()
        } finally {
          current.inFlight = false
          if (current.pending) requestMessageRefresh(maxFrequencyMs)
        }
      }

      const elapsed = Date.now() - state.lastStartAt
      const wait = Math.max(0, maxFrequencyMs - elapsed)

      if (wait === 0) {
        void run()
        return
      }

      if (!state.timer) {
        state.timer = setTimeout(() => void run(), wait)
      }
    },
    [loadMessages],
  )

  const scrollToBottom = useCallback((behavior: ScrollBehavior = "smooth") => {
    messagesEndRef.current?.scrollIntoView({ behavior })
  }, [])

  

  useEffect(() => {
    loadMessages()
  }, [loadMessages])

  // Handle scroll events to show/hide scroll to bottom button
  const handleScroll = useCallback(() => {
    const container = messagesContainerRef.current
    if (!container) return

    const { scrollTop, scrollHeight, clientHeight } = container
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight
    setShowScrollToBottom(distanceFromBottom > 100)
  }, [])

  // Check scroll position when messages change
  useEffect(() => {
    handleScroll()
  }, [messages, handleScroll])

  useEffect(() => {
    if (!opencodeBaseUrl) return
    const unsubscribe = subscribeToEvents(
      opencodeBaseUrl, 
      (event) => {
        const eventType = event.type as string
        
        if (eventType === "transport.mode") {
          const props = event.properties as { mode?: "sse" | "polling" } | null
          if (props?.mode) setEventsTransportMode(props.mode)
          return
        }

        if (eventType === "session.idle") {
          setChatState("idle")
          // Invalidate cache and force refresh on idle
          if (opencodeBaseUrl && selectedChatSessionId) {
            invalidateMessageCache(opencodeBaseUrl, selectedChatSessionId)
          }
          loadMessages()
          refreshOpencodeSessions()
        } else if (eventType === "session.busy") {
          setChatState("sending")
        }
        // Refresh messages on any message event
        if (eventType?.startsWith("message")) {
          // Invalidate cache when messages change
          if (opencodeBaseUrl && selectedChatSessionId) {
            invalidateMessageCache(opencodeBaseUrl, selectedChatSessionId)
          }
          // Coalesce refreshes to avoid hammering the server during streaming updates.
          requestMessageRefresh(1000)
        }
      },
      authToken,
      controlPlaneDirectBaseUrl(),
    )
    return unsubscribe
  }, [authToken, opencodeBaseUrl, selectedChatSessionId, loadMessages, refreshOpencodeSessions, requestMessageRefresh])

  // Poll for message updates while assistant is working.
  // This runs regardless of SSE status since SSE is unreliable through the proxy.
  useEffect(() => {
    if (chatState !== "sending" || !opencodeBaseUrl || !selectedChatSessionId) return

    let active = true
    let delayMs = 1000
    let timer: number | null = null

    const tick = async () => {
      if (!active) return
      try {
        // Invalidate cache and fetch fresh data
        invalidateMessageCache(opencodeBaseUrl, selectedChatSessionId)
        const freshMessages = await fetchMessages(opencodeBaseUrl, selectedChatSessionId, { skipCache: true })
        if (!active) return
        
        setMessages(freshMessages)
        
        // Check if the latest assistant message is completed
        const lastMessage = freshMessages[freshMessages.length - 1]
        if (lastMessage?.info.role === "assistant") {
          const assistantInfo = lastMessage.info as { time?: { completed?: number } }
          if (assistantInfo.time?.completed) {
            // Assistant is done, set to idle
            setChatState("idle")
            refreshOpencodeSessions()
            return // Stop polling
          }
        }
      } catch {
        // Ignore errors, will retry
      }
      
      if (!active) return
      delayMs = Math.min(3000, Math.round(delayMs * 1.1))
      timer = window.setTimeout(() => void tick(), delayMs) as unknown as number
    }

    // Start polling immediately
    void tick()

    return () => {
      active = false
      if (timer) window.clearTimeout(timer)
    }
  }, [chatState, opencodeBaseUrl, selectedChatSessionId, refreshOpencodeSessions])

  useEffect(() => {
    return () => {
      const state = messageRefreshStateRef.current
      if (state.timer) {
        clearTimeout(state.timer)
        state.timer = null
      }
    }
  }, [])

  

  const selectedSession = useMemo(() => {
    if (!selectedWorkspaceSessionId) return undefined
    return workspaceSessions.find((session) => session.id === selectedWorkspaceSessionId)
  }, [workspaceSessions, selectedWorkspaceSessionId])

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
    if (!opencodeBaseUrl || !selectedChatSessionId || !messageInput.trim()) return
    
    const messageText = messageInput.trim()
    
    // Optimistic update - show user message immediately
    const optimisticMessage: OpenCodeMessageWithParts = {
      info: {
        id: `temp-${Date.now()}`,
        sessionID: selectedChatSessionId,
        role: "user",
        time: { created: Date.now() },
      },
      parts: [{ id: `temp-part-${Date.now()}`, sessionID: selectedChatSessionId, messageID: `temp-${Date.now()}`, type: "text", text: messageText }],
    }
    
    setMessages((prev) => [...prev, optimisticMessage])
    setMessageInput("")
    setChatState("sending")
    setStatus("")
    
    // Scroll to bottom immediately
    setTimeout(() => scrollToBottom(), 50)
    
    try {
      // Use async send - the response will come via SSE events
      await sendMessageAsync(opencodeBaseUrl, selectedChatSessionId, messageText)
      // Invalidate cache and refresh messages to get the real message IDs
      invalidateMessageCache(opencodeBaseUrl, selectedChatSessionId)
      loadMessages()
    } catch (err) {
      setStatus((err as Error).message)
      setChatState("idle")
      // Remove optimistic message on error
      setMessages((prev) => prev.filter((m) => !m.info.id.startsWith("temp-")))
    }
    // Don't set idle here - wait for SSE session.idle event
  }

  if (workspaceSessions.length === 0) {
    return (
      <div className="p-4 md:p-6">
        <div className="p-6 text-sm text-muted-foreground bg-card border border-border">
          {t.configNotice}
        </div>
      </div>
    )
  }

  // Chat content component (reused in both layouts)
  const ChatContent = (
    <div className="flex-1 flex flex-col gap-4 min-h-0">
      <div className="relative flex-1 min-h-0">
        <div 
          ref={messagesContainerRef}
          onScroll={handleScroll}
          className="h-full bg-muted/30 border border-border p-4 overflow-y-auto space-y-6 scrollbar-hide"
        >
          {messages.length === 0 && <div className="text-sm text-muted-foreground">{t.noMessages}</div>}
          {messageGroups.map((group) => (
            <MessageGroupCard key={`${group.role}-${group.startIndex}`} group={group} />
          ))}
          <div ref={messagesEndRef} />
        </div>

        {/* Jump to bottom button */}
        {showScrollToBottom && (
          <button
            onClick={() => scrollToBottom()}
            className="absolute bottom-4 right-4 z-50 flex items-center gap-2 px-3 py-2 bg-primary hover:bg-primary/90 text-primary-foreground text-sm font-medium shadow-lg"
          >
            <ArrowDown className="w-4 h-4" />
            Jump to bottom
          </button>
        )}
      </div>

      <div className="flex items-center gap-2">
        <Input
          placeholder={t.inputPlaceholder}
          value={messageInput}
          onChange={(e) => setMessageInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault()
              handleSend()
            }
          }}
          className="flex-1 bg-background border-border text-foreground placeholder:text-muted-foreground"
        />
        <Button
          onClick={handleSend}
          disabled={chatState === "sending"}
          className="bg-primary hover:bg-primary/90 text-primary-foreground"
        >
          <Send className="w-4 h-4 sm:mr-2" />
          <span className="hidden sm:inline">{t.send}</span>
        </Button>
      </div>
    </div>
  )

  const incompleteTasks = latestTodos.filter(t => t.status !== "completed" && t.status !== "cancelled").length

  return (
    <div className="flex flex-col h-full min-h-0 p-2 sm:p-4 md:p-6 gap-2 sm:gap-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="min-w-0">
          <h1 className="text-lg sm:text-xl font-semibold text-foreground tracking-wider truncate">
            {selectedChatSession?.title || t.title}
          </h1>
          <p className="text-xs text-muted-foreground truncate">
            {selectedChatSession?.id || selectedChatSessionId}
          </p>
        </div>
        {status && <span className="text-xs text-destructive flex-shrink-0">{status}</span>}
      </div>

      {/* Mobile layout: single panel with tabs */}
      <div className="flex-1 min-h-0 flex flex-col lg:hidden">
        {/* Mobile tabs */}
        <div className="flex gap-1 p-2 bg-card border border-border rounded-t-xl">
          <TabButton activeView={activeView} onSelect={setActiveView} view="chat" icon={MessageSquare} label={t.chat} />
          <TabButton activeView={activeView} onSelect={setActiveView} view="tasks" icon={ListTodo} label={t.tasks} badge={incompleteTasks} />
          <TabButton activeView={activeView} onSelect={setActiveView} view="files" icon={FileText} label={t.files} />
          <TabButton activeView={activeView} onSelect={setActiveView} view="preview" icon={Eye} label={t.preview} />
          <TabButton activeView={activeView} onSelect={setActiveView} view="terminal" icon={Terminal} label={t.terminal} />
        </div>
        
        {/* Mobile content */}
        <div className="flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-3 sm:p-4 overflow-hidden">
          {activeView === "chat" && ChatContent}
          {activeView === "files" && <FileTreeView onPreviewFile={handlePreviewFile} state={fileTreeState} onStateChange={handleFileTreeStateChange} />}
          {activeView === "preview" && <PreviewView filePath={previewFilePath} />}
          {activeView === "tasks" && <TodoListView todos={latestTodos} emptyMessage={t.noTasks} />}
          {/* Terminal only rendered in mobile layout when isMobileLayout is true */}
          {isMobileLayout && (
            <div className={activeView === "terminal" ? "h-full" : "hidden"}>
              <TerminalView sessionId={selectedWorkspaceSessionId} />
            </div>
          )}
        </div>
      </div>

      {/* Desktop layout: side by side */}
      <div className="hidden lg:flex flex-1 min-h-0 gap-4">
        {/* Chat panel */}
        <div className="flex-[3] min-w-0 bg-card border border-border p-4 xl:p-6 flex flex-col min-h-0">
          {ChatContent}
        </div>

        {/* Sidebar panel */}
        <div className="flex-[2] min-w-[320px] max-w-[420px] bg-card border border-border flex flex-col min-h-0">
          <div className="flex gap-1 p-2 border-b border-border">
            <TabButton activeView={activeView} onSelect={setActiveView} view="tasks" icon={ListTodo} label={t.tasks} badge={incompleteTasks} hideLabel />
            <TabButton activeView={activeView} onSelect={setActiveView} view="files" icon={FileText} label={t.files} hideLabel />
            <TabButton activeView={activeView} onSelect={setActiveView} view="preview" icon={Eye} label={t.preview} hideLabel />
            <TabButton activeView={activeView} onSelect={setActiveView} view="terminal" icon={Terminal} label={t.terminal} hideLabel />
          </div>
          <div className="flex-1 min-h-0 overflow-hidden">
            {activeView === "files" && <FileTreeView onPreviewFile={handlePreviewFile} state={fileTreeState} onStateChange={handleFileTreeStateChange} />}
            {activeView === "preview" && <PreviewView filePath={previewFilePath} />}
            {activeView === "tasks" && <TodoListView todos={latestTodos} emptyMessage={t.noTasks} />}
            {activeView === "chat" && <TodoListView todos={latestTodos} emptyMessage={t.noTasks} />}
            {/* Terminal only rendered in desktop layout when isMobileLayout is false */}
            {!isMobileLayout && (
              <div className={activeView === "terminal" ? "h-full" : "hidden"}>
                <TerminalView sessionId={selectedWorkspaceSessionId} />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

const MessageGroupCard = memo(function MessageGroupCard({ group }: { group: MessageGroup }) {
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
        "transition-all duration-200",
        isUser 
          ? "ml-8 bg-primary/10 border border-primary/30" 
          : "mr-8 bg-muted/50 border border-border"
      )}
    >
      {/* Header */}
      <div className={cn(
        "flex items-center gap-3 px-4 py-3 border-b",
        isUser ? "border-primary/20" : "border-border"
      )}>
        <div
          className={cn(
            "p-2",
            isUser ? "bg-primary/20" : "bg-muted"
          )}
        >
          {isUser ? (
            <User className="w-4 h-4 text-primary" />
          ) : (
            <Bot className="w-4 h-4 text-primary" />
          )}
        </div>
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-foreground">
              {isUser ? "You" : "Assistant"}
            </span>
            {group.messages.length > 1 && (
              <Badge
                variant="outline"
                className={cn(
                  "text-[10px] px-1.5 py-0",
                  isUser
                    ? "border-primary/30 text-primary"
                    : "border-border text-muted-foreground"
                )}
              >
                {group.messages.length} messages
              </Badge>
            )}
          </div>
          {createdAt && !isNaN(createdAt.getTime()) && (
            <div className="text-xs text-muted-foreground flex items-center gap-1 mt-0.5">
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
        {segments.length === 0 && !isUser && (
          <div className="flex items-center gap-3 text-muted-foreground text-sm">
            <KnightRiderSpinner />
            <span>Thinking...</span>
          </div>
        )}
        {segments.length === 0 && isUser && (
          <span className="text-muted-foreground italic text-sm">No content</span>
        )}
        
        {segments.map((segment, idx) => {
          if (segment.type === "text") {
            return (
              <div key={`text-${idx}`} className="relative group/text">
                <MarkdownRenderer 
                  content={segment.content} 
                  className="text-sm text-foreground leading-relaxed pr-8"
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
})

const OtherPartCard = memo(function OtherPartCard({ part }: { part: OpenCodePart }) {
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
    <div className="border border-border bg-muted/30 overflow-hidden">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-muted/50 transition-colors"
      >
        <ChevronDown 
          className={cn(
            "w-4 h-4 text-muted-foreground transition-transform",
            isOpen && "rotate-180"
          )}
        />
        <span className="text-xs uppercase tracking-wide text-muted-foreground">{getPartLabel()}</span>
      </button>
      
      {isOpen && (
        <div className="px-3 pb-3 border-t border-border">
          <pre className="text-xs text-muted-foreground mt-2 whitespace-pre-wrap overflow-x-auto">
            {content}
          </pre>
        </div>
      )}
    </div>
  )
})

const TodoListView = memo(function TodoListView({ todos, emptyMessage }: { todos: TodoItem[]; emptyMessage: string }) {
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
          <ListTodo className="w-12 h-12 text-muted-foreground/30 mx-auto mb-3" />
          <p className="text-sm text-muted-foreground">{emptyMessage}</p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {/* Summary header */}
      <div className="p-3 border-b border-border bg-muted/30">
        <div className="flex items-center justify-between text-xs">
          <span className="text-muted-foreground">{summary.total} tasks</span>
          <div className="flex items-center gap-3">
            {summary.inProgress > 0 && (
              <span className="flex items-center gap-1 text-primary">
                <CircleDot className="w-3 h-3" />
                {summary.inProgress}
              </span>
            )}
            {summary.pending > 0 && (
              <span className="flex items-center gap-1 text-muted-foreground">
                <Square className="w-3 h-3" />
                {summary.pending}
              </span>
            )}
            {summary.completed > 0 && (
              <span className="flex items-center gap-1 text-primary">
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
              "flex items-start gap-2 p-2 transition-colors",
              todo.status === "in_progress" && "bg-primary/10 border border-primary/30",
              todo.status === "completed" && "opacity-50",
              todo.status === "cancelled" && "opacity-40",
              todo.status === "pending" && "bg-muted/30 border border-border"
            )}
          >
            {/* Status icon */}
            <div className="flex-shrink-0 mt-0.5">
              {todo.status === "completed" ? (
                <CheckSquare className="w-4 h-4 text-primary" />
              ) : todo.status === "in_progress" ? (
                <CircleDot className="w-4 h-4 text-primary animate-pulse" />
              ) : todo.status === "cancelled" ? (
                <XCircle className="w-4 h-4 text-muted-foreground" />
              ) : (
                <Square className="w-4 h-4 text-muted-foreground" />
              )}
            </div>
            
            {/* Content */}
            <div className="flex-1 min-w-0">
              <p className={cn(
                "text-sm leading-relaxed",
                todo.status === "completed" ? "text-muted-foreground line-through" : "text-foreground",
                todo.status === "cancelled" && "line-through"
              )}>
                {todo.content}
              </p>
            </div>
            
            {/* Priority badge */}
            {todo.priority && (
              <span className={cn(
                "text-[10px] uppercase tracking-wide flex-shrink-0 px-1.5 py-0.5",
                todo.priority === "high" && "bg-red-400/10 text-red-400",
                todo.priority === "medium" && "bg-yellow-400/10 text-yellow-400",
                todo.priority === "low" && "bg-muted text-muted-foreground"
              )}>
                {todo.priority}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  )
})

// Knight Rider style spinner component - pure CSS animation for performance
function KnightRiderSpinner() {
  return (
    <div className="knight-rider-spinner flex items-center gap-[3px]">
      {Array.from({ length: 8 }).map((_, i) => (
        <div
          key={i}
          className="knight-rider-dot w-[6px] h-[6px] rounded-sm bg-primary/5"
          style={{ "--dot-index": i } as React.CSSProperties}
        />
      ))}
      <style jsx>{`
        @keyframes knight-rider {
          0%, 100% { opacity: 0.05; transform: scale(0.85); }
          50% { opacity: 1; transform: scale(1.1); box-shadow: 0 0 8px hsl(var(--primary) / 0.8); }
        }
        .knight-rider-dot {
          animation: knight-rider 1.6s ease-in-out infinite;
          animation-delay: calc(var(--dot-index) * 0.1s);
          background-color: hsl(var(--primary));
        }
      `}</style>
    </div>
  )
}

export default SessionsApp
