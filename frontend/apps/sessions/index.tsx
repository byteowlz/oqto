"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { FileText, Terminal, Eye, Send, RefreshCw } from "lucide-react"
import { useApp } from "@/components/app-context"
import { FileTreeView } from "@/app/sessions/FileTreeView"
import { TerminalView } from "@/app/sessions/TerminalView"
import { PreviewView } from "@/app/sessions/PreviewView"
import { hasOpencode } from "@/lib/config"
import {
  fetchSessions,
  fetchMessages,
  sendMessage,
  subscribeToEvents,
  type OpenCodeSession,
  type OpenCodeMessage,
} from "@/lib/opencode-client"

export function SessionsApp() {
  const { locale } = useApp()
  const [sessions, setSessions] = useState<OpenCodeSession[]>([])
  const [selectedSessionId, setSelectedSessionId] = useState<string>("")
  const [messages, setMessages] = useState<OpenCodeMessage[]>([])
  const [messageInput, setMessageInput] = useState("")
  const [chatState, setChatState] = useState<"idle" | "sending">("idle")
  const [activeView, setActiveView] = useState<"files" | "terminal" | "preview">("files")
  const [status, setStatus] = useState<string>("")
  const copy = useMemo(
    () => ({
      de: {
        title: "CHAT",
        sessionLabel: "Session",
        refresh: "Aktualisieren",
        noMessages: "Noch keine Nachrichten.",
        inputPlaceholder: "Nachricht eingeben...",
        send: "Senden",
        files: "Dateistruktur",
        terminal: "Terminal",
        preview: "Doc Vorschau",
        noSessions: "Keine Sessions verfügbar",
        statusPrefix: "Aktualisiert",
        configNotice: "NEXT_PUBLIC_OPENCODE_BASE_URL konfigurieren, um das Control Plane zu verbinden.",
      },
      en: {
        title: "CHAT",
        sessionLabel: "Session",
        refresh: "Refresh",
        noMessages: "No messages yet.",
        inputPlaceholder: "Type a message...",
        send: "Send",
        files: "Files",
        terminal: "Terminal",
        preview: "Doc Preview",
        noSessions: "No sessions available",
        statusPrefix: "Updated",
        configNotice: "Configure NEXT_PUBLIC_OPENCODE_BASE_URL to connect to the control plane.",
      },
    }),
    [],
  )
  const t = copy[locale]

  const loadSessions = useCallback(async () => {
    if (!hasOpencode) return
    try {
      const data = await fetchSessions()
      setSessions(data)
      if (!selectedSessionId && data.length > 0) {
        setSelectedSessionId(data[0].id)
      }
    } catch (err) {
      setStatus((err as Error).message)
    }
  }, [selectedSessionId])

  const loadMessages = useCallback(async () => {
    if (!hasOpencode || !selectedSessionId) return
    try {
      const data = await fetchMessages(selectedSessionId)
      setMessages(data)
    } catch (err) {
      setStatus((err as Error).message)
    }
  }, [selectedSessionId])

  useEffect(() => {
    loadSessions()
  }, [loadSessions])

  useEffect(() => {
    loadMessages()
  }, [loadMessages])

  useEffect(() => {
    if (!hasOpencode) return
    const unsubscribe = subscribeToEvents((event) => {
      if (event.type?.startsWith("session")) {
        loadSessions()
      }
      if (event.type?.startsWith("message")) {
        loadMessages()
      }
    })
    return unsubscribe
  }, [loadMessages, loadSessions])

  const selectedSession = useMemo(
    () => sessions.find((session) => session.id === selectedSessionId),
    [sessions, selectedSessionId],
  )

  const handleSend = async () => {
    if (!selectedSessionId || !messageInput.trim()) return
    setChatState("sending")
    try {
      await sendMessage(selectedSessionId, messageInput.trim())
      setMessageInput("")
      await loadMessages()
    } catch (err) {
      setStatus((err as Error).message)
    } finally {
      setChatState("idle")
    }
  }

  if (!hasOpencode) {
    return (
      <div className="p-6 text-sm text-muted-foreground bg-[#161c1a] border border-[#1f2a27] rounded-xl">
        {t.configNotice} <code className="font-semibold">NEXT_PUBLIC_OPENCODE_BASE_URL</code>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4 h-full min-h-0">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-[#d5f0e4] tracking-wider">{t.title}</h1>
          {selectedSession && (
            <p className="text-sm text-muted-foreground">
              {selectedSession.title || selectedSession.id} • {t.statusPrefix} {new Date(selectedSession.updated).toLocaleString()}
            </p>
          )}
        </div>
        {status && <span className="text-xs text-destructive">{status}</span>}
      </div>

      <div className="flex flex-1 min-h-0 gap-4">
        <div className="flex-1 bg-[#161c1a] border border-[#1f2a27] rounded-xl p-6 flex flex-col gap-4 min-h-0">
          <div className="flex flex-wrap items-center gap-3">
            <label className="text-xs uppercase tracking-wide text-muted-foreground">Session</label>
            <select
              className="bg-[#0f1412] border border-[#1f2a27] text-sm text-[#d5f0e4] rounded-md px-3 py-2 outline-none focus:border-[#3ba77c]"
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
            <Button variant="outline" size="sm" onClick={loadSessions} className="gap-2 text-muted-foreground hover:text-foreground">
              <RefreshCw className="w-4 h-4" /> {t.refresh}
            </Button>
          </div>

          <div className="flex-1 rounded-lg bg-[#0f1412] border border-[#1f2a27] p-4 overflow-y-auto space-y-4 min-h-0">
            {messages.length === 0 && <div className="text-sm text-muted-foreground">{t.noMessages}</div>}
            {messages.map((msg) => (
              <div key={msg.id} className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}>
                <div
                  className={`max-w-[80%] rounded-lg px-4 py-3 text-sm whitespace-pre-wrap ${
                    msg.role === "user"
                      ? "bg-[#193026] text-[#e3f6ed] border border-[#2d5c47]"
                      : "bg-[#141a18] text-[#d5f0e4] border border-[#1f2a27]"
                  }`}
                >
                  {msg.content || msg.parts?.map((part) => part.text).join("\n")}
                </div>
              </div>
            ))}
          </div>

          <div className="flex items-center gap-3">
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
              <Send className="w-4 h-4 mr-2" />
              {t.send}
            </Button>
          </div>
        </div>

        <div className="w-[360px] shrink-0 bg-[#161c1a] border border-[#1f2a27] rounded-xl flex flex-col min-h-0">
          <div className="flex gap-2 p-3 border-b border-[#1f2a27]">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setActiveView("files")}
              className={`flex-1 justify-center rounded-md ${
                activeView === "files"
                  ? "bg-[#1b2d26] text-[#d5f0e4] border border-[#3ba77c]"
                  : "text-[#9aa8a3] border border-transparent hover:border-[#264036] hover:bg-[#131a17]"
              }`}
            >
              <FileText className="w-4 h-4 mr-2" /> {t.files}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setActiveView("terminal")}
              className={`flex-1 justify-center rounded-md ${
                activeView === "terminal"
                  ? "bg-[#1b2d26] text-[#d5f0e4] border border-[#3ba77c]"
                  : "text-[#9aa8a3] border border-transparent hover:border-[#264036] hover:bg-[#131a17]"
              }`}
            >
              <Terminal className="w-4 h-4 mr-2" /> {t.terminal}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setActiveView("preview")}
              className={`flex-1 justify-center rounded-md ${
                activeView === "preview"
                  ? "bg-[#1b2d26] text-[#d5f0e4] border border-[#3ba77c]"
                  : "text-[#9aa8a3] border border-transparent hover:border-[#264036] hover:bg-[#131a17]"
              }`}
            >
              <Eye className="w-4 h-4 mr-2" /> {t.preview}
            </Button>
          </div>
          <div className="flex-1 min-h-0 overflow-hidden">
            {activeView === "files" && <FileTreeView />}
            {activeView === "terminal" && <TerminalView />}
            {activeView === "preview" && <PreviewView />}
          </div>
        </div>
      </div>
    </div>
  )
}

export default SessionsApp
