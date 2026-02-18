import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { startDaemon } from "./daemon.js";
import { getPidFile, getSessionId, getSocketDir, getSocketPath } from "./paths.js";

const DAEMON_ENV_KEY = "OQTO_BROWSER_DAEMON";

type CliOptions = {
  session: string;
  headed: boolean;
  executablePath?: string;
  extensions: string[];
};

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const isDaemon = args.includes("--daemon") || process.env[DAEMON_ENV_KEY] === "1";

  if (isDaemon) {
    await startDaemon();
    return;
  }

  const { options, command, commandArgs } = parseArgs(args);
  applyOptions(options);

  await ensureDaemonRunning(options.session);

  if (!command) {
    throw new Error("No command provided");
  }

  const payload = buildPayload(command, commandArgs);
  const response = await sendCommand(options.session, payload);
  process.stdout.write(`${JSON.stringify(response, null, 2)}\n`);

  if (!response.success) {
    process.exitCode = 1;
  }
}

function parseArgs(args: string[]): { options: CliOptions; command?: string; commandArgs: string[] } {
  const options: CliOptions = {
    session: process.env.AGENT_BROWSER_SESSION || "default",
    headed: false,
    extensions: [],
  };

  const remaining: string[] = [];
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "--session") {
      options.session = args[i + 1] ?? options.session;
      i += 1;
      continue;
    }
    if (arg === "--headed") {
      options.headed = true;
      continue;
    }
    if (arg === "--executable-path") {
      options.executablePath = args[i + 1];
      i += 1;
      continue;
    }
    if (arg === "--extension") {
      const value = args[i + 1];
      if (value) {
        options.extensions.push(value);
      }
      i += 1;
      continue;
    }
    if (arg === "--daemon") {
      continue;
    }
    remaining.push(arg);
  }

  const command = remaining.shift();
  return { options, command, commandArgs: remaining };
}

function applyOptions(options: CliOptions): void {
  process.env.AGENT_BROWSER_SESSION = options.session;
  if (options.headed) {
    process.env.AGENT_BROWSER_HEADED = "1";
  }
  if (options.executablePath) {
    process.env.AGENT_BROWSER_EXECUTABLE_PATH = options.executablePath;
  }
  if (options.extensions.length > 0) {
    process.env.AGENT_BROWSER_EXTENSIONS = options.extensions.join(",");
  }
}

