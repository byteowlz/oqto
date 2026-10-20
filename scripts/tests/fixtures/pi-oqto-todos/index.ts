/**
 * Octo Todos Extension for Pi
 *
 * Provides a todowrite tool that integrates with Octo's frontend todo panel.
 * This is a drop-in replacement for OpenCode's todowrite/todoread tools.
 *
 * The tool outputs todos in a format that Octo's frontend parses and displays
 * in the right sidebar panel, matching the expected TodoItem structure.
 *
 * Todo format:
 * {
 *   id: string,
 *   content: string,
 *   status: "pending" | "in_progress" | "completed" | "cancelled",
 *   priority: "high" | "medium" | "low"
 * }
 */

import crypto from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { StringEnum } from "@earendil-works/pi-ai";
import type { ExtensionAPI, ExtensionContext, Theme, ThemeColor } from "@earendil-works/pi-coding-agent";
import { Text, truncateToWidth } from "@earendil-works/pi-tui";
import { Type } from "typebox";

// ============================================================================
// Types
// ============================================================================

type TodoStatus = "pending" | "in_progress" | "completed" | "cancelled";
type TodoPriority = "high" | "medium" | "low";

interface TodoItem {
	id: string;
	content: string;
	status: TodoStatus;
	priority: TodoPriority;
}

interface TodoStore {
	todos: TodoItem[];
	updated_at: string;
}

interface OctoTodosConfig {
	enabled: boolean;
	debug: boolean;
	storagePath?: string;
	sessionScoped: boolean;
	tuiWidget: boolean;
}

// ============================================================================
// Constants
// ============================================================================

const CONFIG_FILENAME = "octo-todos.json";
const TODOS_FILENAME = "todos.json";

const DEFAULT_CONFIG: OctoTodosConfig = {
	enabled: true,
	debug: false,
	sessionScoped: true,
	tuiWidget: true,
};

const TODO_STATUSES = ["pending", "in_progress", "completed", "cancelled"] as const;
const TODO_PRIORITIES = ["high", "medium", "low"] as const;

// ============================================================================
// Tool Parameters
// ============================================================================

const TodoWriteParams = Type.Object({
	todos: Type.Array(
		Type.Object({
			id: Type.Optional(Type.String({ description: "Unique identifier (auto-generated if not provided)" })),
			content: Type.String({ description: "Task description" }),
			status: StringEnum(TODO_STATUSES, { description: "Task status" }),
			priority: Type.Optional(StringEnum(TODO_PRIORITIES, { description: "Task priority (default: medium)" })),
		}),
		{ description: "List of todos to write (replaces existing list)" }
	),
});

const TodoReadParams = Type.Object({
	filter: Type.Optional(
		Type.Object({
			status: Type.Optional(StringEnum(TODO_STATUSES, { description: "Filter by status" })),
			priority: Type.Optional(StringEnum(TODO_PRIORITIES, { description: "Filter by priority" })),
		})
	),
});

// Unused but kept for reference:
// const TodoUpdateParams = Type.Object({ ... });
// const TodoAddParams = Type.Object({ ... });
// const TodoRemoveParams = Type.Object({ ... });

// ============================================================================
// Config Loading
// ============================================================================

function loadConfig(cwd: string): OctoTodosConfig {
	const paths = [join(cwd, CONFIG_FILENAME), join(cwd, ".pi", CONFIG_FILENAME), join(homedir(), ".pi", "agent", CONFIG_FILENAME)];

	for (const configPath of paths) {
		if (existsSync(configPath)) {
			try {
				const content = readFileSync(configPath, "utf-8");
				const userConfig = JSON.parse(content) as Partial<OctoTodosConfig>;
				return { ...DEFAULT_CONFIG, ...userConfig };
			} catch {
				// Invalid JSON, continue to next path
			}
		}
	}

	return DEFAULT_CONFIG;
}

// ============================================================================
// Todo Storage
// ============================================================================

function getTodosDir(cwd: string, config: OctoTodosConfig): string {
	if (config.storagePath) {
		if (config.storagePath.startsWith("~")) {
			return join(homedir(), config.storagePath.slice(1));
		}
		return config.storagePath;
	}
	return join(cwd, ".pi", "todos");
}

