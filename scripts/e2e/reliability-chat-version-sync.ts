#!/usr/bin/env bun

type WsEvent = {
  id?: string;
  channel?: string;
  event?: string;
  cmd?: string;
  success?: boolean;
  data?: unknown;
  error?: string;
  session_id?: string;
  message_version?: { version?: number; message_count?: number };
  [k: string]: unknown;
};

type Pending = {
  resolve: (event: WsEvent) => void;
  reject: (error: Error) => void;
  timeout: Timer;
};

type Args = {
  baseUrl: string;
  token?: string;
  username?: string;
  password?: string;
  workspacePath?: string;
  promptOne: string;
  promptTwo: string;
};

function die(msg: string): never {
  console.error(`[chat-version-sync] ERROR: ${msg}`);
  process.exit(1);
}

function log(msg: string): void {
  console.log(`[chat-version-sync] ${msg}`);
}

function parseArgs(): Args {
  const args = process.argv.slice(2);
  const out: Record<string, string> = {};
  for (let i = 0; i < args.length; i += 1) {
    const a = args[i];
    if (!a.startsWith("--")) continue;
    const key = a.slice(2);
    const val = args[i + 1] ?? "";
    out[key] = val;
    i += 1;
  }

  const baseUrl = (out["base-url"] || "").replace(/\/$/, "");
  const token = out["token"] || undefined;
  const username = out["username"] || undefined;
  const password = out["password"] || undefined;
  const workspacePath = out["workspace-path"] || undefined;
  const promptOne = out["prompt-one"] || "Reply with exactly: E2E-ONE";
  const promptTwo = out["prompt-two"] || "Reply with exactly: E2E-TWO";

  if (!baseUrl) die("--base-url missing");
  if (!token && !(username && password)) {
    die("Provide either --token OR (--username and --password)");
  }

  return { baseUrl, token, username, password, workspacePath, promptOne, promptTwo };
}

function toWsUrl(baseUrl: string, token: string): string {
  const u = new URL(`${baseUrl}/api/ws/mux`);
  if (u.protocol === "https:") u.protocol = "wss:";
  if (u.protocol === "http:") u.protocol = "ws:";
  u.searchParams.set("token", token);
  return u.toString();
}

async function login(baseUrl: string, username: string, password: string): Promise<string> {
  const resp = await fetch(`${baseUrl}/api/auth/login`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ username, password }),
  });
  if (!resp.ok) {
    die(`login failed: HTTP ${resp.status}`);
  }
  const body = (await resp.json()) as { token?: string };
  if (!body.token) die("login response missing token");
  return body.token;
}

