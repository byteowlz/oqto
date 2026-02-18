import { WebSocketServer, WebSocket } from "ws";
import type { BrowserManager, ScreencastFrame, ScreencastOptions } from "./browser.js";

export type FrameMessage = {
  type: "frame";
  data: string;
  metadata: ScreencastFrame["metadata"];
};

export type StatusMessage = {
  type: "status";
  connected: boolean;
  screencasting: boolean;
  viewportWidth?: number;
  viewportHeight?: number;
};

export type ErrorMessage = {
  type: "error";
  message: string;
};

export type InputMouseMessage = {
  type: "input_mouse";
  eventType: "mousePressed" | "mouseReleased" | "mouseMoved" | "mouseWheel";
  x: number;
  y: number;
  button?: "left" | "right" | "middle" | "none";
  clickCount?: number;
  deltaX?: number;
  deltaY?: number;
  modifiers?: number;
};

export type InputKeyboardMessage = {
  type: "input_keyboard";
  eventType: "keyDown" | "keyUp" | "char";
  key?: string;
  code?: string;
  text?: string;
  keyCode?: number;
  modifiers?: number;
};

export type InputTouchMessage = {
  type: "input_touch";
  eventType: "touchStart" | "touchEnd" | "touchMove" | "touchCancel";
  touchPoints: Array<{ x: number; y: number; id?: number }>;
  modifiers?: number;
};

export type StreamMessage =
  | FrameMessage
  | StatusMessage
  | ErrorMessage
  | InputMouseMessage
  | InputKeyboardMessage
  | InputTouchMessage;

/**
 * Check whether a WebSocket connection origin should be allowed.
 * Allows: no origin (CLI tools), file:// origins, and localhost/loopback origins.
 * Rejects: all other origins (prevents malicious web pages from connecting).
 */
export function isAllowedOrigin(origin: string | undefined): boolean {
  if (!origin) return true;
  if (origin.startsWith("file://")) return true;
  try {
    const url = new URL(origin);
    const host = url.hostname;
    if (host === "localhost" || host === "127.0.0.1" || host === "::1" || host === "[::1]") {
      return true;
    }
  } catch {
    // Invalid origin URL - reject
  }
  return false;
}

export class StreamServer {
  private wss: WebSocketServer | null = null;
  private clients = new Set<WebSocket>();
  private browser: BrowserManager;
  private port: number;
  private screencastOptions: ScreencastOptions;
  private isScreencasting = false;

  constructor(browser: BrowserManager, port: number, options: ScreencastOptions) {
    this.browser = browser;
    this.port = port;
    this.screencastOptions = options;
  }

  start(): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        this.wss = new WebSocketServer({
          port: this.port,
          verifyClient: (info: {
            origin: string;
            secure: boolean;
            req: import("http").IncomingMessage;
          }) => {
            if (isAllowedOrigin(info.origin)) {
              return true;
            }
            console.log(`[oqto-browserd] Rejected connection from origin: ${info.origin}`);
            return false;
          },
        });

        this.wss.on("connection", (ws) => this.handleConnection(ws));
        this.wss.on("error", (error) => {
          console.error("[oqto-browserd] stream server error:", error);
          reject(error);
        });
        this.wss.on("listening", () => {
          console.log(`[oqto-browserd] Stream server listening on port ${this.port}`);
          resolve();
        });
      } catch (error) {
        reject(error);
      }
    });
  }

  async stop(): Promise<void> {
    if (this.isScreencasting) {
      await this.stopScreencast();
    }

    for (const client of this.clients) {
      client.close();
    }
    this.clients.clear();

    if (!this.wss) {
      return;
    }

    await new Promise<void>((resolve) => {
      this.wss!.close(() => resolve());
    });
    this.wss = null;
  }

  getPort(): number {
    return this.port;
  }

  getClientCount(): number {
    return this.clients.size;
  }

  broadcastFrame(frame: ScreencastFrame): void {
    const payload: FrameMessage = {
      type: "frame",
      data: frame.data,
      metadata: frame.metadata,
    };
    const json = JSON.stringify(payload);

    for (const client of this.clients) {
      if (client.readyState === WebSocket.OPEN) {
        client.send(json);
      }
    }
  }

  private handleConnection(ws: WebSocket): void {
    console.log("[oqto-browserd] Client connected");
    this.clients.add(ws);
    this.sendStatus(ws);

    if (this.clients.size === 1 && !this.isScreencasting) {
      this.startScreencast().catch((error) => {
        console.error("[oqto-browserd] Failed to start screencast:", error);
        this.sendError(ws, error.message);
      });
    }

    ws.on("message", (data) => {
      try {
        const message = JSON.parse(data.toString()) as StreamMessage;
        this.handleMessage(message, ws).catch((error) => {
          this.sendError(ws, error.message);
        });
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        this.sendError(ws, message);
      }
    });

    ws.on("close", () => {
      console.log("[oqto-browserd] Client disconnected");
      this.clients.delete(ws);
      if (this.clients.size === 0 && this.isScreencasting) {
        this.stopScreencast().catch(() => undefined);
      }
    });

    ws.on("error", (error) => {
      console.error("[oqto-browserd] Client error:", error);
      this.clients.delete(ws);
    });
  }

  private async handleMessage(message: StreamMessage, ws: WebSocket): Promise<void> {
    switch (message.type) {
      case "input_mouse":
        await this.browser.injectMouseEvent({
          type: message.eventType,
          x: message.x,
          y: message.y,
          button: message.button,
          clickCount: message.clickCount,
          deltaX: message.deltaX,
          deltaY: message.deltaY,
          modifiers: message.modifiers,
        });
        break;
      case "input_keyboard":
        await this.browser.injectKeyboardEvent({
          type: message.eventType,
          key: message.key,
          code: message.code,
          text: message.text,
          keyCode: message.keyCode,
          modifiers: message.modifiers,
        });
        break;
      case "input_touch":
        await this.browser.injectTouchEvent({
          type: message.eventType,
          touchPoints: message.touchPoints,
          modifiers: message.modifiers,
        });
        break;
      case "status":
        this.sendStatus(ws);
        break;
      default:
        break;
    }
  }

  private sendStatus(ws: WebSocket): void {
    let viewportWidth: number | undefined;
    let viewportHeight: number | undefined;

    try {
      const viewport = this.browser.getViewportSize();
      viewportWidth = viewport?.width;
      viewportHeight = viewport?.height;
    } catch {
      // ignore
    }

    const payload: StatusMessage = {
      type: "status",
      connected: true,
      screencasting: this.isScreencasting,
      viewportWidth,
      viewportHeight,
    };

    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(payload));
    }
  }

  private sendError(ws: WebSocket, message: string): void {
    const payload: ErrorMessage = { type: "error", message };
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(payload));
    }
  }

  private async startScreencast(): Promise<void> {
    if (this.isScreencasting) {
      return;
    }

    this.isScreencasting = true;
    try {
      if (!this.browser.isLaunched()) {
        throw new Error("Browser not launched");
      }

      await this.browser.startScreencast(
        (frame) => this.broadcastFrame(frame),
        this.screencastOptions,
      );
      for (const client of this.clients) {
        this.sendStatus(client);
      }
    } catch (error) {
      this.isScreencasting = false;
      throw error;
    }
  }

  private async stopScreencast(): Promise<void> {
    if (!this.isScreencasting) {
      return;
    }

    await this.browser.stopScreencast();
    this.isScreencasting = false;
    for (const client of this.clients) {
      this.sendStatus(client);
    }
  }
}