function getTodosPath(cwd: string, config: OctoTodosConfig, sessionId?: string): string {
	const dir = getTodosDir(cwd, config);
	if (config.sessionScoped && sessionId) {
		return join(dir, `${sessionId}.json`);
	}
	return join(dir, TODOS_FILENAME);
}

function ensureTodosDir(cwd: string, config: OctoTodosConfig): void {
	const dir = getTodosDir(cwd, config);
	if (!existsSync(dir)) {
		mkdirSync(dir, { recursive: true });
	}
}

function loadTodos(cwd: string, config: OctoTodosConfig, sessionId?: string): TodoStore {
	const path = getTodosPath(cwd, config, sessionId);

	if (!existsSync(path)) {
		return { todos: [], updated_at: new Date().toISOString() };
	}

	try {
		const content = readFileSync(path, "utf-8");
		const store = JSON.parse(content) as TodoStore;
		return store;
	} catch {
		return { todos: [], updated_at: new Date().toISOString() };
	}
}

function saveTodos(cwd: string, config: OctoTodosConfig, todos: TodoItem[], sessionId?: string): void {
	ensureTodosDir(cwd, config);
	const path = getTodosPath(cwd, config, sessionId);
	const store: TodoStore = {
		todos,
		updated_at: new Date().toISOString(),
	};
	writeFileSync(path, JSON.stringify(store, null, 2), "utf-8");
}

function generateTodoId(): string {
	return crypto.randomBytes(4).toString("hex");
}

function normalizeTodo(todo: Partial<TodoItem> & { content: string }): TodoItem {
	return {
		id: todo.id || generateTodoId(),
		content: todo.content,
		status: todo.status || "pending",
		priority: todo.priority || "medium",
	};
}

// ============================================================================
// Session ID Helper
// ============================================================================

function getSessionId(ctx: ExtensionContext): string | undefined {
	const manager = ctx.sessionManager as { getSessionId?: () => string };
	return manager.getSessionId?.();
}

// ============================================================================
// Rendering
// ============================================================================

// RenderContext removed - not used

function getStatusIcon(status: TodoStatus): string {
	switch (status) {
		case "completed":
			return "[x]";
		case "in_progress":
			return "[○]";
		case "cancelled":
			return "[-]";
		default:
			return "[ ]";
	}
}

function getPriorityLabel(priority: TodoPriority): string {
	switch (priority) {
		case "high":
			return "!";
		case "low":
			return "v";
		default:
			return "";
	}
}

function normalizeTodoText(text: string): string {
	return text.replace(/\s+/g, " ").trim();
}

function buildSummaryLine(todos: TodoItem[], theme: Theme): string {
	const counts: Record<string, number> = {};
	for (const t of todos) {
		counts[t.status] = (counts[t.status] || 0) + 1;
	}
	const labels: [string, string][] = [
		["in_progress", "in progress"],
		["pending", "pending"],
		["completed", "done"],
		["cancelled", "cancelled"],
	];
	const parts = labels.filter(([key]) => counts[key] > 0).map(([key, label]) => `${counts[key]} ${label}`);
	return theme.fg("muted", `${todos.length} todos (${parts.join(", ")})`);
}

function getStatusColor(status: TodoStatus): ThemeColor {
	if (status === "in_progress") return "success";
	if (status === "completed" || status === "cancelled") return "dim";
	return "text";
}

function renderTodoLine(todo: TodoItem, theme: Theme, maxWidth: number): string {
	const icon = getStatusIcon(todo.status);
	const priorityLabel = getPriorityLabel(todo.priority);
	const contentPreview = truncateToWidth(normalizeTodoText(todo.content), maxWidth);
	let line = theme.fg(getStatusColor(todo.status), `${icon} ${contentPreview}`);
	if (priorityLabel) {
		const priorityColor = todo.priority === "high" ? "error" : "dim";
		line += ` ${theme.fg(priorityColor, priorityLabel)}`;
	}
	return line;
}

function orderTodos(todos: TodoItem[]): TodoItem[] {
	const inProgress = todos.filter((t) => t.status === "in_progress");
	const pending = todos.filter((t) => t.status === "pending");
	const completed = todos.filter((t) => t.status === "completed");
	const cancelled = todos.filter((t) => t.status === "cancelled");
	return [...inProgress, ...pending, ...completed, ...cancelled];
}