function buildPayload(command: string, args: string[]): Record<string, unknown> {
  const id = randomUUID();
  switch (command) {
    // --- Navigation ---
    case "open":
    case "navigate":
      return { id, action: "navigate", url: args[0] };
    case "back":
    case "forward":
    case "reload":
    case "close":
      return { id, action: command };
    case "url":
      return { id, action: "url" };
    case "title":
      return { id, action: "title" };

    // --- Viewport ---
    case "set": {
      const [subcommand, width, height] = args;
      if (subcommand !== "viewport") {
        throw new Error("Unsupported set command");
      }
      return {
        id,
        action: "viewport",
        width: Number.parseInt(width ?? "0", 10),
        height: Number.parseInt(height ?? "0", 10),
      };
    }
    case "viewport":
      return {
        id,
        action: "viewport",
        width: Number.parseInt(args[0] ?? "0", 10),
        height: Number.parseInt(args[1] ?? "0", 10),
      };

    // --- Snapshot / content ---
    case "snapshot":
      return { id, action: "snapshot", interactive: args.includes("-i"), cursor: args.includes("-c"), compact: args.includes("--compact") };
    case "content":
      return { id, action: "content", selector: args[0] };
    case "screenshot":
      return { id, action: "screenshot", path: args[0], fullPage: args.includes("--fullpage") };
    case "pdf":
      return { id, action: "pdf", path: args[0] };

    // --- Interaction ---
    case "click":
      return { id, action: "click", selector: args[0] };
    case "dblclick":
      return { id, action: "dblclick", selector: args[0] };
    case "fill":
      return { id, action: "fill", selector: args[0], value: args[1] };
    case "type":
      return { id, action: "type", selector: args[0], text: args.slice(1).join(" ") };
    case "press":
      return { id, action: "press", key: args[0], selector: args[1] };
    case "check":
      return { id, action: "check", selector: args[0] };
    case "uncheck":
      return { id, action: "uncheck", selector: args[0] };
    case "upload":
      return { id, action: "upload", selector: args[0], files: args.slice(1) };
    case "focus":
      return { id, action: "focus", selector: args[0] };
    case "hover":
      return { id, action: "hover", selector: args[0] };
    case "drag":
      return { id, action: "drag", source: args[0], target: args[1] };
    case "select":
      return { id, action: "select", selector: args[0], values: args.slice(1) };
    case "clear":
      return { id, action: "clear", selector: args[0] };
    case "selectall":
      return { id, action: "selectall", selector: args[0] };
    case "highlight":
      return { id, action: "highlight", selector: args[0] };
    case "scrollintoview":
      return { id, action: "scrollintoview", selector: args[0] };
    case "tap":
      return { id, action: "tap", selector: args[0] };

    // --- Scroll ---
    case "scroll": {
      const dirArg = args[0];
      if (["up", "down", "left", "right"].includes(dirArg)) {
        return { id, action: "scroll", direction: dirArg, amount: Number.parseInt(args[1] ?? "100", 10) };
      }
      return { id, action: "scroll", selector: args[0], y: Number.parseInt(args[1] ?? "0", 10) };
    }

    // --- Evaluate ---
    case "eval":
    case "evaluate":
      return { id, action: "evaluate", script: args.join(" ") };
    case "evalhandle":
      return { id, action: "evalhandle", script: args.join(" ") };

    // --- Wait ---
    case "wait": {
      if (args.length === 0) return { id, action: "wait" };
      const maybeTimeout = Number.parseInt(args[0], 10);
      if (!Number.isNaN(maybeTimeout) && args.length === 1) {
        return { id, action: "wait", timeout: maybeTimeout };
      }
      return { id, action: "wait", selector: args[0] };
    }
    case "waitforurl":
      return { id, action: "waitforurl", url: args[0] };
    case "waitforloadstate":
      return { id, action: "waitforloadstate", state: args[0] };
    case "waitforfunction":
      return { id, action: "waitforfunction", expression: args.join(" ") };
    case "waitfordownload":
      return { id, action: "waitfordownload", path: args[0] };

    // --- Element queries ---
    case "getattribute":
      return { id, action: "getattribute", selector: args[0], attribute: args[1] };
    case "gettext":
      return { id, action: "gettext", selector: args[0] };
    case "innertext":
      return { id, action: "innertext", selector: args[0] };
    case "innerhtml":
      return { id, action: "innerhtml", selector: args[0] };
    case "inputvalue":
      return { id, action: "inputvalue", selector: args[0] };
    case "setvalue":
      return { id, action: "setvalue", selector: args[0], value: args[1] };
    case "isvisible":
      return { id, action: "isvisible", selector: args[0] };
    case "isenabled":
      return { id, action: "isenabled", selector: args[0] };
    case "ischecked":
      return { id, action: "ischecked", selector: args[0] };
    case "count":
      return { id, action: "count", selector: args[0] };
    case "boundingbox":
      return { id, action: "boundingbox", selector: args[0] };
    case "styles":
      return { id, action: "styles", selector: args[0] };

    // --- Frames ---
    case "frame":
      return { id, action: "frame", selector: args[0] };
    case "mainframe":
      return { id, action: "mainframe" };

    // --- Tabs ---
    case "tab_new":
      return { id, action: "tab_new", url: args[0] };
    case "tab_list":
      return { id, action: "tab_list" };
    case "tab_switch":
      return { id, action: "tab_switch", index: Number.parseInt(args[0] ?? "0", 10) };
    case "tab_close":
      return { id, action: "tab_close", index: args[0] !== undefined ? Number.parseInt(args[0], 10) : undefined };
    case "window_new":
      return { id, action: "window_new" };

    // --- Cookies ---
    case "cookies_get":
      return { id, action: "cookies_get" };
    case "cookies_set":
      return { id, action: "cookies_set", cookies: JSON.parse(args[0] ?? "[]") };
    case "cookies_clear":
      return { id, action: "cookies_clear" };

    // --- Storage ---
    case "storage_get":
      return { id, action: "storage_get", type: args[0] ?? "local", key: args[1] };
    case "storage_set":
      return { id, action: "storage_set", type: args[0] ?? "local", key: args[1], value: args[2] };
    case "storage_clear":
      return { id, action: "storage_clear", type: args[0] ?? "local" };
    case "storage-state":
      return { id, action: "storage_state" };
    case "storage-state-set":
      return { id, action: "storage_state_set", storageState: args[0] };
    case "state_save":
    case "state-save":
      return { id, action: "state_save", path: args[0] };
    case "state_load":
    case "state-load":
      return { id, action: "state_load", path: args[0] };

    // --- Dialog ---
    case "dialog":
      return { id, action: "dialog", response: args[0] ?? "accept", promptText: args[1] };

    // --- Network ---
    case "route":
      return { id, action: "route", url: args[0], abort: args.includes("--abort") };
    case "unroute":
      return { id, action: "unroute", url: args[0] };
    case "requests":
      return { id, action: "requests", filter: args[0], clear: args.includes("--clear") };
    case "download":
      return { id, action: "download", selector: args[0], path: args[1] };
    case "responsebody":
      return { id, action: "responsebody", url: args[0] };
    case "headers":
      return { id, action: "headers", headers: JSON.parse(args[0] ?? "{}") };

    // --- Emulation ---
    case "geolocation":
      return { id, action: "geolocation", latitude: Number.parseFloat(args[0] ?? "0"), longitude: Number.parseFloat(args[1] ?? "0") };
    case "permissions":
      return { id, action: "permissions", permissions: args.slice(0, -1), grant: args[args.length - 1] !== "false" };
    case "device":
      return { id, action: "device", device: args.join(" ") };
    case "emulatemedia":
      return { id, action: "emulatemedia", colorScheme: args[0] as "light" | "dark" | undefined };
    case "offline":
      return { id, action: "offline", offline: args[0] !== "false" };
    case "timezone":
      return { id, action: "timezone", timezone: args[0] };
    case "locale":
      return { id, action: "locale", locale: args[0] };
    case "credentials":
      return { id, action: "credentials", username: args[0], password: args[1] };

    // --- Console / errors ---
    case "console":
      return { id, action: "console", clear: args.includes("--clear") };
    case "errors":
      return { id, action: "errors", clear: args.includes("--clear") };

    // --- Keyboard / mouse ---
    case "keyboard":
      return { id, action: "keyboard", keys: args.join("+") };
    case "keydown":
      return { id, action: "keydown", key: args[0] };
    case "keyup":
      return { id, action: "keyup", key: args[0] };
    case "inserttext":
      return { id, action: "inserttext", text: args.join(" ") };
    case "mousemove":
      return { id, action: "mousemove", x: Number.parseFloat(args[0] ?? "0"), y: Number.parseFloat(args[1] ?? "0") };
    case "mousedown":
      return { id, action: "mousedown", button: args[0] as "left" | "right" | "middle" | undefined };
    case "mouseup":
      return { id, action: "mouseup", button: args[0] as "left" | "right" | "middle" | undefined };
    case "wheel":
      return { id, action: "wheel", deltaX: Number.parseFloat(args[0] ?? "0"), deltaY: Number.parseFloat(args[1] ?? "0") };

    // --- Clipboard ---
    case "clipboard":
      return { id, action: "clipboard", operation: args[0] ?? "read" };

    // --- Script / style ---
    case "addscript":
      return { id, action: "addscript", content: args.join(" ") };
    case "addstyle":
      return { id, action: "addstyle", content: args.join(" ") };
    case "addinitscript":
      return { id, action: "addinitscript", script: args.join(" ") };

    // --- Recording ---
    case "recording_start":
    case "record_start":
      return { id, action: "recording_start", path: args[0], url: args[1] };
    case "recording_stop":
    case "record_stop":
      return { id, action: "recording_stop" };
    case "recording_restart":
    case "record_restart":
      return { id, action: "recording_restart", path: args[0], url: args[1] };

    // --- Tracing / HAR ---
    case "trace_start":
      return { id, action: "trace_start" };
    case "trace_stop":
      return { id, action: "trace_stop", path: args[0] };
    case "har_start":
      return { id, action: "har_start" };
    case "har_stop":
      return { id, action: "har_stop", path: args[0] };

    // --- Misc ---
    case "bringtofront":
      return { id, action: "bringtofront" };
    case "pause":
      return { id, action: "pause" };
    case "dispatch":
      return { id, action: "dispatch", selector: args[0], event: args[1] };
    case "setcontent":
      return { id, action: "setcontent", html: args.join(" ") };

    default:
      throw new Error(`Unsupported command: ${command}`);
  }
}

