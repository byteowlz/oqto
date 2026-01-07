/**
 * Octo Delegation Extension for Pi
 *
 * Provides tools for Pi to delegate tasks to OpenCode sessions managed by Octo.
 * This allows Pi (Main Chat) to orchestrate work across multiple project workspaces.
 *
 * Tools:
 * - octo_session: Start, prompt, check status, and manage OpenCode sessions
 *
 * Configuration (environment variables):
 * - OCTO_API_URL: Base URL for Octo API (default: http://localhost:8080)
 * - OCTO_USER_ID: User ID for authentication (required in multi-user mode)
 */

import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";

// Configuration
const OCTO_API_URL = process.env.OCTO_API_URL || "http://localhost:8080";

// Types for Octo API responses
interface SessionInfo {
  id: string;
  status: "pending" | "starting" | "running" | "stopping" | "stopped" | "failed";
  workspace_path: string;
  created_at: string;
  started_at?: string;
  last_activity_at?: string;
  error_message?: string;
  source?: string;
}

interface SessionMessage {
  role: "user" | "assistant";
  content: string;
  timestamp?: string;
}

interface DelegateResponse {
  session_id: string;
  status: string;
  message?: string;
}

// Helper to make API calls to Octo
async function octoFetch<T>(
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const url = `${OCTO_API_URL}/delegate${path}`;
  const response = await fetch(url, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...options.headers,
    },
  });

  if (!response.ok) {
    const error = await response.text();
    throw new Error(`Octo API error (${response.status}): ${error}`);
  }

  return response.json();
}

