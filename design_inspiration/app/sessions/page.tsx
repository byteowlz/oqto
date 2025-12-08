"use client"

import { useState } from "react"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Send, Paperclip, Terminal, FileText, Eye } from "lucide-react"
import { Input } from "@/components/ui/input"
import { FileTreeView } from "./FileTreeView"
import { TerminalView } from "./TerminalView"
import { PreviewView } from "./PreviewView"

export default function SessionsPage() {
  const [activeView, setActiveView] = useState<"files" | "terminal" | "preview">("files")
  const [message, setMessage] = useState("")

  const chatMessages = [
    { role: "assistant", content: "Hello! I'm your Coding Copilot. How can I help you today?" },
    { role: "user", content: "Can you review the authentication logic in auth.ts?" },
    {
      role: "assistant",
      content:
        "I'll analyze the authentication logic for you. Let me read the file first.\n\n✓ Read auth.ts\n\nI found a few potential improvements:\n1. Password hashing uses bcrypt - good!\n2. Consider adding rate limiting\n3. Session tokens should expire after 24h",
    },
  ]

  return (
    <div className="p-6 space-y-4 h-full flex flex-col bg-background">
      {/* Session Header */}
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-xl font-bold text-foreground tracking-wider">ACTIVE SESSION</h1>
          <p className="text-sm text-muted-foreground">API Integration Project • Coding Copilot</p>
        </div>
        <Button
          variant="outline"
          className="border-destructive text-destructive hover:bg-destructive/10 bg-transparent"
        >
          End Session
        </Button>
      </div>

      {/* Split View Container */}
      <div className="flex-1 grid grid-cols-1 lg:grid-cols-2 gap-4 min-h-0">
        {/* Left Panel - Chat */}
        <Card className="bg-card border-border flex flex-col">
          <CardHeader className="pb-3">
            <CardTitle className="text-sm font-medium text-muted-foreground tracking-wider">CHAT INTERFACE</CardTitle>
          </CardHeader>
          <CardContent className="flex-1 flex flex-col min-h-0">
            {/* Messages */}
            <div className="flex-1 overflow-y-auto space-y-4 mb-4">
              {chatMessages.map((msg, idx) => (
                <div key={idx} className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}>
                  <div
                    className={`max-w-[80%] p-3 rounded ${
                      msg.role === "user"
                        ? "bg-primary text-primary-foreground"
                        : "bg-muted text-foreground border border-border"
                    }`}
                  >
                    <pre className="text-sm whitespace-pre-wrap font-sans">{msg.content}</pre>
                  </div>
                </div>
              ))}
            </div>

            {/* Input */}
            <div className="flex gap-2">
              <Button variant="ghost" size="icon" className="text-muted-foreground hover:text-primary">
                <Paperclip className="w-4 h-4" />
              </Button>
              <Input
                placeholder="Type your message..."
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                className="flex-1 bg-background border-input text-foreground"
              />
              <Button className="bg-primary hover:bg-primary/90 text-primary-foreground">
                <Send className="w-4 h-4" />
              </Button>
            </div>
          </CardContent>
        </Card>

        {/* Right Panel - Dynamic Content */}
        <Card className="bg-card border-border flex flex-col">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm font-medium text-muted-foreground tracking-wider">WORKSPACE VIEW</CardTitle>
              <div className="flex gap-1">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setActiveView("files")}
                  className={activeView === "files" ? "text-primary" : "text-muted-foreground"}
                >
                  <FileText className="w-4 h-4 mr-1" />
                  Files
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setActiveView("terminal")}
                  className={activeView === "terminal" ? "text-primary" : "text-muted-foreground"}
                >
                  <Terminal className="w-4 h-4 mr-1" />
                  Terminal
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setActiveView("preview")}
                  className={activeView === "preview" ? "text-primary" : "text-muted-foreground"}
                >
                  <Eye className="w-4 h-4 mr-1" />
                  Preview
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
  )
}
