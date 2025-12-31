"use client"

import { useCallback, useEffect, useMemo, useState, useRef, memo, useTransition, startTransition } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { FileText, Terminal, Eye, Send, ChevronDown, User, Bot, Clock, ArrowDown, ListTodo, Square, CheckSquare, CircleDot, XCircle, MessageSquare, Loader2, Paperclip, X, Copy, Check, StopCircle, Gauge, Brain } from "lucide-react"
import { useApp } from "@/components/app-context"
import { FileTreeView, type FileTreeState, initialFileTreeState } from "@/app/sessions/FileTreeView"
import { TerminalView } from "@/app/sessions/TerminalView"
import { PreviewView } from "@/app/sessions/PreviewView"
import { MemoriesView } from "@/app/sessions/MemoriesView"
import { useIsMobile } from "@/hooks/use-mobile"
import { useModelContextLimit } from "@/hooks/use-models-dev"
import { MarkdownRenderer, CopyButton } from "@/components/ui/markdown-renderer"
import { ToolCallCard } from "@/components/ui/tool-call-card"
import { cn } from "@/lib/utils"
import {
  fetchMessages,
  sendMessageAsync,
  runShellCommandAsync,
  fetchAgents,
  subscribeToEvents,
  invalidateMessageCache,
  respondToPermission,
  abortSession,
  type OpenCodeMessageWithParts,
  type OpenCodePart,
  type OpenCodeAssistantMessage,
  type Permission,
  type PermissionResponse,
} from "@/lib/opencode-client"
import { PermissionDialog, PermissionBanner } from "@/components/ui/permission-dialog"
import { controlPlaneDirectBaseUrl, fileserverProxyBaseUrl, getChatMessages, convertChatMessagesToOpenCode, getWorkspaceConfig, getFeatures, type Persona, type Features } from "@/lib/control-plane-client"
import { generateReadableId, formatSessionDate } from "@/lib/session-utils"

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

type ActiveView = "chat" | "files" | "terminal" | "preview" | "tasks" | "memories"

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
    <button
      onClick={() => onSelect(view)}
      className={cn(
        "flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
        activeView === view
          ? "bg-primary/15 text-foreground border border-primary"
          : "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50"
      )}
      title={label}
    >
      <Icon className="w-4 h-4" />
      {!hideLabel && <span className="hidden sm:inline ml-1 text-xs">{label}</span>}
      {badge !== undefined && badge > 0 && (
        <span className="absolute -top-1 -right-1 w-3.5 h-3.5 bg-pink-500 text-white text-[9px] rounded-full flex items-center justify-center">
          {badge}
        </span>
      )}
    </button>
  )
}