function renderTodoList(todos: TodoItem[], theme: Theme, expanded: boolean): string {
	if (todos.length === 0) {
		return theme.fg("muted", "No todos");
	}

	const lines: string[] = [];
	const maxItems = expanded ? 20 : 5;
	const maxWidth = expanded ? 120 : 80;

	lines.push(buildSummaryLine(todos, theme));
	lines.push("");

	const allOrdered = orderTodos(todos);
	const displayTodos = expanded ? allOrdered : allOrdered.slice(0, maxItems);

	for (const todo of displayTodos) {
		lines.push(renderTodoLine(todo, theme, maxWidth));
	}

	if (!expanded && allOrdered.length > maxItems) {
		lines.push(theme.fg("dim", `  ... ${allOrdered.length - maxItems} more`));
	}

	return lines.join("\n");
}

// ============================================================================
// Extension Entry Point
// ============================================================================

export default function octoTodosExtension(pi: ExtensionAPI) {
	// Store reference to current todos for rendering and the TUI widget
	let _currentTodos: TodoItem[] = [];

	// ==========================================================================
	// TUI Widget - persistent todo display above the editor
	// ==========================================================================

	const WIDGET_KEY = "octo-todos";

	/**
	 * Build the widget lines for the current todos.
	 */
	function renderWidgetTodoLine(todo: TodoItem, theme: Theme, effectiveWidth: number): string {
		const icon = getStatusIcon(todo.status);
		const priorityLabel = getPriorityLabel(todo.priority);
		const reservedSuffix = priorityLabel ? 8 : 6;
		const contentWidth = Math.max(8, effectiveWidth - reservedSuffix);
		const contentPreview = truncateToWidth(normalizeTodoText(todo.content), contentWidth);

		let line = theme.fg(getStatusColor(todo.status), `  ${icon} ${contentPreview}`);
		if (priorityLabel) {
			const priorityColor = todo.priority === "high" ? "error" : "dim";
			line += ` ${theme.fg(priorityColor, priorityLabel)}`;
		}
		return truncateToWidth(line, effectiveWidth);
	}

	function buildWidgetLines(todos: TodoItem[], theme: Theme, width?: number): string[] {
		// In rare cases width may be undefined/0 during render transitions.
		const effectiveWidth = width && width > 0 ? width : 35;
		const maxWidgetItems = 8;

		const lines: string[] = [];
		lines.push(truncateToWidth(buildSummaryLine(todos, theme).replace(/^\d+ todos/, "Todos:"), effectiveWidth));

		const active = todos.filter((t) => t.status === "in_progress" || t.status === "pending");
		const displayItems = active.slice(0, maxWidgetItems);
		for (const todo of displayItems) {
			lines.push(renderWidgetTodoLine(todo, theme, effectiveWidth));
		}
		if (active.length > maxWidgetItems) {
			lines.push(truncateToWidth(theme.fg("dim", `  ... ${active.length - maxWidgetItems} more`), effectiveWidth));
		}

		return lines;
	}

	/**
	 * Update the persistent TUI widget showing current todos.
	 * Called after every tool execution and session event.
	 *
	 * Wrapped in try/catch to prevent TUI errors (e.g. when Pi runs in
	 * --mode rpc with no real TUI backend) from crashing the process.
	 */
	function updateWidget(ctx: ExtensionContext): void {
		if (!ctx.hasUI) return;

		try {
			const config = loadConfig(ctx.cwd);
			if (!config.enabled || !config.tuiWidget || _currentTodos.length === 0) {
				ctx.ui.setWidget(WIDGET_KEY, undefined);
				return;
			}

			ctx.ui.setWidget(WIDGET_KEY, (_tui, theme) => {
				try {
					return {
						render: (width: number) => {
							try {
								return buildWidgetLines(_currentTodos, theme, width);
							} catch {
								return ["[todo render error]"];
							}
						},
						// biome-ignore lint/suspicious/noEmptyBlockStatements: widget is rebuilt on each update
						invalidate: () => {},
					};
				} catch {
					// Return a safe fallback widget if building lines fails
					return {
						render: () => ["[todo widget error]"],
						// biome-ignore lint/suspicious/noEmptyBlockStatements: fallback widget
						invalidate: () => {},
					};
				}
			});
		} catch {
			// setWidget itself failed -- TUI not available or in bad state.
			// Silently ignore; the extension continues to work for tool I/O.
		}
	}

	/**
	 * Reconstruct todos from file storage on session events.
	 * Fully wrapped in try/catch -- a failure here must never crash Pi.
	 */
	function reconstructTodos(ctx: ExtensionContext): void {
		try {
			const config = loadConfig(ctx.cwd);
			if (!config.enabled) {
				_currentTodos = [];
				updateWidget(ctx);
				return;
			}

			const sessionId = getSessionId(ctx);
			const store = loadTodos(ctx.cwd, config, sessionId);
			_currentTodos = store.todos;
			updateWidget(ctx);
		} catch (e) {
			// Log but never propagate -- extension errors must not crash the host.
			console.error("[octo-todos] reconstructTodos failed:", e);
		}
	}

	// ==========================================================================
	// Session event handlers - reconstruct state and update widget
	// ==========================================================================

	// Session event handlers are individually wrapped so one failure does not
	// prevent the others from registering or executing.
	pi.on("session_start", async (_event, ctx) => {
		try {
			reconstructTodos(ctx);
		} catch (e) {
			console.error("[octo-todos] session_start handler error:", e);
		}
	});
	pi.on("session_tree", async (_event, ctx) => {
		try {
			reconstructTodos(ctx);
		} catch (e) {
			console.error("[octo-todos] session_tree handler error:", e);
		}
	});

	// ==========================================================================
	// TodoWrite - Main tool for writing todos (matches OpenCode format)
	// ==========================================================================
	pi.registerTool({
		name: "TodoWrite",
		label: "Todo Write",
		description:
			"Write a list of todos that will be displayed in the Octo frontend panel. " +
			"This replaces the entire todo list. Use for task planning and tracking. " +
			"Todos have: content (task description), status (pending/in_progress/completed/cancelled), " +
			"priority (high/medium/low). The frontend displays these in a dedicated panel.\n\n" +
			"IMPORTANT: Keep the todo list current throughout the session. When you complete a task, " +
			"immediately rewrite the full list with that task's status set to 'completed'. " +
			"When you start working on a task, set it to 'in_progress'. " +
			"The user watches this panel to track your progress in real time.",
		parameters: TodoWriteParams,

		async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
			const config = loadConfig(ctx.cwd);
			if (!config.enabled) {
				return {
					content: [{ type: "text", text: "Todos extension is disabled" }],
					details: { action: "write", error: "disabled" },
				};
			}

			const sessionId = getSessionId(ctx);

			// Normalize all todos
			const normalizedTodos = params.todos.map((t) => normalizeTodo(t));

			// Save to storage
			saveTodos(ctx.cwd, config, normalizedTodos, sessionId);
			_currentTodos = normalizedTodos;
			updateWidget(ctx);

			// Return in format that Octo frontend expects
			return {
				content: [
					{
						type: "text",
						text: JSON.stringify({ todos: normalizedTodos }, null, 2),
					},
				],
				details: { action: "write", todos: normalizedTodos },
			};
		},

		renderCall(args, theme) {
			try {
				const todos = (args.todos as Array<{ content?: string }>) || [];
				const count = todos.length;
				return new Text(theme.fg("toolTitle", theme.bold("TodoWrite ")) + theme.fg("muted", `(${count} items)`), 0, 0);
			} catch {
				return new Text("TodoWrite", 0, 0);
			}
		},

		renderResult(result, { expanded }, theme) {
			try {
				const details = result.details as { todos?: TodoItem[]; error?: string } | undefined;

				if (details?.error) {
					return new Text(theme.fg("error", `Error: ${details.error}`), 0, 0);
				}

				const todos = details?.todos || [];
				return new Text(renderTodoList(todos, theme, expanded), 0, 0);
			} catch {
				return new Text("(render error)", 0, 0);
			}
		},
	});

	// ==========================================================================
	// TodoRead - Read current todos
	// ==========================================================================
	pi.registerTool({
		name: "TodoRead",
		label: "Todo Read",
		description: "Read the current list of todos with optional filtering by status or priority.",
		parameters: TodoReadParams,

		async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
			const config = loadConfig(ctx.cwd);
			if (!config.enabled) {
				return {
					content: [{ type: "text", text: "Todos extension is disabled" }],
					details: { action: "read", error: "disabled" },
				};
			}

			const sessionId = getSessionId(ctx);
			const store = loadTodos(ctx.cwd, config, sessionId);
			let todos = store.todos;

			// Apply filters
			if (params.filter) {
				if (params.filter.status) {
					todos = todos.filter((t) => t.status === params.filter?.status);
				}
				if (params.filter.priority) {
					todos = todos.filter((t) => t.priority === params.filter?.priority);
				}
			}

			_currentTodos = todos;
			updateWidget(ctx);

			return {
				content: [
					{
						type: "text",
						text: JSON.stringify({ todos }, null, 2),
					},
				],
				details: { action: "read", todos },
			};
		},

		renderCall(args, theme) {
			try {
				const filter = args.filter as { status?: string; priority?: string } | undefined;
				let filterStr = "";
				if (filter?.status) filterStr += ` status=${filter.status}`;
				if (filter?.priority) filterStr += ` priority=${filter.priority}`;
				return new Text(theme.fg("toolTitle", theme.bold("TodoRead")) + (filterStr ? theme.fg("muted", filterStr) : ""), 0, 0);
			} catch {
				return new Text("TodoRead", 0, 0);
			}
		},

		renderResult(result, { expanded }, theme) {
			try {
				const details = result.details as { todos?: TodoItem[]; error?: string } | undefined;

				if (details?.error) {
					return new Text(theme.fg("error", `Error: ${details.error}`), 0, 0);
				}

				const todos = details?.todos || [];
				return new Text(renderTodoList(todos, theme, expanded), 0, 0);
			} catch {
				return new Text("(render error)", 0, 0);
			}
		},
	});

	// ==========================================================================
	// Todo - Unified tool for all todo operations
	// ==========================================================================
	pi.registerTool({
		name: "Todo",
		label: "Todo",
		description:
			"Unified todo management: add, update, remove, or list todos. " +
			"Actions: add (new todo), update (modify existing), remove (delete), list (show all). " +
			"Todos are displayed in the Octo frontend panel.\n\n" +
			"IMPORTANT: Always update todo status as you work. Set tasks to 'in_progress' when starting " +
			"and 'completed' when done. The user relies on this panel to see your progress.",
		parameters: Type.Object({
			action: StringEnum(["add", "update", "remove", "list"] as const),
			// For add
			content: Type.Optional(Type.String({ description: "Task description (for add)" })),
			// For add/update
			status: Type.Optional(StringEnum(TODO_STATUSES)),
			priority: Type.Optional(StringEnum(TODO_PRIORITIES)),
			// For update/remove
			id: Type.Optional(Type.String({ description: "Todo ID (for update/remove)" })),
		}),

		async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
			const config = loadConfig(ctx.cwd);
			if (!config.enabled) {
				return {
					content: [{ type: "text", text: "Todos extension is disabled" }],
					details: { action: params.action, error: "disabled" },
				};
			}

			const sessionId = getSessionId(ctx);
			const store = loadTodos(ctx.cwd, config, sessionId);
			const todos = [...store.todos];

			switch (params.action) {
				case "add": {
					if (!params.content) {
						return {
							content: [{ type: "text", text: "Error: content required for add" }],
							details: { action: "add", error: "content required" },
						};
					}
					const newTodo = normalizeTodo({
						content: params.content,
						status: params.status,
						priority: params.priority,
					});
					todos.push(newTodo);
					saveTodos(ctx.cwd, config, todos, sessionId);
					_currentTodos = todos;
					updateWidget(ctx);
					return {
						content: [{ type: "text", text: JSON.stringify({ todos }, null, 2) }],
						details: { action: "add", todos, added: newTodo },
					};
				}

				case "update": {
					if (!params.id) {
						return {
							content: [{ type: "text", text: "Error: id required for update" }],
							details: { action: "update", error: "id required" },
						};
					}
					const index = todos.findIndex((t) => t.id === params.id);
					if (index === -1) {
						return {
							content: [{ type: "text", text: `Error: todo ${params.id} not found` }],
							details: { action: "update", error: "not found" },
						};
					}
					const existing = todos[index];
					const updated: TodoItem = {
						...existing,
						...(params.content !== undefined && { content: params.content }),
						...(params.status !== undefined && { status: params.status }),
						...(params.priority !== undefined && { priority: params.priority }),
					};
					todos[index] = updated;
					saveTodos(ctx.cwd, config, todos, sessionId);
					_currentTodos = todos;
					updateWidget(ctx);
					return {
						content: [{ type: "text", text: JSON.stringify({ todos }, null, 2) }],
						details: { action: "update", todos, updated },
					};
				}

				case "remove": {
					if (!params.id) {
						return {
							content: [{ type: "text", text: "Error: id required for remove" }],
							details: { action: "remove", error: "id required" },
						};
					}
					const removeIndex = todos.findIndex((t) => t.id === params.id);
					if (removeIndex === -1) {
						return {
							content: [{ type: "text", text: `Error: todo ${params.id} not found` }],
							details: { action: "remove", error: "not found" },
						};
					}
					const removed = todos.splice(removeIndex, 1)[0];
					saveTodos(ctx.cwd, config, todos, sessionId);
					_currentTodos = todos;
					updateWidget(ctx);
					return {
						content: [{ type: "text", text: JSON.stringify({ todos }, null, 2) }],
						details: { action: "remove", todos, removed },
					};
				}

				default: {
					_currentTodos = todos;
					updateWidget(ctx);
					return {
						content: [{ type: "text", text: JSON.stringify({ todos }, null, 2) }],
						details: { action: "list", todos },
					};
				}
			}
		},

		renderCall(args, theme) {
			try {
				const action = (args.action as string) || "list";
				const id = args.id as string | undefined;
				const content = args.content as string | undefined;

				let text = theme.fg("toolTitle", theme.bold("Todo ")) + theme.fg("accent", action);
				if (id) text += ` ${theme.fg("dim", id)}`;
				if (content) text += ` ${theme.fg("muted", `"${content.slice(0, 30)}${content.length > 30 ? "..." : ""}"`)}`;

				return new Text(text, 0, 0);
			} catch {
				return new Text("Todo", 0, 0);
			}
		},

		renderResult(result, { expanded }, theme) {
			try {
				const details = result.details as
					| {
							action?: string;
							todos?: TodoItem[];
							added?: TodoItem;
							updated?: TodoItem;
							removed?: TodoItem;
							error?: string;
					  }
					| undefined;

				if (details?.error) {
					return new Text(theme.fg("error", `Error: ${details.error}`), 0, 0);
				}

				const todos = details?.todos || [];
				let prefix = "";

				if (details?.action === "add" && details.added) {
					prefix = `${theme.fg("success", "OK Added: ")}${theme.fg("text", details.added.content)}\n\n`;
				} else if (details?.action === "update" && details.updated) {
					prefix = `${theme.fg("success", "OK Updated: ")}${theme.fg("text", details.updated.content)}\n\n`;
				} else if (details?.action === "remove" && details.removed) {
					prefix = `${theme.fg("success", "OK Removed: ")}${theme.fg("dim", details.removed.content)}\n\n`;
				}

				return new Text(prefix + renderTodoList(todos, theme, expanded), 0, 0);
			} catch {
				return new Text("(render error)", 0, 0);
			}
		},
	});

	// ==========================================================================
	// /todos command - Show todos in UI
	// ==========================================================================
	pi.registerCommand("todos", {
		description: "Show current todos",
		handler: async (_args, ctx) => {
			try {
				const config = loadConfig(ctx.cwd);
				const sessionId = getSessionId(ctx);
				const store = loadTodos(ctx.cwd, config, sessionId);

				if (!ctx.hasUI) {
					console.log(JSON.stringify(store.todos, null, 2));
					return;
				}

				if (store.todos.length === 0) {
					ctx.ui.notify("No todos", "info");
					return;
				}

				const summary = {
					pending: store.todos.filter((t) => t.status === "pending").length,
					in_progress: store.todos.filter((t) => t.status === "in_progress").length,
					completed: store.todos.filter((t) => t.status === "completed").length,
					cancelled: store.todos.filter((t) => t.status === "cancelled").length,
				};

				ctx.ui.notify(
					`${store.todos.length} todos: ${summary.in_progress} in progress, ${summary.pending} pending, ${summary.completed} done`,
					"info"
				);
			} catch (e) {
				console.error("[octo-todos] /todos command error:", e);
			}
		},
	});
}
