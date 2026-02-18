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

  // Use XDG_STATE_HOME instead of XDG_RUNTIME_DIR: XDG_RUNTIME_DIR is a
  // tmpfs and bwrap re-mounts it as a fresh tmpfs inside the sandbox, so
  // socket files created here on the host are invisible to Pi agents inside
  // the sandbox. XDG_STATE_HOME is on a real filesystem and bind-mounts work
  // correctly.
  if (process.env.XDG_STATE_HOME) {
    return path.join(process.env.XDG_STATE_HOME, "oqto", "agent-browser", sess);
  }

  const home = os.homedir();
  if (home) {
    return path.join(home, ".local", "state", "oqto", "agent-browser", sess);
  }

  return path.join(os.tmpdir(), "oqto", "agent-browser", sess);
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