// Compact copy button for message headers
function CompactCopyButton({ text, className }: { text: string; className?: string }) {
  const [copied, setCopied] = useState(false)
  
  const handleCopy = useCallback(() => {
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text)
      } else {
        const textArea = document.createElement("textarea")
        textArea.value = text
        textArea.style.position = "fixed"
        textArea.style.left = "-9999px"
        document.body.appendChild(textArea)
        textArea.select()
        document.execCommand("copy")
        document.body.removeChild(textArea)
      }
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {}
  }, [text])
  
  return (
    <button
      onClick={handleCopy}
      className={cn("text-muted-foreground hover:text-foreground", className)}
    >
      {copied ? (
        <Check className="w-3 h-3 text-primary" />
      ) : (
        <Copy className="w-3 h-3" />
      )}
    </button>
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
    selectedChatFromHistory,
    refreshOpencodeSessions,
    ensureOpencodeRunning,
    authToken,
    chatHistory,
    projects,
    startProjectSession,
    setSessionBusy,
  } = useApp()
  const [messages, setMessages] = useState<OpenCodeMessageWithParts[]>([])
  const [messageInput, setMessageInput] = useState("")
  
  // Per-chat state (working indicator is per-session, not global)
  const [chatStates, setChatStates] = useState<Map<string, "idle" | "sending">>(new Map())
  const chatState = selectedChatSessionId ? (chatStates.get(selectedChatSessionId) || "idle") : "idle"
  const setChatState = useCallback((state: "idle" | "sending") => {
    if (!selectedChatSessionId) return
    setChatStates(prev => {
      const next = new Map(prev)
      next.set(selectedChatSessionId, state)
      return next
    })
    // Also update global busy state for sidebar indicator
    setSessionBusy(selectedChatSessionId, state === "sending")
  }, [selectedChatSessionId, setSessionBusy])
  
  // Per-chat draft text cache (persists across session switches AND component remounts via localStorage)
  const previousSessionIdRef = useRef<string | null>(null)
  
  // Helper to get/set drafts from localStorage
  const getDraft = useCallback((sessionId: string): string => {
    if (typeof window === "undefined") return ""
    try {
      const drafts = JSON.parse(localStorage.getItem("octo:chatDrafts") || "{}")
      return drafts[sessionId] || ""
    } catch {
      return ""
    }
  }, [])
  
  const setDraft = useCallback((sessionId: string, text: string) => {
    if (typeof window === "undefined") return
    try {
      const drafts = JSON.parse(localStorage.getItem("octo:chatDrafts") || "{}")
      if (text.trim()) {
        drafts[sessionId] = text
      } else {
        delete drafts[sessionId]
      }
      localStorage.setItem("octo:chatDrafts", JSON.stringify(drafts))
    } catch {
      // Ignore localStorage errors
    }
  }, [])
  
  // Restore draft only when switching to a new session
  useEffect(() => {
    const prevId = previousSessionIdRef.current
    const currId = selectedChatSessionId
    
    // Restore draft for current session when switching (or clear if none)
    if (currId && currId !== prevId) {
      const savedDraft = getDraft(currId)
      setMessageInput(savedDraft)
    }
    
    previousSessionIdRef.current = currId
  }, [selectedChatSessionId, getDraft])

  // Auto-resize textarea when messageInput changes programmatically (e.g., draft restoration)
  useEffect(() => {
    if (chatInputRef.current) {
      const textarea = chatInputRef.current
      if (!messageInput) {
        // No content - reset to minimum height
        textarea.style.height = "36px"
      } else {
        // Has content - calculate needed height
        textarea.style.height = "36px" // Reset first to get accurate scrollHeight
        const scrollHeight = textarea.scrollHeight
        textarea.style.height = `${Math.min(scrollHeight, 200)}px`
      }
    }
  }, [messageInput])

  const [isLoading, setIsLoading] = useState(true)
  const [showTimeoutError, setShowTimeoutError] = useState(false)
  const [activeView, setActiveView] = useState<ActiveView>("chat")
  const [status, setStatus] = useState<string>("")
  const [showScrollToBottom, setShowScrollToBottom] = useState(false)
  const [previewFilePath, setPreviewFilePath] = useState<string | null>(null)
  const [fileTreeState, setFileTreeState] = useState<FileTreeState>(initialFileTreeState)
  const messagesContainerRef = useRef<HTMLDivElement>(null)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  
  // Track if user has manually scrolled away from bottom
  const isNearBottomRef = useRef(true)
  const lastSessionIdRef = useRef<string | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const chatInputRef = useRef<HTMLTextAreaElement>(null)
  const chatContainerRef = useRef<HTMLDivElement>(null)
  
  // File upload state
  const [pendingUploads, setPendingUploads] = useState<{ name: string; path: string }[]>([])
  
  // Default agent for shell commands - use "build" as the default primary agent
  const [defaultAgent, setDefaultAgent] = useState<string>("build")
  const [isUploading, setIsUploading] = useState(false)
  
  // Permission state
  const [pendingPermissions, setPendingPermissions] = useState<Permission[]>([])
  const [activePermission, setActivePermission] = useState<Permission | null>(null)
  
  // Track if we're on mobile layout (below lg breakpoint = 1024px)
  const isMobileLayout = useIsMobile()
  
  // Feature flags from backend
  const [features, setFeatures] = useState<Features>({ mmry_enabled: false })
  
  // Fetch features on mount
  useEffect(() => {
    getFeatures().then(setFeatures).catch(() => {
      // Silently ignore - features will remain disabled
    })
  }, [])
  
  // Handle mobile keyboard - scroll input into view when keyboard appears
  // iOS Safari requires special handling as it resizes the visual viewport
  useEffect(() => {
    if (typeof window === "undefined") return
    
    const viewport = window.visualViewport
    if (!viewport) return
    
    let lastHeight = viewport.height
    
    const handleResize = () => {
      const currentHeight = viewport.height
      const heightDiff = lastHeight - currentHeight
      
      // Keyboard likely opened (significant height reduction)
      if (heightDiff > 100) {
        // Scroll the focused input into view
        const activeElement = document.activeElement as HTMLElement
        if (activeElement?.tagName === "INPUT" || activeElement?.tagName === "TEXTAREA") {
          setTimeout(() => {
            activeElement.scrollIntoView({ behavior: "smooth", block: "center" })
          }, 100)
        }
      }
      
      lastHeight = currentHeight
    }
    
    viewport.addEventListener("resize", handleResize)
    return () => viewport.removeEventListener("resize", handleResize)
  }, [])
  
  // Handler for previewing a file from FileTreeView
  const handlePreviewFile = useCallback((filePath: string) => {
    setPreviewFilePath(filePath)
    setActiveView("preview")
  }, [])
  
  // Handler for file tree state changes (for persistence)
  const handleFileTreeStateChange = useCallback((newState: FileTreeState) => {
    setFileTreeState(newState)
  }, [])
  
  // Fetch available agents and check workspace config for default agent
  useEffect(() => {
    if (!opencodeBaseUrl || !selectedWorkspaceSessionId) return
    
    const loadAgentConfig = async () => {
      try {
        // First, check if workspace has a custom agent in opencode.json
        const workspaceConfig = await getWorkspaceConfig(selectedWorkspaceSessionId)
        
        if (workspaceConfig?.agent) {
          // Workspace specifies a custom agent - use it
          console.log("Using workspace-specified agent:", workspaceConfig.agent)
          setDefaultAgent(workspaceConfig.agent)
          return
        }
        
        // No workspace config - fetch available agents and default to "build"
        const agents = await fetchAgents(opencodeBaseUrl)
        console.log("Available agents:", agents)
        
        // Prefer "build" agent (main agent with all tools), fallback to first primary agent
        const buildAgent = agents.find(a => a.id === "build")
        const firstPrimaryAgent = agents.find(a => a.id === "build" || a.id === "plan") || agents[0]
        
        if (buildAgent) {
          setDefaultAgent(buildAgent.id)
        } else if (firstPrimaryAgent) {
          setDefaultAgent(firstPrimaryAgent.id)
        }
        // Keep "build" as fallback if no agents found
      } catch (err) {
        console.error("Failed to load agent config:", err)
        // Keep "build" as fallback on error
      }
    }
    
    loadAgentConfig()
  }, [opencodeBaseUrl, selectedWorkspaceSessionId])

  // Loading state management with timeout
  useEffect(() => {
    // Reset loading state when workspace sessions change
    if (workspaceSessions.length > 0) {
      setIsLoading(false)
      setShowTimeoutError(false)
    } else {
      setIsLoading(true)
      // Show error message after 10 seconds of no response
      const timeout = setTimeout(() => {
        setShowTimeoutError(true)
      }, 10000)
      return () => clearTimeout(timeout)
    }
  }, [workspaceSessions])

  // File upload handler
  const handleFileUpload = useCallback(async (files: FileList | null) => {
    if (!files || files.length === 0 || !selectedWorkspaceSessionId) return
    
    setIsUploading(true)
    const uploadedFiles: { name: string; path: string }[] = []
    
    try {
      const baseUrl = fileserverProxyBaseUrl(selectedWorkspaceSessionId)
      
      for (const file of Array.from(files)) {
        const destPath = `uploads/${file.name}`
        const url = new URL(`${baseUrl}/file`, window.location.origin)
        url.searchParams.set("path", destPath)
        url.searchParams.set("mkdir", "true")
        
        const formData = new FormData()
        formData.append("file", file)
        
        const res = await fetch(url.toString(), {
          method: "POST",
          credentials: "include",
          body: formData,
        })
        
        if (!res.ok) {
          const text = await res.text().catch(() => res.statusText)
          throw new Error(text || `Upload failed (${res.status})`)
        }
        
        uploadedFiles.push({ name: file.name, path: destPath })
      }
      
      setPendingUploads(prev => [...prev, ...uploadedFiles])
    } catch (err) {
      setStatus(err instanceof Error ? err.message : "Upload failed")
    } finally {
      setIsUploading(false)
      // Reset file input
      if (fileInputRef.current) {
        fileInputRef.current.value = ""
      }
    }
  }, [selectedWorkspaceSessionId])

  const removePendingUpload = useCallback((path: string) => {
    setPendingUploads(prev => prev.filter(u => u.path !== path))
  }, [])

  // Permission response handler
  const handlePermissionResponse = useCallback(async (permissionId: string, response: PermissionResponse) => {
    if (!opencodeBaseUrl || !selectedChatSessionId) {
      throw new Error("No active session")
    }
    await respondToPermission(opencodeBaseUrl, selectedChatSessionId, permissionId, response)
    // Remove from pending list
    setPendingPermissions(prev => prev.filter(p => p.id !== permissionId))
  }, [opencodeBaseUrl, selectedChatSessionId])

  // Show next permission when current one is dismissed
  const handlePermissionDismiss = useCallback(() => {
    setActivePermission(current => {
      // Find next pending permission that isn't the current one
      const next = pendingPermissions.find(p => p.id !== current?.id)
      return next || null
    })
  }, [pendingPermissions])

  // Open permission dialog when clicking the banner
  const handlePermissionBannerClick = useCallback(() => {
    if (pendingPermissions.length > 0) {
      setActivePermission(pendingPermissions[0])
    }
  }, [pendingPermissions])

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
        memories: "Erinnerungen",
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
        memories: "Memories",
        noSessions: "No sessions available",
        statusPrefix: "Updated",
        configNotice: "Start the control plane backend to load sessions.",
        noTasks: "No tasks yet.",
      },
    }),
    [],
  )
  const t = copy[locale]

  // Determine if we're viewing a history-only session (no running opencode)
  const isHistoryOnlySession = useMemo(() => {
    // If we have a live opencode session with this ID, it's not history-only
    if (selectedChatSession && selectedChatSession.id === selectedChatSessionId) {
      return false
    }
    // If we have this session in disk history but no live session, it's history-only
    if (selectedChatFromHistory && selectedChatFromHistory.id === selectedChatSessionId) {
      return true
    }
    return false
  }, [selectedChatSession, selectedChatFromHistory, selectedChatSessionId])

  // Merge messages to prevent flickering - preserves existing message references when unchanged
  const mergeMessages = useCallback((prev: OpenCodeMessageWithParts[], next: OpenCodeMessageWithParts[]): OpenCodeMessageWithParts[] => {
    if (prev.length === 0) return next
    if (next.length === 0) return next
    
    // Build a map of existing messages by ID for quick lookup
    const prevById = new Map(prev.map(m => [m.info.id, m]))
    
    // Merge: keep existing reference if message hasn't changed, otherwise use new one
    return next.map(newMsg => {
      const existing = prevById.get(newMsg.info.id)
      if (!existing) return newMsg
      
      // Compare parts length and last part to detect changes
      // This is a lightweight check to avoid deep comparison
      const existingParts = existing.parts
      const newParts = newMsg.parts
      
      if (existingParts.length !== newParts.length) return newMsg
      
      // Check if the last part has changed (most common case during streaming)
      if (newParts.length > 0) {
        const lastNew = newParts[newParts.length - 1]
        const lastExisting = existingParts[existingParts.length - 1]
        
        // Compare text content or tool state
        if (lastNew.type === "text" && lastExisting.type === "text") {
          if (lastNew.text !== lastExisting.text) return newMsg
        } else if (lastNew.type === "tool" && lastExisting.type === "tool") {
          if (lastNew.state?.status !== lastExisting.state?.status ||
              lastNew.state?.output !== lastExisting.state?.output) {
            return newMsg
          }
        } else if (lastNew.type !== lastExisting.type) {
          return newMsg
        }
      }
      
      // No significant changes detected, keep existing reference
      return existing
    })
  }, [])

  const loadMessages = useCallback(async () => {
    if (!selectedChatSessionId) return
    
    try {
      let loadedMessages: OpenCodeMessageWithParts[] = []
      
      if (opencodeBaseUrl && !isHistoryOnlySession) {
        // Live opencode is authoritative for streaming updates.
        loadedMessages = await fetchMessages(opencodeBaseUrl, selectedChatSessionId)
      } else {
        // History-only view (or no live session): use disk history cache.
        try {
          const historyMessages = await getChatMessages(selectedChatSessionId)
          if (historyMessages.length > 0) {
            loadedMessages = convertChatMessagesToOpenCode(historyMessages)
          }
        } catch {
          // Ignore history failures; we don't have a live fallback here.
        }
      }

      if (loadedMessages.length === 0 && opencodeBaseUrl && !isHistoryOnlySession) {
        // If live returned nothing, fall back to disk history for older sessions.
        try {
          const historyMessages = await getChatMessages(selectedChatSessionId)
          if (historyMessages.length > 0) {
            loadedMessages = convertChatMessagesToOpenCode(historyMessages)
          }
        } catch {
          // Ignore history failures on fallback.
        }
      }
      
      // Use merge to prevent flickering when updating
      setMessages(prev => mergeMessages(prev, loadedMessages))
    } catch (err) {
      setStatus((err as Error).message)
    }
  }, [opencodeBaseUrl, selectedChatSessionId, isHistoryOnlySession, mergeMessages])

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
    const container = messagesContainerRef.current
    if (!container) return
    
    container.scrollTo({
      top: container.scrollHeight,
      behavior,
    })
    isNearBottomRef.current = true
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
    
    // Track if user is near bottom (within 150px)
    isNearBottomRef.current = distanceFromBottom < 150
    
    // Show button when more than 300px from bottom
    setShowScrollToBottom(distanceFromBottom > 300)
  }, [])

  // Check scroll position when messages change
  useEffect(() => {
    handleScroll()
  }, [messages, handleScroll])
  
  // Scroll to bottom when switching sessions
  useEffect(() => {
    if (!selectedChatSessionId) return
    
    // Detect session switch
    if (lastSessionIdRef.current !== selectedChatSessionId) {
      lastSessionIdRef.current = selectedChatSessionId
      isNearBottomRef.current = true // Reset to bottom on session switch
      
      // Wait for messages to load and render, then scroll to bottom
      const timeoutId = setTimeout(() => {
        scrollToBottom("instant")
      }, 50)
      
      return () => clearTimeout(timeoutId)
    }
  }, [selectedChatSessionId, scrollToBottom])
  
  // Auto-scroll to bottom when new messages arrive (if user is near bottom)
  useEffect(() => {
    if (messages.length === 0) return
    if (!isNearBottomRef.current) return
    
    // Scroll to bottom when messages change and user was near bottom
    scrollToBottom("instant")
  }, [messages, scrollToBottom])

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
        
        // Handle permission events
        if (eventType === "permission.updated") {
          const permission = event.properties as Permission
          console.log("[Permission] Received permission request:", permission)
          setPendingPermissions(prev => {
            // Avoid duplicates
            if (prev.some(p => p.id === permission.id)) return prev
            return [...prev, permission]
          })
          // Auto-show the first permission dialog if none is active
          setActivePermission(current => current || permission)
        } else if (eventType === "permission.replied") {
          const { permissionID } = event.properties as { sessionID: string; permissionID: string; response: string }
          console.log("[Permission] Permission replied:", permissionID)
          setPendingPermissions(prev => prev.filter(p => p.id !== permissionID))
          setActivePermission(current => current?.id === permissionID ? null : current)
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
        
        // Use merge to prevent flickering
        setMessages(prev => mergeMessages(prev, freshMessages))
        
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
  }, [chatState, opencodeBaseUrl, selectedChatSessionId, refreshOpencodeSessions, mergeMessages])

  useEffect(() => {
    return () => {
      const state = messageRefreshStateRef.current
      if (state.timer) {
        clearTimeout(state.timer)
        state.timer = null
      }
    }
  }, [])

  // Double-Escape keyboard shortcut to stop agent (like opencode TUI)
  useEffect(() => {
    let lastEscapeTime = 0
    const DOUBLE_PRESS_THRESHOLD = 500 // ms
    
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && chatState === "sending") {
        const now = Date.now()
        if (now - lastEscapeTime < DOUBLE_PRESS_THRESHOLD) {
          // Double-escape detected - stop the agent
          e.preventDefault()
          if (opencodeBaseUrl && selectedChatSessionId) {
            abortSession(opencodeBaseUrl, selectedChatSessionId)
              .then(() => {
                setChatState("idle")
                setStatus(locale === "de" ? "Abgebrochen" : "Stopped")
                setTimeout(() => setStatus(""), 2000)
              })
              .catch((err) => setStatus((err as Error).message))
          }
          lastEscapeTime = 0 // Reset
        } else {
          lastEscapeTime = now
        }
      }
    }
    
    window.addEventListener("keydown", handleKeyDown)
    return () => window.removeEventListener("keydown", handleKeyDown)
  }, [chatState, opencodeBaseUrl, selectedChatSessionId, locale])

  const selectedSession = useMemo(() => {
    if (!selectedWorkspaceSessionId) return undefined
    return workspaceSessions.find((session) => session.id === selectedWorkspaceSessionId)
  }, [workspaceSessions, selectedWorkspaceSessionId])

  const messageGroups = useMemo(() => groupMessages(messages), [messages])
  
  // Progressive rendering - show last N groups immediately, expand on scroll up
  const [visibleGroupCount, setVisibleGroupCount] = useState(20)
  
  // Reset visible count when session changes
  useEffect(() => {
    setVisibleGroupCount(20)
  }, [selectedChatSessionId])
  
  // Calculate which groups to show (from the end, so newest messages are visible)
  const visibleGroups = useMemo(() => {
    if (messageGroups.length <= visibleGroupCount) {
      return messageGroups
    }
    return messageGroups.slice(-visibleGroupCount)
  }, [messageGroups, visibleGroupCount])
  
  const hasHiddenMessages = messageGroups.length > visibleGroupCount
  
  const loadMoreMessages = useCallback(() => {
    setVisibleGroupCount(prev => Math.min(prev + 20, messageGroups.length))
  }, [messageGroups.length])

  // Calculate total tokens and extract current model for context window gauge
  const tokenUsage = useMemo(() => {
    let inputTokens = 0
    let outputTokens = 0
    let providerID: string | undefined
    let modelID: string | undefined
    
    for (const msg of messages) {
      if (msg.info.role === "assistant") {
        const assistantInfo = msg.info as OpenCodeAssistantMessage
        if (assistantInfo.tokens) {
          inputTokens += assistantInfo.tokens.input || 0
          outputTokens += assistantInfo.tokens.output || 0
        }
        // Track the most recent model used
        if (assistantInfo.providerID && assistantInfo.modelID) {
          providerID = assistantInfo.providerID
          modelID = assistantInfo.modelID
        }
      }
    }
    
    return { inputTokens, outputTokens, providerID, modelID }
  }, [messages])
  
  // Get context limit from models.dev based on current model
  const contextLimit = useModelContextLimit(
    tokenUsage.providerID,
    tokenUsage.modelID,
    200000 // Default fallback
  )

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
    if (!selectedChatSessionId) return
    if (!messageInput.trim() && pendingUploads.length === 0) return
    
    // Build message text with uploaded file paths
    let messageText = messageInput.trim()
    if (pendingUploads.length > 0) {
      const uploadPrefix = pendingUploads.length === 1
        ? `[Uploaded file: ${pendingUploads[0].path}]`
        : `[Uploaded files: ${pendingUploads.map(u => u.path).join(", ")}]`
      messageText = messageText ? `${uploadPrefix}\n\n${messageText}` : uploadPrefix
    }
    
    // Check if this is a shell command (starts with "!")
    const isShellCommand = messageText.startsWith("!")
    const shellCommand = isShellCommand ? messageText.slice(1).trim() : ""
    
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
    // Reset textarea height to minimum
    if (chatInputRef.current) {
      chatInputRef.current.style.height = "36px"
    }
    // Clear draft cache for this session since message was sent
    if (selectedChatSessionId) {
      setDraft(selectedChatSessionId, "")
    }
    setPendingUploads([])
    setChatState("sending")
    setStatus("")
    
    // Scroll to bottom immediately
    setTimeout(() => scrollToBottom(), 50)
    
    try {
      // If we don't have an opencode URL (history-only session), start opencode first
      let effectiveBaseUrl = opencodeBaseUrl
      if (!effectiveBaseUrl || isHistoryOnlySession) {
        // Get workspace path from history session
        const workspacePath = selectedChatFromHistory?.workspace_path
        if (!workspacePath) {
          throw new Error("Cannot resume session: no workspace path found")
        }
        
        // Start opencode for this workspace
        setStatus(locale === "de" ? "Starte OpenCode..." : "Starting OpenCode...")
        const url = await ensureOpencodeRunning(workspacePath)
        if (!url) {
          throw new Error("Failed to start OpenCode for this workspace")
        }
        effectiveBaseUrl = url
        setStatus("")
      }
      
      if (isShellCommand && shellCommand) {
        // Run shell command via opencode shell endpoint using "build" agent
        const agentId = defaultAgent || "build"
        console.log("Running shell command with agent:", agentId, "command:", shellCommand)
        await runShellCommandAsync(effectiveBaseUrl, selectedChatSessionId, shellCommand, agentId)
      } else {
        // Use async send - the response will come via SSE events
        await sendMessageAsync(effectiveBaseUrl, selectedChatSessionId, messageText)
      }
      // Invalidate cache and refresh messages to get the real message IDs
      invalidateMessageCache(effectiveBaseUrl, selectedChatSessionId)
      loadMessages()
    } catch (err) {
      setStatus((err as Error).message)
      setChatState("idle")
      // Remove optimistic message on error
      setMessages((prev) => prev.filter((m) => !m.info.id.startsWith("temp-")))
    }
    // Don't set idle here - wait for SSE session.idle event
  }

  const handleStop = async () => {
    if (!opencodeBaseUrl || !selectedChatSessionId) return
    if (chatState !== "sending") return
    
    try {
      await abortSession(opencodeBaseUrl, selectedChatSessionId)
      // The SSE event will set the state to idle
      // But set it immediately for responsiveness
      setChatState("idle")
      setStatus(locale === "de" ? "Abgebrochen" : "Stopped")
      // Clear status after a moment
      setTimeout(() => setStatus(""), 2000)
    } catch (err) {
      setStatus((err as Error).message)
    }
  }

  // Loading skeleton for chat view
  const ChatSkeleton = (
    <div className="flex-1 flex flex-col gap-4 min-h-0 animate-pulse">
      <div className="flex-1 bg-muted/20 p-4 space-y-6">
        {/* Skeleton message bubbles */}
        <div className="mr-8 space-y-2">
          <div className="h-4 bg-muted/40 w-24" />
          <div className="h-16 bg-muted/30" />
        </div>
        <div className="ml-8 space-y-2">
          <div className="h-4 bg-muted/40 w-16 ml-auto" />
          <div className="h-10 bg-muted/30" />
        </div>
        <div className="mr-8 space-y-2">
          <div className="h-4 bg-muted/40 w-24" />
          <div className="h-24 bg-muted/30" />
        </div>
      </div>
      <div className="h-10 bg-muted/20" />
    </div>
  )

  // Loading skeleton for sidebar
  const SidebarSkeleton = (
    <div className="flex-1 flex flex-col animate-pulse">
      <div className="flex gap-1 p-2">
        {[1, 2, 3, 4].map((i) => (
          <div key={i} className="flex-1 h-8 bg-muted/30" />
        ))}
      </div>
      <div className="flex-1 p-4 space-y-3">
        <div className="h-4 bg-muted/40 w-3/4" />
        <div className="h-4 bg-muted/40 w-1/2" />
        <div className="h-4 bg-muted/40 w-2/3" />
        <div className="h-32 bg-muted/30 mt-4" />
      </div>
    </div>
  )

  // Show loading skeleton or error only if we have no sessions AND no chat history
  if (workspaceSessions.length === 0 && chatHistory.length === 0) {
    // Show skeleton while loading, project selector after timeout
    return (
      <div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4">
        {showTimeoutError ? (
          <div className="p-4 md:p-6 max-w-2xl mx-auto w-full">
            <div className="p-6 bg-card border border-border rounded-lg">
              <h2 className="text-lg font-medium mb-4">{locale === "de" ? "Projekt auswählen" : "Select a Project"}</h2>
              {projects.length > 0 ? (
                <div className="grid gap-2 max-h-[60vh] overflow-y-auto">
                  {projects.map((project) => (
                    <button
                      key={project.path}
                      onClick={() => startProjectSession(project.path)}
                      className="flex items-center gap-3 p-3 text-left rounded-md border border-border hover:bg-muted/50 transition-colors"
                    >
                      <FileText className="h-5 w-5 text-muted-foreground flex-shrink-0" />
                      <div className="min-w-0">
                        <div className="font-medium truncate">{project.name}</div>
                        <div className="text-sm text-muted-foreground truncate">{project.path}</div>
                      </div>
                    </button>
                  ))}
                </div>
              ) : (
                <div className="text-sm text-muted-foreground">
                  {t.configNotice}
                </div>
              )}
            </div>
          </div>
        ) : (
          <>
            {/* Mobile skeleton */}
            <div className="flex-1 min-h-0 flex flex-col lg:hidden">
              <div className="sticky top-0 z-10 flex gap-0.5 p-1 sm:p-2 bg-muted/10">
                {[1, 2, 3, 4, 5].map((i) => (
                  <div key={i} className="flex-1 h-7 bg-muted/30 animate-pulse" />
                ))}
              </div>
              <div className="flex-1 min-h-0 bg-muted/10 p-1.5 sm:p-4 overflow-hidden">
                {ChatSkeleton}
              </div>
            </div>

            {/* Desktop skeleton */}
            <div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
              <div className="flex-[3] min-w-0 bg-muted/10 p-4 xl:p-6 flex flex-col min-h-0 h-full">
                {ChatSkeleton}
              </div>
              <div className="flex-[2] min-w-[320px] max-w-[420px] bg-muted/10 flex flex-col min-h-0 h-full">
                {SidebarSkeleton}
              </div>
            </div>
          </>
        )}
      </div>
    )
  }

  // Chat content component (reused in both layouts)
  const ChatContent = (
    <div ref={chatContainerRef} className="flex-1 flex flex-col gap-2 sm:gap-4 min-h-0">
      {/* Permission banner */}
      <PermissionBanner 
        count={pendingPermissions.length} 
        onClick={handlePermissionBannerClick} 
      />
      
      {/* Working indicator with stop button */}
      {chatState === "sending" && (
        <div className="flex items-center gap-1.5 px-2 py-0.5 bg-primary/10 text-xs text-primary">
          <KnightRiderSpinner />
          <span className="font-medium flex-1">{locale === "de" ? "Agent arbeitet..." : "Agent working..."}</span>
          <button
            onClick={handleStop}
            className="mr-1 text-destructive hover:text-destructive/80 transition-colors"
            title={locale === "de" ? "Agent stoppen (2x Esc)" : "Stop agent (2x Esc)"}
          >
            <StopCircle className="w-5 h-5" />
          </button>
        </div>
      )}
      <div className="relative flex-1 min-h-0">
        <div 
          ref={messagesContainerRef}
          onScroll={handleScroll}
          className="h-full bg-muted/30 border border-border p-2 sm:p-4 overflow-y-auto space-y-4 sm:space-y-6 scrollbar-hide"
        >
          {messages.length === 0 && <div className="text-sm text-muted-foreground">{t.noMessages}</div>}
          {hasHiddenMessages && (
            <button
              onClick={loadMoreMessages}
              className="w-full py-2 text-xs text-muted-foreground hover:text-foreground hover:bg-muted/50 border border-dashed border-border transition-colors"
            >
              {locale === "de" 
                ? `${messageGroups.length - visibleGroupCount} altere Nachrichten laden...` 
                : `Load ${messageGroups.length - visibleGroupCount} older messages...`}
            </button>
          )}
          {visibleGroups.map((group) => (
            <MessageGroupCard key={group.messages[0]?.info.id || `${group.role}-${group.startIndex}`} group={group} persona={selectedSession?.persona} />
          ))}
          <div ref={messagesEndRef} />
        </div>

        {/* Jump to bottom button */}
        {showScrollToBottom && (
          <button
            onClick={() => scrollToBottom()}
            className="absolute bottom-2 left-2 right-2 sm:left-1/2 sm:-translate-x-1/2 sm:right-auto sm:w-auto z-50 flex items-center justify-center gap-2 px-3 py-2 bg-primary hover:bg-primary/90 text-primary-foreground text-sm font-medium shadow-lg"
          >
            <ArrowDown className="w-4 h-4" />
            <span className="sm:inline">Jump to bottom</span>
          </button>
        )}
      </div>

      {/* Pending uploads indicator */}
      {pendingUploads.length > 0 && (
        <div className="flex flex-wrap gap-2 mb-2">
          {pendingUploads.map((upload) => (
            <div
              key={upload.path}
              className="flex items-center gap-1.5 px-2 py-1 bg-primary/10 border border-primary/30 text-xs text-foreground"
            >
              <Paperclip className="w-3 h-3 text-primary" />
              <span className="truncate max-w-[150px]">{upload.name}</span>
              <button
                onClick={() => removePendingUpload(upload.path)}
                className="text-muted-foreground hover:text-foreground ml-1"
              >
                <X className="w-3 h-3" />
              </button>
            </div>
          ))}
        </div>
      )}

      {/* Hidden file input */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        className="hidden"
        onChange={(e) => handleFileUpload(e.target.files)}
      />

      {/* Chat input - works for both live and history sessions */}
      <div className="chat-input-container flex flex-col gap-1 bg-muted/30 border border-border px-2 py-1">
        {/* Show hint for history sessions that will be resumed */}
        {isHistoryOnlySession && (
          <div className="flex items-center gap-1.5 px-1 pt-1 text-xs text-muted-foreground">
            <Clock className="w-3 h-3" />
            <span>
              {locale === "de" 
                ? "Sende eine Nachricht um diese Sitzung fortzusetzen" 
                : "Send a message to resume this session"}
            </span>
          </div>
        )}
        <div className="flex items-end gap-2">
          <button
            onClick={() => fileInputRef.current?.click()}
            disabled={isUploading}
            className="flex-shrink-0 p-1.5 mb-1 text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            title={locale === "de" ? "Datei hochladen" : "Upload file"}
          >
            {isUploading ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Paperclip className="w-4 h-4" />
            )}
          </button>
          <textarea
            ref={chatInputRef}
            placeholder={isHistoryOnlySession 
              ? (locale === "de" ? "Nachricht zum Fortsetzen..." : "Message to resume...") 
              : t.inputPlaceholder}
            value={messageInput}
            onChange={(e) => {
              setMessageInput(e.target.value)
              // Auto-resize is handled by useEffect on messageInput change
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault()
                handleSend()
                // Reset textarea height after sending
                if (chatInputRef.current) {
                  chatInputRef.current.style.height = "36px"
                }
              }
            }}
            onPaste={(e) => {
              // Handle pasted files (images, etc.)
              const items = e.clipboardData?.items
              if (!items) return
              
              const files: File[] = []
              for (const item of Array.from(items)) {
                if (item.kind === "file") {
                  const file = item.getAsFile()
                  if (file) {
                    files.push(file)
                  }
                }
              }
              
              if (files.length > 0) {
                // Prevent default paste behavior for files
                e.preventDefault()
                // Create a FileList-like object and upload
                const dataTransfer = new DataTransfer()
                files.forEach(f => dataTransfer.items.add(f))
                handleFileUpload(dataTransfer.files)
              }
              // If no files, let the default paste behavior handle text
            }}
            onFocus={(e) => {
              // Scroll input into view on mobile when keyboard opens
              setTimeout(() => {
                e.target.scrollIntoView({ behavior: "smooth", block: "nearest" })
              }, 300)
            }}
            rows={1}
            className="flex-1 bg-transparent border-none outline-none text-foreground placeholder:text-muted-foreground text-sm resize-none min-h-[36px] max-h-[200px] py-2 leading-tight overflow-y-auto"
          />
          <Button
            onClick={handleSend}
            disabled={chatState === "sending" || (!messageInput.trim() && pendingUploads.length === 0)}
            className="bg-primary hover:bg-primary/90 text-primary-foreground mb-1"
          >
            <Send className="w-4 h-4 sm:mr-2" />
            <span className="hidden sm:inline">{t.send}</span>
          </Button>
        </div>
      </div>
    </div>
  )

  const incompleteTasks = latestTodos.filter(t => t.status !== "completed" && t.status !== "cancelled").length

  // Format session metadata for display
  const sessionCreatedAt = selectedChatSession?.time?.created
  const formattedDate = sessionCreatedAt ? formatSessionDate(sessionCreatedAt) : null
  const readableId = selectedChatSession?.id ? generateReadableId(selectedChatSession.id) : null
  
  // Clean up session title - remove ISO timestamp suffix if present (e.g., "New session - 2025-12-18T07:46:58.478Z")
  const cleanSessionTitle = (() => {
    const title = selectedChatSession?.title
    if (!title) return null
    // Remove " - YYYY-MM-DDTHH:MM:SS.sssZ" pattern from the end
    return title.replace(/\s*-\s*\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z?$/, "").trim() || null
  })()

  // Session header component for reuse
  const persona = selectedSession?.persona
  const SessionHeader = (
    <div className="flex items-center justify-between pb-3 mb-3 border-b border-border">
      <div className="flex items-center gap-3 min-w-0 flex-1">
        {/* Persona avatar/indicator */}
        {persona && (
          <div 
            className="w-8 h-8 sm:w-10 sm:h-10 rounded-full flex items-center justify-center flex-shrink-0"
            style={{ backgroundColor: persona.color || "#6366f1" }}
          >
            <User className="w-4 h-4 sm:w-5 sm:h-5 text-white" />
          </div>
        )}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h1 className="text-base sm:text-lg font-semibold text-foreground tracking-wider truncate">
              {cleanSessionTitle || t.title}
            </h1>
            {persona && (
              <span 
                className="text-xs px-1.5 py-0.5 rounded-full text-white flex-shrink-0"
                style={{ backgroundColor: persona.color || "#6366f1" }}
              >
                {persona.name}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2 text-xs text-foreground/60 dark:text-muted-foreground">
            {readableId && <span className="font-mono">{readableId}</span>}
            {readableId && formattedDate && <span className="opacity-50">|</span>}
            {formattedDate && <span>{formattedDate}</span>}
          </div>
        </div>
      </div>
      <div className="flex items-center gap-3 flex-shrink-0 ml-2">
        {status && <span className="text-xs text-destructive">{status}</span>}
        <ContextWindowGauge 
          inputTokens={tokenUsage.inputTokens} 
          outputTokens={tokenUsage.outputTokens} 
          maxTokens={contextLimit}
          locale={locale}
        />
      </div>
    </div>
  )

  return (
    <div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4">
      {/* Mobile layout: single panel with tabs */}
      <div className="flex-1 min-h-0 flex flex-col lg:hidden">
        {/* Mobile tabs - sticky at top */}
        <div className="sticky top-0 z-10 bg-card border border-border rounded-t-xl overflow-hidden">
          <div className="flex gap-0.5 p-1 sm:p-2">
            <TabButton activeView={activeView} onSelect={setActiveView} view="chat" icon={MessageSquare} label={t.chat} />
            <TabButton activeView={activeView} onSelect={setActiveView} view="tasks" icon={ListTodo} label={t.tasks} badge={incompleteTasks} />
            <TabButton activeView={activeView} onSelect={setActiveView} view="files" icon={FileText} label={t.files} />
            <TabButton activeView={activeView} onSelect={setActiveView} view="preview" icon={Eye} label={t.preview} />
            {features.mmry_enabled && <TabButton activeView={activeView} onSelect={setActiveView} view="memories" icon={Brain} label={t.memories} />}
            <TabButton activeView={activeView} onSelect={setActiveView} view="terminal" icon={Terminal} label={t.terminal} />
          </div>
          {/* Mobile context window gauge - full width bar directly below tabs */}
          <ContextWindowGauge 
            inputTokens={tokenUsage.inputTokens} 
            outputTokens={tokenUsage.outputTokens} 
            maxTokens={contextLimit}
            locale={locale}
            compact
          />
        </div>
        
        {/* Mobile content */}
        <div className="flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-1.5 sm:p-4 overflow-hidden flex flex-col">
          {activeView === "chat" && ChatContent}
          {activeView === "files" && <FileTreeView onPreviewFile={handlePreviewFile} state={fileTreeState} onStateChange={handleFileTreeStateChange} />}
          {activeView === "preview" && <PreviewView filePath={previewFilePath} />}
          {activeView === "tasks" && <TodoListView todos={latestTodos} emptyMessage={t.noTasks} />}
          {features.mmry_enabled && activeView === "memories" && <MemoriesView />}
          {/* Terminal only rendered in mobile layout when isMobileLayout is true */}
          {isMobileLayout && (
            <div className={activeView === "terminal" ? "h-full" : "hidden"}>
              <TerminalView sessionId={selectedWorkspaceSessionId} />
            </div>
          )}
        </div>
      </div>

      {/* Desktop layout: side by side */}
      <div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
        {/* Chat panel */}
        <div className="flex-[3] min-w-0 bg-card border border-border p-4 xl:p-6 flex flex-col min-h-0 h-full">
          {SessionHeader}
          {ChatContent}
        </div>

        {/* Sidebar panel */}
        <div className="flex-[2] min-w-[320px] max-w-[420px] bg-card border border-border flex flex-col min-h-0 h-full">
          <div className="flex gap-1 p-2 border-b border-border">
            <TabButton activeView={activeView} onSelect={setActiveView} view="tasks" icon={ListTodo} label={t.tasks} badge={incompleteTasks} hideLabel />
            <TabButton activeView={activeView} onSelect={setActiveView} view="files" icon={FileText} label={t.files} hideLabel />
            <TabButton activeView={activeView} onSelect={setActiveView} view="preview" icon={Eye} label={t.preview} hideLabel />
            {features.mmry_enabled && <TabButton activeView={activeView} onSelect={setActiveView} view="memories" icon={Brain} label={t.memories} hideLabel />}
            <TabButton activeView={activeView} onSelect={setActiveView} view="terminal" icon={Terminal} label={t.terminal} hideLabel />
          </div>
          <div className="flex-1 min-h-0 overflow-hidden">
            {activeView === "files" && <FileTreeView onPreviewFile={handlePreviewFile} state={fileTreeState} onStateChange={handleFileTreeStateChange} />}
            {activeView === "preview" && <PreviewView filePath={previewFilePath} />}
            {activeView === "tasks" && <TodoListView todos={latestTodos} emptyMessage={t.noTasks} />}
            {activeView === "chat" && <TodoListView todos={latestTodos} emptyMessage={t.noTasks} />}
            {features.mmry_enabled && activeView === "memories" && <MemoriesView />}
            {/* Terminal only rendered in desktop layout when isMobileLayout is false */}
            {!isMobileLayout && (
              <div className={activeView === "terminal" ? "h-full" : "hidden"}>
                <TerminalView sessionId={selectedWorkspaceSessionId} />
              </div>
            )}
          </div>
        </div>
      </div>
      
      {/* Permission dialog */}
      <PermissionDialog
        permission={activePermission}
        onRespond={handlePermissionResponse}
        onDismiss={handlePermissionDismiss}
      />
    </div>
  )
}

const MessageGroupCard = memo(function MessageGroupCard({ group, persona }: { group: MessageGroup; persona?: Persona | null }) {
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

  // Get assistant display name from persona or default to "Assistant"
  const assistantName = persona?.name || "Assistant"
  const personaColor = persona?.color

  return (
    <div
      className={cn(
        "transition-all duration-200 overflow-hidden",
        isUser 
          ? "sm:ml-8 bg-primary/20 dark:bg-primary/10 border border-primary/40 dark:border-primary/30" 
          : "sm:mr-8 bg-muted/50 border border-border"
      )}
      style={!isUser && personaColor ? { borderLeftColor: personaColor, borderLeftWidth: "3px" } : undefined}
    >
      {/* Header */}
      <div className={cn(
        "compact-header flex items-center gap-1 sm:gap-2 px-2 sm:px-3 py-1.5 sm:py-2 border-b",
        isUser ? "border-primary/30 dark:border-primary/20" : "border-border"
      )}>
        {isUser ? (
          <User className="w-3 h-3 sm:w-4 sm:h-4 text-primary flex-shrink-0" />
        ) : personaColor ? (
          <div 
            className="w-3 h-3 sm:w-4 sm:h-4 rounded-full flex-shrink-0"
            style={{ backgroundColor: personaColor }}
          />
        ) : (
          <Bot className="w-3 h-3 sm:w-4 sm:h-4 text-primary flex-shrink-0" />
        )}
        <span className="text-sm font-medium text-foreground">
          {isUser ? "You" : assistantName}
        </span>
        {group.messages.length > 1 && (
          <span className={cn(
            "text-[9px] sm:text-[10px] px-1 border leading-none",
            isUser
              ? "border-primary/30 text-primary"
              : "border-border text-muted-foreground"
          )}>
            {group.messages.length}
          </span>
        )}
        <div className="flex-1" />
        {createdAt && !isNaN(createdAt.getTime()) && (
          <span className="text-[9px] sm:text-[10px] text-foreground/50 dark:text-muted-foreground leading-none sm:leading-normal">
            {createdAt.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          </span>
        )}
        {/* Copy button - full size on desktop, compact on mobile */}
        {allTextContent && (
          <CopyButton text={allTextContent} className="hidden sm:block opacity-0 group-hover:opacity-100 ml-2" />
        )}
        {allTextContent && (
          <CompactCopyButton text={allTextContent} className="sm:hidden ml-2" />
        )}
      </div>

      {/* Content - render segments in order */}
      <div className="px-2 sm:px-4 py-2 sm:py-3 group space-y-3 overflow-hidden">
        {segments.length === 0 && !isUser && (
          <div className="flex items-center gap-3 text-muted-foreground text-sm">
            <KnightRiderSpinner />
            <span>Working...</span>
          </div>
        )}
        {segments.length === 0 && isUser && (
          <span className="text-muted-foreground italic text-sm">No content</span>
        )}
        
        {segments.map((segment, idx) => {
          if (segment.type === "text") {
            return (
              <div key={`text-${idx}`} className="overflow-hidden">
                <MarkdownRenderer 
                  content={segment.content} 
                  className="text-sm text-foreground leading-relaxed overflow-hidden"
                />
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

// Knight Rider style spinner component - classic KITT scanner effect with trailing swoosh
// Context Window Gauge component
function ContextWindowGauge({ 
  inputTokens, 
  outputTokens, 
  maxTokens = 200000,
  locale,
  compact = false,
}: { 
  inputTokens: number
  outputTokens: number
  maxTokens?: number
  locale: "de" | "en"
  compact?: boolean
}) {
  const totalTokens = inputTokens + outputTokens
  const percentage = Math.min((totalTokens / maxTokens) * 100, 100)
  
  // Color based on usage
  const getColor = () => {
    if (percentage >= 90) return "bg-destructive"
    if (percentage >= 70) return "bg-yellow-500"
    return "bg-primary"
  }
  
  const formatTokens = (n: number) => {
    if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`
    if (n >= 1000) return `${(n / 1000).toFixed(1)}K`
    return n.toString()
  }
  
  if (totalTokens === 0) return null
  
  // Compact mode for mobile - full width bar, no icon
  if (compact) {
    return (
      <div 
        className="w-full h-1 bg-muted overflow-hidden"
        title={`${locale === "de" ? "Kontextfenster" : "Context window"}: ${formatTokens(totalTokens)} / ${formatTokens(maxTokens)} tokens (${percentage.toFixed(0)}%)`}
      >
        <div 
          className={cn("h-full transition-all duration-300", getColor())}
          style={{ width: `${percentage}%` }}
        />
      </div>
    )
  }
  
  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground" title={`${locale === "de" ? "Kontextfenster" : "Context window"}: ${formatTokens(totalTokens)} / ${formatTokens(maxTokens)} tokens`}>
      <Gauge className="w-3.5 h-3.5" />
      <div className="flex items-center gap-1.5">
        <div className="w-16 h-1.5 bg-muted rounded-full overflow-hidden">
          <div 
            className={cn("h-full transition-all duration-300", getColor())}
            style={{ width: `${percentage}%` }}
          />
        </div>
        <span className="font-mono text-[10px]">{percentage.toFixed(0)}%</span>
      </div>
    </div>
  )
}

function KnightRiderSpinner() {
  return (
    <>
      <style>{`
        @keyframes kitt-swoosh {
          0% { left: -35%; opacity: 0; }
          10% { left: 0%; opacity: 1; }
          40% { left: 80%; opacity: 1; }
          50% { left: 115%; opacity: 0; }
          60% { left: 80%; opacity: 1; }
          90% { left: 0%; opacity: 1; }
          100% { left: -35%; opacity: 0; }
        }
        @keyframes kitt-ghost-1 {
          0% { left: -33.5%; opacity: 0; }
          10% { left: 1.5%; opacity: 0.6; }
          40% { left: 78.5%; opacity: 0.6; }
          50% { left: 113.5%; opacity: 0; }
          60% { left: 78.5%; opacity: 0.6; }
          90% { left: 1.5%; opacity: 0.6; }
          100% { left: -33.5%; opacity: 0; }
        }
        @keyframes kitt-ghost-2 {
          0% { left: -32%; opacity: 0; }
          10% { left: 3%; opacity: 0.4; }
          40% { left: 77%; opacity: 0.4; }
          50% { left: 112%; opacity: 0; }
          60% { left: 77%; opacity: 0.4; }
          90% { left: 3%; opacity: 0.4; }
          100% { left: -32%; opacity: 0; }
        }
        @keyframes kitt-ghost-3 {
          0% { left: -30.5%; opacity: 0; }
          10% { left: 4.5%; opacity: 0.25; }
          40% { left: 75.5%; opacity: 0.25; }
          50% { left: 110.5%; opacity: 0; }
          60% { left: 75.5%; opacity: 0.25; }
          90% { left: 4.5%; opacity: 0.25; }
          100% { left: -30.5%; opacity: 0; }
        }
        @keyframes kitt-ghost-4 {
          0% { left: -29%; opacity: 0; }
          10% { left: 6%; opacity: 0.15; }
          40% { left: 74%; opacity: 0.15; }
          50% { left: 109%; opacity: 0; }
          60% { left: 74%; opacity: 0.15; }
          90% { left: 6%; opacity: 0.15; }
          100% { left: -29%; opacity: 0; }
        }
      `}</style>
      <div className="relative h-[6px] w-[60px] rounded-full overflow-hidden bg-primary/15">
        {/* Ghost trails - furthest back */}
        <div
          className="absolute top-0 h-full rounded-full"
          style={{
            width: "20%",
            background: "linear-gradient(to right, transparent, var(--primary), transparent)",
            animation: "kitt-ghost-4 1.2s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite",
            filter: "blur(2px)",
          }}
        />
        <div
          className="absolute top-0 h-full rounded-full"
          style={{
            width: "20%",
            background: "linear-gradient(to right, transparent, var(--primary), transparent)",
            animation: "kitt-ghost-3 1.2s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite",
            filter: "blur(1.5px)",
          }}
        />
        <div
          className="absolute top-0 h-full rounded-full"
          style={{
            width: "20%",
            background: "linear-gradient(to right, transparent, var(--primary), transparent)",
            animation: "kitt-ghost-2 1.2s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite",
            filter: "blur(1px)",
          }}
        />
        <div
          className="absolute top-0 h-full rounded-full"
          style={{
            width: "20%",
            background: "linear-gradient(to right, transparent, var(--primary), transparent)",
            animation: "kitt-ghost-1 1.2s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite",
            filter: "blur(0.5px)",
          }}
        />
        {/* Main bright light */}
        <div
          className="absolute top-0 h-full rounded-full"
          style={{
            width: "20%",
            background: "linear-gradient(to right, transparent, var(--primary), transparent)",
            boxShadow: "0 0 8px var(--primary), 0 0 12px var(--primary)",
            animation: "kitt-swoosh 1.2s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite",
          }}
        />
      </div>
    </>
  )
}

export default SessionsApp