export default function (pi: ExtensionAPI) {
  // Register the octo_session tool
  pi.registerTool({
    name: "octo_session",
    label: "Octo Session",
    description: `Delegate tasks to OpenCode coding sessions managed by Octo.

Actions:
- start: Start a new session in a project directory with an initial task
- prompt: Send a follow-up message to an existing session
- status: Check the status of a session (running, idle, etc.)
- messages: Get recent messages from a session
- stop: Stop a running session (if allowed by config)
- list: List all sessions for the current user

Use this to delegate complex coding tasks to specialized OpenCode agents working in specific project directories. Sessions appear in the Octo sidebar and can be monitored by the user.`,

    parameters: Type.Object({
      action: Type.Union([
        Type.Literal("start"),
        Type.Literal("prompt"),
        Type.Literal("status"),
        Type.Literal("messages"),
        Type.Literal("stop"),
        Type.Literal("list"),
      ], { description: "The action to perform" }),
      directory: Type.Optional(
        Type.String({ description: "Project directory path (required for 'start')" })
      ),
      session_id: Type.Optional(
        Type.String({ description: "Session ID (required for 'prompt', 'status', 'messages', 'stop')" })
      ),
      prompt: Type.Optional(
        Type.String({ description: "Task or message to send (required for 'start' and 'prompt')" })
      ),
      agent: Type.Optional(
        Type.String({ description: "OpenCode agent name to use (optional, for 'start')" })
      ),
      limit: Type.Optional(
        Type.Number({ description: "Number of messages to retrieve (for 'messages', default: 20)" })
      ),
    }),

    async execute(toolCallId, params, onUpdate, ctx, signal) {
      const { action, directory, session_id, prompt, agent, limit } = params;

      try {
        switch (action) {
          case "start": {
            if (!directory) {
              return {
                content: [{ type: "text", text: "Error: 'directory' is required for 'start' action" }],
                details: { error: "missing_directory" },
              };
            }
            if (!prompt) {
              return {
                content: [{ type: "text", text: "Error: 'prompt' is required for 'start' action" }],
                details: { error: "missing_prompt" },
              };
            }

            onUpdate({ status: `Starting session in ${directory}...` });

            const result = await octoFetch<DelegateResponse>("/start", {
              method: "POST",
              body: JSON.stringify({
                directory,
                prompt,
                agent,
              }),
            });

            return {
              content: [{
                type: "text",
                text: `Started session ${result.session_id} in ${directory}\nStatus: ${result.status}\nThe session is now processing your task. Use 'status' or 'messages' to check progress.`,
              }],
              details: result,
            };
          }

          case "prompt": {
            if (!session_id) {
              return {
                content: [{ type: "text", text: "Error: 'session_id' is required for 'prompt' action" }],
                details: { error: "missing_session_id" },
              };
            }
            if (!prompt) {
              return {
                content: [{ type: "text", text: "Error: 'prompt' is required for 'prompt' action" }],
                details: { error: "missing_prompt" },
              };
            }

            onUpdate({ status: `Sending prompt to session ${session_id}...` });

            const result = await octoFetch<DelegateResponse>(`/prompt/${session_id}`, {
              method: "POST",
              body: JSON.stringify({ prompt }),
            });

            return {
              content: [{
                type: "text",
                text: `Sent prompt to session ${session_id}\nStatus: ${result.status}`,
              }],
              details: result,
            };
          }

          case "status": {
            if (!session_id) {
              return {
                content: [{ type: "text", text: "Error: 'session_id' is required for 'status' action" }],
                details: { error: "missing_session_id" },
              };
            }

            const session = await octoFetch<SessionInfo>(`/status/${session_id}`);

            const statusText = [
              `Session: ${session.id}`,
              `Status: ${session.status}`,
              `Workspace: ${session.workspace_path}`,
              `Created: ${session.created_at}`,
              session.started_at ? `Started: ${session.started_at}` : null,
              session.last_activity_at ? `Last Activity: ${session.last_activity_at}` : null,
              session.error_message ? `Error: ${session.error_message}` : null,
            ].filter(Boolean).join("\n");

            return {
              content: [{ type: "text", text: statusText }],
              details: session,
            };
          }

          case "messages": {
            if (!session_id) {
              return {
                content: [{ type: "text", text: "Error: 'session_id' is required for 'messages' action" }],
                details: { error: "missing_session_id" },
              };
            }

            const messages = await octoFetch<SessionMessage[]>(
              `/messages/${session_id}?limit=${limit || 20}`
            );

            if (messages.length === 0) {
              return {
                content: [{ type: "text", text: "No messages in session yet." }],
                details: { messages: [] },
              };
            }

            const formatted = messages.map((m) => {
              const role = m.role === "user" ? "User" : "Assistant";
              const time = m.timestamp ? ` (${m.timestamp})` : "";
              return `[${role}${time}]\n${m.content}`;
            }).join("\n\n---\n\n");

            return {
              content: [{ type: "text", text: formatted }],
              details: { messages, count: messages.length },
            };
          }

          case "stop": {
            if (!session_id) {
              return {
                content: [{ type: "text", text: "Error: 'session_id' is required for 'stop' action" }],
                details: { error: "missing_session_id" },
              };
            }

            onUpdate({ status: `Stopping session ${session_id}...` });

            const result = await octoFetch<DelegateResponse>(`/stop/${session_id}`, {
              method: "POST",
            });

            return {
              content: [{ type: "text", text: `Session ${session_id} stopped.\nStatus: ${result.status}` }],
              details: result,
            };
          }

          case "list": {
            const sessions = await octoFetch<SessionInfo[]>("/sessions");

            if (sessions.length === 0) {
              return {
                content: [{ type: "text", text: "No active sessions." }],
                details: { sessions: [] },
              };
            }

            const formatted = sessions.map((s) => {
              return `- ${s.id} [${s.status}] ${s.workspace_path}`;
            }).join("\n");

            return {
              content: [{ type: "text", text: `Active sessions:\n${formatted}` }],
              details: { sessions, count: sessions.length },
            };
          }

          default:
            return {
              content: [{ type: "text", text: `Unknown action: ${action}` }],
              details: { error: "unknown_action" },
            };
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        return {
          content: [{ type: "text", text: `Error: ${message}` }],
          details: { error: message },
        };
      }
    },

    // Custom rendering for tool calls
    renderCall(args, theme) {
      const { action, directory, session_id, prompt } = args;
      let desc = `octo_session ${action}`;
      if (directory) desc += ` in ${directory}`;
      if (session_id) desc += ` (${session_id.slice(0, 8)}...)`;
      if (prompt) desc += `: "${prompt.slice(0, 50)}${prompt.length > 50 ? "..." : ""}"`;
      return theme.fg("accent", desc);
    },

    renderResult(result, options, theme) {
      if (result.details?.error) {
        return theme.fg("error", `Error: ${result.details.error}`);
      }
      if (result.details?.session_id) {
        return theme.fg("success", `Session: ${result.details.session_id}`);
      }
      if (result.details?.count !== undefined) {
        return theme.fg("dim", `${result.details.count} items`);
      }
      return theme.fg("success", "Done");
    },
  });

  // Register a command to list delegated sessions
  pi.registerCommand("sessions", {
    description: "List all OpenCode sessions delegated from Main Chat",
    handler: async (args, ctx) => {
      try {
        const sessions = await octoFetch<SessionInfo[]>("/sessions");
        if (sessions.length === 0) {
          ctx.ui.notify("No active sessions", "info");
          return;
        }
        const lines = sessions.map((s) => `${s.status.padEnd(10)} ${s.id.slice(0, 8)} ${s.workspace_path}`);
        ctx.ui.notify(`Sessions:\n${lines.join("\n")}`, "info");
      } catch (error) {
        ctx.ui.notify(`Error: ${error}`, "error");
      }
    },
  });
}
