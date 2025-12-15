"use client"

import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react"
import { Terminal, FitAddon, Ghostty } from "ghostty-web"

const ghosttyReady: Promise<Ghostty> = (Ghostty as unknown as { loadFromPath: (path: string) => Promise<Ghostty> }).loadFromPath(
  "/ghostty-vt.wasm",
)

export type GhosttyTerminalHandle = {
  focus: () => void
  blur: () => void
}

interface GhosttyTerminalProps {
  wsUrl: string
  fontFamily?: string
  fontSize?: number
  className?: string
}

export const GhosttyTerminal = forwardRef<GhosttyTerminalHandle, GhosttyTerminalProps>(
  ({ wsUrl, fontFamily = "JetBrains Mono", fontSize = 14, className }, ref) => {
    const containerRef = useRef<HTMLDivElement | null>(null)
    const terminalRef = useRef<Terminal | null>(null)
    const fitRef = useRef<FitAddon | null>(null)
    const socketRef = useRef<WebSocket | null>(null)
    const resizeObserver = useRef<ResizeObserver | null>(null)
    const [status, setStatus] = useState<"connecting" | "connected" | "error">("connecting")
    const decoderRef = useRef<TextDecoder | null>(null)

    useImperativeHandle(ref, () => ({
      focus: () => terminalRef.current?.focus(),
      blur: () => terminalRef.current?.blur(),
    }))

    useEffect(() => {
      let disposed = false

      async function bootstrap() {
        const ghostty = await ghosttyReady
        if (!decoderRef.current) {
          decoderRef.current = new TextDecoder()
        }
        if (disposed || !containerRef.current) return


        const terminal = new Terminal({
          ghostty,
          fontFamily,
          fontSize,
          cursorBlink: true,
          convertEol: true,
          theme: {
            background: "#0b0d12",
            foreground: "#f5f5f5",
          },
        })
        terminalRef.current = terminal
        const fitAddon = new FitAddon()
        fitRef.current = fitAddon
        terminal.loadAddon(fitAddon)
        terminal.open(containerRef.current)
        fitAddon.fit()
        resizeObserver.current = new ResizeObserver(() => {
          fitAddon.fit()
        })
        resizeObserver.current.observe(containerRef.current)

        terminal.onData((data) => {
          socketRef.current?.send(data)
        })

        connectWs()
      }

      function connectWs() {
        const socket = new WebSocket(wsUrl)
        socketRef.current = socket
        setStatus("connecting")
        socket.onopen = () => {
          setStatus("connected")
        }
        socket.onmessage = async (event) => {
          if (!terminalRef.current) return
          if (typeof event.data === "string") {
            terminalRef.current.write(event.data)
          } else if (event.data instanceof ArrayBuffer) {
            terminalRef.current.write(decoderRef.current?.decode(event.data) ?? "")
          } else if (event.data instanceof Blob) {
            const buffer = await event.data.arrayBuffer()
            terminalRef.current.write(decoderRef.current?.decode(buffer) ?? "")
          }
        }
        socket.onerror = (error) => {
          console.error("terminal websocket error", error)
          setStatus("error")
        }
        socket.onclose = () => {
          setStatus("error")
        }
      }

      bootstrap()

      return () => {
        disposed = true
        socketRef.current?.close()
        terminalRef.current?.dispose()
        resizeObserver.current?.disconnect()
        fitRef.current?.dispose()
      }
    }, [wsUrl, fontFamily, fontSize])

    return (
      <div className={`relative h-full w-full bg-black rounded ${className ?? ""}`}>
        <div ref={containerRef} className="h-full w-full" />
        <div className="absolute top-2 right-2 text-xs font-mono text-white/60">
          {status === "connecting" && "Connecting"}
          {status === "connected" && "Connected"}
          {status === "error" && "Disconnected"}
        </div>
      </div>
    )
  },
)

GhosttyTerminal.displayName = "GhosttyTerminal"