async function ensureDaemonRunning(session: string): Promise<void> {
  if (isDaemonRunning(session)) {
    return;
  }

  const nodePath = process.execPath;
  const scriptPath = resolveSelf();
  const env = {
    ...process.env,
    [DAEMON_ENV_KEY]: "1",
    AGENT_BROWSER_SESSION: session,
  };

  const child = spawn(nodePath, [scriptPath, "--daemon"], {
    env,
    detached: true,
    stdio: "ignore",
  });
  child.unref();

  await waitForSocket(session, 5000);
}

function resolveSelf(): string {
  const filePath = fileURLToPath(import.meta.url);
  return path.resolve(filePath);
}

function isDaemonRunning(session: string): boolean {
  const pidFile = getPidFile(session);
  if (!fs.existsSync(pidFile)) {
    return false;
  }

  try {
    const pid = Number.parseInt(fs.readFileSync(pidFile, "utf8").trim(), 10);
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

async function waitForSocket(session: string, timeoutMs: number): Promise<void> {
  const socketPath = getSocketPath(session);
  const socketDir = getSocketDir(session);
  if (!fs.existsSync(socketDir)) {
    fs.mkdirSync(socketDir, { recursive: true, mode: 0o700 });
  }

  try {
    fs.chmodSync(socketDir, 0o700);
  } catch {
    // ignore
  }

  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (fs.existsSync(socketPath)) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }

  throw new Error("Timed out waiting for daemon socket");
}

async function sendCommand(session: string, payload: Record<string, unknown>): Promise<any> {
  const socketPath = getSocketPath(session);
  const line = `${JSON.stringify(payload)}\n`;

  return new Promise((resolve, reject) => {
    const socket = net.createConnection(socketPath);
    let response = "";

    socket.on("connect", () => {
      socket.write(line);
    });

    socket.on("data", (data) => {
      response += data.toString();
      if (response.includes("\n")) {
        socket.end();
      }
    });

    socket.on("end", () => {
      const firstLine = response.split("\n")[0].trim();
      if (!firstLine) {
        reject(new Error("Empty response"));
        return;
      }
      try {
        resolve(JSON.parse(firstLine));
      } catch (error) {
        reject(error);
      }
    });

    socket.on("error", (error) => {
      reject(error);
    });
  });
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`${message}\n`);
  process.exit(1);
});
