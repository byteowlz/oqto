"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { FileText, Terminal, Eye, Send, Paperclip, RefreshCw } from "lucide-react"
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
  const [sessions, setSessions] = useState<OpenCodeSession[]>([])
  const [selectedSessionId, setSelectedSessionId] = useState<string>("")
  const [messages, setMessages] = useState<OpenCodeMessage[]>([])
  const [messageInput, setMessageInput] = useState("")
  const [chatState, setChatState] = useState<"idle" | "sending">("idle")
  const [activeView, setActiveView] = useState<"files" | "terminal" | "preview">("files")
  const [status, setStatus] = useState<string>("")

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
      <div className="p-6 text-sm text-muted-foreground">
        Configure <code className="font-semibold">NEXT_PUBLIC_OPENCODE_BASE_URL</code> to connect to the control plane.
      </div>
    )
  }

  return (
    <div className="p-6 space-y-6 h-full flex flex-col bg-background">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground tracking-wider">ACTIVE SESSIONS</h1>
          {selectedSession && (
            <p className="text-sm text-muted-foreground">
              {selectedSession.title || selectedSession.id} • Updated {new Date(selectedSession.updated).toLocaleString()}
            </p>
          )}
        </div>
        <div className="flex items-center gap-2">
          {status && <span className="text-xs text-destructive">{status}</span>}
          <Button variant="ghost" size="sm" onClick={loadSessions} className="gap-2 text-muted-foreground">
            <RefreshCw className="w-4 h-4" /> Refresh
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-5 gap-4">
        <Card className="bg-card border-border lg:col-span-1">
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium text-muted-foreground tracking-wider">SESSIONS</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 max-h-[480px] overflow-y-auto">
            {sessions.map((session) => {
              const isActive = session.id === selectedSessionId
              return (
                <button
                  key={session.id}
                  onClick={() => setSelectedSessionId(session.id)}
                  className={`w-full text-left border rounded p-3 text-xs font-mono tracking-wide transition-colors ${
                    isActive ? "border-primary text-primary" : "border-border text-muted-foreground hover:border-primary"
                  }`}
                >
                  <div>{session.title || session.id}</div>
                  <div className="text-[10px] uppercase">
                    {new Date(session.updated).toLocaleTimeString()} • {session.id.slice(0, 8)}
                  </div>
                </button>
              )
            })}
            {sessions.length === 0 && (
              <div className="text-xs text-muted-foreground">No sessions available. Start one from the Workspaces app.</div>
            )}
          </CardContent>
        </Card>

        <div className="lg:col-span-4 grid grid-cols-1 gap-4">
          <Card className="bg-card border-border flex flex-col">
            <CardHeader className="pb-3">
              <CardTitle className="text-sm font-medium text-muted-foreground tracking-wider">CHAT INTERFACE</CardTitle>
            </CardHeader>
            <CardContent className="flex-1 flex flex-col min-h-0">
              <div className="flex-1 overflow-y-auto space-y-4 mb-4">
                {messages.length === 0 && (
                  <div className="text-sm text-muted-foreground">No messages streamed yet.</div>
                )}
                {messages.map((msg) => (
                  <div key={msg.id} className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}>
                    <div
                      className={`max-w-[80%] p-3 rounded text-sm whitespace-pre-wrap ${
                        msg.role === "user"
                          ? "bg-primary text-primary-foreground"
                          : "bg-muted text-foreground border border-border"
                      }`}
                    >
                      {msg.content || msg.parts?.map((part) => part.text).join("\n")}
                    </div>
                  </div>
                ))}
              </div>
              <div className="flex gap-2">
                <Button variant="ghost" size="icon" className="text-muted-foreground hover:text-primary">
                  <Paperclip className="w-4 h-4" />
                </Button>
                <Input
                  placeholder="Type your message..."
                  value={messageInput}
                  onChange={(e) => setMessageInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                      handleSend()
                    }
                  }}
                  className="flex-1 bg-background border-input text-foreground"
                />
                <Button
                  onClick={handleSend}
                  disabled={chatState === "sending"}
                  className="bg-primary hover:bg-primary/90 text-primary-foreground"
                >
                  <Send className="w-4 h-4" />
                </Button>
              </div>
            </CardContent>
          </Card>

          <Card className="bg-card border-border flex flex-col">
            <CardHeader className="pb-3">
              <div className="flex items-center justify-between">
                <CardTitle className="text-sm font-medium text-muted-foreground tracking-wider">
                  WORKSPACE VIEW
                </CardTitle>
                <div className="flex gap-1">
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setActiveView("files")}
                    className={activeView === "files" ? "text-primary" : "text-muted-foreground"}
                  >
                    <FileText className="w-4 h-4 mr-1" /> Files
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setActiveView("terminal")}
                    className={activeView === "terminal" ? "text-primary" : "text-muted-foreground"}
                  >
                    <Terminal className="w-4 h-4 mr-1" /> Terminal
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setActiveView("preview")}
                    className={activeView === "preview" ? "text-primary" : "text-muted-foreground"}
                  >
                    <Eye className="w-4 h-4 mr-1" /> Preview
                  </Button>
                </div>
              </div>
            </CardHeader>
            <CardContent className="flex-1 min-h-0">
              {activeView === "files" && <FileTreeView />}
              {activeView === "terminal" && <TerminalView />}
              {activeView === "preview" && <PreviewView />}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  )
}

export default SessionsApp
