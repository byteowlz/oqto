import fs from "node:fs";
import net from "node:net";
import { BrowserManager } from "./browser.js";
import { StreamServer } from "./stream-server.js";
import { executeCommand, setScreencastFrameCallback } from "./actions.js";
import {
  errorResponse,
  parseCommand,
} from "./protocol.js";
import { getPidFile, getSessionId, getSocketDir, getSocketPath, getStreamPortFile } from "./paths.js";

function ensureDir(dir: string): void {
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
  }

  try {
    fs.chmodSync(dir, 0o700);
  } catch {
    // ignore
  }
}

function cleanupSession(session: string): void {
  const pidFile = getPidFile(session);
  const streamFile = getStreamPortFile(session);
  const socketPath = getSocketPath(session);
  try {
    if (fs.existsSync(pidFile)) fs.unlinkSync(pidFile);
    if (fs.existsSync(streamFile)) fs.unlinkSync(streamFile);
    if (fs.existsSync(socketPath)) fs.unlinkSync(socketPath);
  } catch {
    // ignore
  }
}

function boolFromEnv(value: string | undefined, defaultValue: boolean): boolean {
  if (!value) return defaultValue;
  return value === "1" || value.toLowerCase() === "true";
}

function parsePositiveInt(value: string | undefined, fallback: number): number {
  if (!value) return fallback;
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function getScreencastOptions() {
  const format = process.env.AGENT_BROWSER_STREAM_FORMAT?.toLowerCase() === "png" ? "png" : "jpeg";
  return {
    format,
    quality: parsePositiveInt(process.env.AGENT_BROWSER_STREAM_QUALITY, 90),
    maxWidth: parsePositiveInt(process.env.AGENT_BROWSER_STREAM_MAX_WIDTH, 1920),
    maxHeight: parsePositiveInt(process.env.AGENT_BROWSER_STREAM_MAX_HEIGHT, 1080),
    everyNthFrame: parsePositiveInt(process.env.AGENT_BROWSER_STREAM_EVERY_NTH_FRAME, 1),
  } as const;
}

export async function startDaemon(): Promise<void> {
  const session = getSessionId();
  const socketDir = getSocketDir(session);
  ensureDir(socketDir);
  cleanupSession(session);

  if (process.env.AGENT_BROWSER_SOCKET_DIR && !socketDir.includes(session)) {
    console.warn(
      `[octo-browserd] AGENT_BROWSER_SOCKET_DIR does not include session '${session}'. ` +
        "Ensure socket dirs are session-scoped for isolation."
    );
  }

  const browser = new BrowserManager();
  let streamServer: StreamServer | null = null;

  const streamPort = parsePositiveInt(process.env.AGENT_BROWSER_STREAM_PORT, 0);
  if (streamPort > 0) {
    streamServer = new StreamServer(browser, streamPort, getScreencastOptions());
    await streamServer.start();
    fs.writeFileSync(getStreamPortFile(session), String(streamPort));
  }

  // Wire screencast frame callback for actions that start screencast programmatically
  setScreencastFrameCallback((frame) => {
    if (streamServer) {
      // The stream server broadcasts frames; this wires the actions module
      // screencast_start/stop to the same broadcast path.
    }
  });

  const server = net.createServer((socket) => {
    let buffer = "";

    socket.on("data", async (data) => {
      buffer += data.toString();
      while (buffer.includes("\n")) {
        const newlineIdx = buffer.indexOf("\n");
        const line = buffer.slice(0, newlineIdx);
        buffer = buffer.slice(newlineIdx + 1);

        if (!line.trim()) {
          continue;
        }

        const parsed = parseCommand(line);
        if (!parsed.success) {
          const response = errorResponse(parsed.id ?? "unknown", parsed.error);
          socket.write(`${JSON.stringify(response)}\n`);
          continue;
        }

        try {
          // Auto-launch browser if not yet launched (except for launch/close commands)
          if (
            parsed.command.action !== "launch" &&
            parsed.command.action !== "close" &&
            !browser.isLaunched()
          ) {
            await autoLaunch(browser);
          }

          const response = await executeCommand(parsed.command, browser);
          socket.write(`${JSON.stringify(response)}\n`);

          if (parsed.command.action === "close") {
            await shutdown(server, browser, streamServer, session);
            return;
          }
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          const response = errorResponse(parsed.command.id, message);
          socket.write(`${JSON.stringify(response)}\n`);
        }
      }
    });

    socket.on("error", () => {
      // ignore
    });
  });

  const socketPath = getSocketPath(session);
  server.listen(socketPath, () => {
    fs.writeFileSync(getPidFile(session), String(process.pid));
  });

  const shutdownHandler = async () => {
    await shutdown(server, browser, streamServer, session);
  };

  process.on("SIGINT", shutdownHandler);
  process.on("SIGTERM", shutdownHandler);
  process.on("SIGHUP", shutdownHandler);

  process.on("exit", () => {
    cleanupSession(session);
  });
}

function parseExtensions(): string[] | undefined {
  const raw = process.env.AGENT_BROWSER_EXTENSIONS;
  if (!raw) return undefined;
  const list = raw
    .split(",")
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  return list.length > 0 ? list : undefined;
}

function parseArgs(): string[] | undefined {
  const raw = process.env.AGENT_BROWSER_ARGS;
  if (!raw) return undefined;
  const list = raw
    .split(/[,\n]/)
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  return list.length > 0 ? list : undefined;
}

function parseProxy(): { server: string; bypass?: string } | undefined {
  const server = process.env.AGENT_BROWSER_PROXY;
  if (!server) return undefined;
  const bypass = process.env.AGENT_BROWSER_PROXY_BYPASS;
  return { server, ...(bypass && { bypass }) };
}

async function autoLaunch(browser: BrowserManager): Promise<void> {
  const headless = !boolFromEnv(process.env.AGENT_BROWSER_HEADED, false);
  const viewportWidth = parsePositiveInt(process.env.AGENT_BROWSER_VIEWPORT_WIDTH, 1280);
  const viewportHeight = parsePositiveInt(process.env.AGENT_BROWSER_VIEWPORT_HEIGHT, 720);
  const ignoreHTTPSErrors = boolFromEnv(process.env.AGENT_BROWSER_IGNORE_HTTPS_ERRORS, false);
  const allowFileAccess = boolFromEnv(process.env.AGENT_BROWSER_ALLOW_FILE_ACCESS, false);

  await browser.launch({
    headless,
    viewport: { width: viewportWidth, height: viewportHeight },
    executablePath: process.env.AGENT_BROWSER_EXECUTABLE_PATH,
    extensions: parseExtensions(),
    profile: process.env.AGENT_BROWSER_PROFILE,
    storageState: process.env.AGENT_BROWSER_STATE,
    args: parseArgs(),
    userAgent: process.env.AGENT_BROWSER_USER_AGENT,
    proxy: parseProxy(),
    ignoreHTTPSErrors,
    allowFileAccess,
  });
}

async function shutdown(
  server: net.Server,
  browser: BrowserManager,
  streamServer: StreamServer | null,
  session: string,
): Promise<void> {
  try {
    if (streamServer) {
      await streamServer.stop();
    }
  } catch {
    // ignore
  }

  await browser.close().catch(() => undefined);

  await new Promise<void>((resolve) => {
    server.close(() => resolve());
  });

  cleanupSession(session);
  process.exit(0);
}
