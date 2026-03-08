#!/usr/bin/env bun

type WsEvent = {
  id?: string;
  channel?: string;
  type?: string;
  error?: string;
  [k: string]: unknown;
};

type Pending = {
  resolve: (event: WsEvent) => void;
  reject: (error: Error) => void;
  timeout: Timer;
};

function die(msg: string): never {
  console.error(`[files-mux] ERROR: ${msg}`);
  process.exit(1);
}

function log(msg: string): void {
  console.log(`[files-mux] ${msg}`);
}

function parseArgs() {
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
  const baseUrl = out["base-url"] || "";
  const token = out["token"] || "";
  const workspacePath = out["workspace-path"] || "";
  const loops = Number(out["loops"] || "5");
  if (!baseUrl) die("--base-url missing");
  if (!token) die("--token missing");
  if (!workspacePath) die("--workspace-path missing");
  if (!Number.isFinite(loops) || loops < 1) die("--loops must be >= 1");
  return { baseUrl: baseUrl.replace(/\/$/, ""), token, workspacePath, loops };
}

function toWsUrl(baseUrl: string, token: string): string {
  const u = new URL(`${baseUrl}/api/ws/mux`);
  if (u.protocol === "https:") u.protocol = "wss:";
  if (u.protocol === "http:") u.protocol = "ws:";
  u.searchParams.set("token", token);
  return u.toString();
}

async function main() {
  const { baseUrl, token, workspacePath, loops } = parseArgs();
  const wsUrl = toWsUrl(baseUrl, token);

  const ws = new WebSocket(wsUrl);
  let requestId = 0;
  const pending = new Map<string, Pending>();

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

    if (msg.id && pending.has(msg.id)) {
      const p = pending.get(msg.id)!;
      clearTimeout(p.timeout);
      pending.delete(msg.id);
      p.resolve(msg);
    }
  };

  ws.onclose = () => {
    for (const [id, p] of pending.entries()) {
      clearTimeout(p.timeout);
      p.reject(new Error(`WebSocket closed before response (id=${id})`));
    }
    pending.clear();
  };

  await openPromise;
  log(`connected ws=${wsUrl.replace(token, "***")}`);

  async function sendAndWait(command: Record<string, unknown>, expectedType: string): Promise<WsEvent> {
    requestId += 1;
    const id = `f-${requestId}`;
    const payload = { ...command, id };

    const promise = new Promise<WsEvent>((resolve, reject) => {
      const timeout = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`Timeout waiting for response id=${id} type=${expectedType}`));
      }, 15000);
      pending.set(id, { resolve, reject, timeout });
    });

    ws.send(JSON.stringify(payload));
    const ev = await promise;

    if (ev.channel !== "files") {
      throw new Error(`Unexpected channel: ${String(ev.channel)}`);
    }
    if (ev.type === "error") {
      throw new Error(`Files error: ${String(ev.error || "unknown")}`);
    }
    if (ev.type !== expectedType) {
      throw new Error(`Unexpected response type: got=${String(ev.type)} expected=${expectedType}`);
    }
    return ev;
  }

  const runId = `reliability-${Date.now()}`;
  const baseDir = `.oqto-reliability/${runId}`;
  const content = `reliability test ${new Date().toISOString()}\n`;
  const b64 = Buffer.from(content, "utf8").toString("base64");

  for (let i = 1; i <= loops; i += 1) {
    const dir = `${baseDir}/loop-${i}`;
    const fileA = `${dir}/a.txt`;
    const fileB = `${dir}/b.txt`;
    const fileC = `${dir}/c.txt`;

    await sendAndWait({ channel: "files", type: "create_directory", path: dir, create_parents: true, workspace_path: workspacePath }, "create_directory_result");
    await sendAndWait({ channel: "files", type: "write", path: fileA, content: b64, create_parents: true, workspace_path: workspacePath }, "write_result");

    const read = await sendAndWait({ channel: "files", type: "read", path: fileA, workspace_path: workspacePath }, "read_result");
    const got = Buffer.from(String(read.content || ""), "base64").toString("utf8");
    if (got !== content) {
      throw new Error(`Content mismatch loop=${i}`);
    }

    await sendAndWait({ channel: "files", type: "rename", from: fileA, to: fileB, workspace_path: workspacePath }, "rename_result");
    await sendAndWait({ channel: "files", type: "copy", from: fileB, to: fileC, overwrite: true, workspace_path: workspacePath }, "copy_result");
    await sendAndWait({ channel: "files", type: "move", from: fileC, to: fileA, overwrite: true, workspace_path: workspacePath }, "move_result");

    await sendAndWait({ channel: "files", type: "stat", path: fileA, workspace_path: workspacePath }, "stat_result");
    await sendAndWait({ channel: "files", type: "delete", path: dir, recursive: true, workspace_path: workspacePath }, "delete_result");

    log(`loop ${i}/${loops} passed`);
  }

  // cleanup top-level reliability dir best-effort
  try {
    await sendAndWait({ channel: "files", type: "delete", path: ".oqto-reliability", recursive: true, workspace_path: workspacePath }, "delete_result");
  } catch {
    // ignore; may not exist or be used by parallel run
  }

  ws.close();
  log(`all file mux mutation checks passed (loops=${loops})`);
}

main().catch((e) => {
  die(String(e instanceof Error ? e.message : e));
});
