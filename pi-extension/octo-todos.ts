/**
 * Octo Todos Extension for Pi
 *
 * Provides the todowrite tool for task management in Pi sessions.
 * This allows Pi to track tasks and display them in the Octo sidebar,
 * matching the same functionality as OpenCode/Claude sessions.
 */

import type { ExtensionAPI, ExtensionContext } from "@mariozechner/pi-coding-agent";
import { Type } from "@sinclair/typebox";

interface TodoItem {
  id: string;
  content: string;
  status: "pending" | "in_progress" | "completed" | "cancelled";
  priority?: "high" | "medium" | "low";
}

interface TodoDetails {
  todos: TodoItem[];
}

export default function (pi: ExtensionAPI) {
  let todos: TodoItem[] = [];

  const reconstructState = (ctx: ExtensionContext) => {
    todos = [];

    for (const entry of ctx.sessionManager.getBranch()) {
      if (entry.type !== "message") continue;
      const msg = entry.message;
      if (msg.role !== "toolResult") continue;
      if (msg.toolName !== "todowrite" && msg.toolName !== "todoread") continue;

      const details = msg.details as TodoDetails | undefined;
      if (details?.todos) {
        todos = details.todos;
      }
    }
  };

  pi.on("session_start", async (_event, ctx) => reconstructState(ctx));
  pi.on("session_switch", async (_event, ctx) => reconstructState(ctx));
  pi.on("session_branch", async (_event, ctx) => reconstructState(ctx));
  pi.on("session_tree", async (_event, ctx) => reconstructState(ctx));

  // Register the todowrite tool for task management
  pi.registerTool({
    name: "todowrite",
    label: "Todo Write",
    description: `Create and manage a structured task list for your current session. This helps you track progress, organize complex tasks, and demonstrate thoroughness to the user.

Use this tool when:
- Working on complex multi-step tasks (3+ steps)
- User provides multiple tasks to complete
- You need to track progress across multiple operations
- Breaking down large features into manageable steps

Task states:
- pending: Task not yet started
- in_progress: Currently working on (limit to ONE at a time)
- completed: Task finished successfully
- cancelled: Task no longer needed

Mark tasks complete immediately after finishing. Only have one task in_progress at any time.`,

    parameters: Type.Object({
      todos: Type.Array(
        Type.Object({
          id: Type.String({ description: "Unique identifier for the todo item" }),
          content: Type.String({ description: "Brief description of the task" }),
          status: Type.Union([
            Type.Literal("pending"),
            Type.Literal("in_progress"),
            Type.Literal("completed"),
            Type.Literal("cancelled"),
          ], { description: "Current status of the task" }),
          priority: Type.Optional(
            Type.Union([
              Type.Literal("high"),
              Type.Literal("medium"),
              Type.Literal("low"),
            ], { description: "Priority level of the task" }),
          ),
        }),
        { description: "The updated todo list" }
      ),
    }),

    async execute(toolCallId, params, onUpdate, ctx, signal) {
      const { todos: nextTodos } = params;
      todos = Array.isArray(nextTodos) ? [...nextTodos] : [];

      // Count by status
      const pending = todos.filter((t) => t.status === "pending").length;
      const inProgress = todos.filter((t) => t.status === "in_progress").length;
      const completed = todos.filter((t) => t.status === "completed").length;
      const cancelled = todos.filter((t) => t.status === "cancelled").length;

      const summary = [
        `${todos.length} tasks total`,
        pending > 0 ? `${pending} pending` : null,
        inProgress > 0 ? `${inProgress} in progress` : null,
        completed > 0 ? `${completed} completed` : null,
        cancelled > 0 ? `${cancelled} cancelled` : null,
      ].filter(Boolean).join(", ");

      return {
        content: [{ type: "text", text: `Updated todo list: ${summary}` }],
        details: { todos: [...todos] },
      };
    },

    renderCall(args, theme) {
      const { todos } = args;
      if (!todos || !Array.isArray(todos)) {
        return theme.fg("accent", "todowrite");
      }
      const inProgress = todos.filter((t: { status: string }) => t.status === "in_progress");
      if (inProgress.length > 0) {
        return theme.fg("accent", `todowrite: ${inProgress[0].content.slice(0, 40)}...`);
      }
      return theme.fg("accent", `todowrite: ${todos.length} tasks`);
    },

    renderResult(result, options, theme) {
      if (result.details?.todos) {
        const todos = result.details.todos;
        const completed = todos.filter((t: { status: string }) => t.status === "completed").length;
        const total = todos.length;
        return theme.fg("success", `${completed}/${total} completed`);
      }
      return theme.fg("success", "Updated");
    },
  });

  // Register the todoread tool for task management
  pi.registerTool({
    name: "todoread",
    label: "Todo Read",
    description: "Read the current todo list for this session.",
    parameters: Type.Object({}, { additionalProperties: false }),

    async execute(toolCallId, params, onUpdate, ctx, signal) {
      return {
        content: [
          {
            type: "text",
            text: todos.length
              ? `Current todo list: ${todos.length} tasks`
              : "No todos yet.",
          },
        ],
        details: { todos: [...todos] },
      };
    },

    renderCall(args, theme) {
      return theme.fg("accent", "todoread");
    },

    renderResult(result, options, theme) {
      if (result.details?.todos) {
        const todos = result.details.todos;
        const completed = todos.filter((t: { status: string }) => t.status === "completed").length;
        const total = todos.length;
        return theme.fg("success", `${completed}/${total} completed`);
      }
      return theme.fg("success", "Read");
    },
  });
}
