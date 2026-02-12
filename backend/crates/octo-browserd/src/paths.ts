import os from "node:os";
import path from "node:path";

export function getSessionId(): string {
  return process.env.AGENT_BROWSER_SESSION || "default";
}

export function getSocketDir(session?: string): string {
  if (process.env.AGENT_BROWSER_SOCKET_DIR) {
    return process.env.AGENT_BROWSER_SOCKET_DIR;
  }

  const sess = session ?? getSessionId();

  if (process.env.XDG_RUNTIME_DIR) {
    return path.join(process.env.XDG_RUNTIME_DIR, "octo", "agent-browser", sess);
  }

  const home = os.homedir();
  if (home) {
    return path.join(home, ".agent-browser", "octo", sess);
  }

  return path.join(os.tmpdir(), "agent-browser", "octo", sess);
}

export function getSocketPath(session?: string): string {
  const sess = session ?? getSessionId();
  return path.join(getSocketDir(sess), `${sess}.sock`);
}

export function getPidFile(session?: string): string {
  const sess = session ?? getSessionId();
  return path.join(getSocketDir(sess), `${sess}.pid`);
}

export function getStreamPortFile(session?: string): string {
  const sess = session ?? getSessionId();
  return path.join(getSocketDir(sess), `${sess}.stream`);
}