async function main() {
  const args = parseArgs();
  const token =
    args.token ||
    (await login(args.baseUrl, args.username as string, args.password as string));

  const wsUrl = toWsUrl(args.baseUrl, token);
  const ws = new WebSocket(wsUrl);

  let requestId = 0;
  const pending = new Map<string, Pending>();
  const idleWatchers = new Map<string, { resolve: () => void; reject: (e: Error) => void; timeout: Timer }>();

  const openPromise = new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("WebSocket connect timeout")), 15000);
    ws.onopen = () => {
      clearTimeout(timer);
      resolve();
    };
    ws.onerror = () => {
      clearTimeout(timer);
      reject(new Error("WebSocket open error"));
    };
  });

  ws.onmessage = (ev) => {
    let msg: WsEvent;
    try {
      msg = JSON.parse(String(ev.data));
    } catch {
      return;
    }

    if (msg.channel === "agent" && msg.event === "response" && msg.id && pending.has(msg.id)) {
      const p = pending.get(msg.id)!;
      clearTimeout(p.timeout);
      pending.delete(msg.id);
      p.resolve(msg);
      return;
    }

    if (msg.channel === "agent" && msg.event === "agent.idle" && msg.session_id) {
      const w = idleWatchers.get(msg.session_id);
      if (w) {
        clearTimeout(w.timeout);
        idleWatchers.delete(msg.session_id);
        w.resolve();
      }
    }
  };

  ws.onclose = () => {
    for (const [id, p] of pending.entries()) {
      clearTimeout(p.timeout);
      p.reject(new Error(`WebSocket closed before response (id=${id})`));
    }
    pending.clear();

    for (const [sessionId, w] of idleWatchers.entries()) {
      clearTimeout(w.timeout);
      w.reject(new Error(`WebSocket closed before agent.idle for session ${sessionId}`));
    }
    idleWatchers.clear();
  };

  await openPromise;
  log(`connected ws=${wsUrl.replace(token, "***")}`);

  function sendAgentAndWait(
    sessionId: string,
    payload: Record<string, unknown>,
    timeoutMs = 45000,
  ): Promise<WsEvent> {
    requestId += 1;
    const id = `cv-${requestId}`;
    const cmd = { channel: "agent", id, session_id: sessionId, ...payload };

    const promise = new Promise<WsEvent>((resolve, reject) => {
      const timeout = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`Timeout waiting for agent response id=${id}`));
      }, timeoutMs);
      pending.set(id, { resolve, reject, timeout });
    });

    ws.send(JSON.stringify(cmd));
    return promise;
  }

  function waitForIdle(sessionId: string, timeoutMs = 120000): Promise<void> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        idleWatchers.delete(sessionId);
        reject(new Error(`Timeout waiting for agent.idle (${sessionId})`));
      }, timeoutMs);
      idleWatchers.set(sessionId, { resolve, reject, timeout });
    });
  }

  function extractVersion(resp: WsEvent): { version: number; messageCount?: number } {
    if (resp.success !== true) {
      throw new Error(`Command failed: ${String(resp.error || "unknown")}`);
    }
    const data = (resp.data || {}) as {
      message_version?: { version?: number; message_count?: number };
      messages?: unknown[];
    };
    const version = data.message_version?.version;
    if (!Number.isFinite(version)) {
      throw new Error("get_messages response missing numeric data.message_version.version");
    }
    return {
      version: Number(version),
      messageCount:
        typeof data.message_version?.message_count === "number"
          ? data.message_version.message_count
          : undefined,
    };
  }

  const sessionId = `e2e-version-${Date.now()}`;
  const sessionConfig: Record<string, unknown> = { harness: "pi" };
  if (args.workspacePath) {
    sessionConfig.cwd = args.workspacePath;
  }

  log(`creating session ${sessionId}`);
  const createResp = await sendAgentAndWait(sessionId, {
    cmd: "session.create",
    config: sessionConfig,
  });
  if (createResp.success !== true) {
    throw new Error(`session.create failed: ${String(createResp.error || "unknown")}`);
  }

  log("sending prompt #1");
  const idle1 = waitForIdle(sessionId);
  const promptOneResp = await sendAgentAndWait(sessionId, {
    cmd: "prompt",
    message: args.promptOne,
  }, 15000);
  if (promptOneResp.success !== true) {
    throw new Error(`prompt #1 command failed: ${String(promptOneResp.error || "unknown")}`);
  }
  await idle1;

  const get1 = await sendAgentAndWait(sessionId, { cmd: "get_messages" });
  const v1 = extractVersion(get1);
  log(`after prompt #1: version=${v1.version} message_count=${String(v1.messageCount ?? "n/a")}`);

  log("sending prompt #2");
  const idle2 = waitForIdle(sessionId);
  const promptTwoResp = await sendAgentAndWait(sessionId, {
    cmd: "prompt",
    message: args.promptTwo,
  }, 15000);
  if (promptTwoResp.success !== true) {
    throw new Error(`prompt #2 command failed: ${String(promptTwoResp.error || "unknown")}`);
  }
  await idle2;

  const get2 = await sendAgentAndWait(sessionId, { cmd: "get_messages" });
  const v2 = extractVersion(get2);
  log(`after prompt #2: version=${v2.version} message_count=${String(v2.messageCount ?? "n/a")}`);

  if (v2.version <= v1.version) {
    throw new Error(
      `Version did not increase across prompts: v1=${v1.version}, v2=${v2.version}`,
    );
  }

  log("PASS: get_messages exposes message_version and it increases after persisted turns");

  // best-effort session close
  try {
    await sendAgentAndWait(sessionId, { cmd: "session.close" }, 8000);
  } catch {
    // ignore
  }
  ws.close();
}

main().catch((e) => {
  die(String(e instanceof Error ? e.message : e));
});
