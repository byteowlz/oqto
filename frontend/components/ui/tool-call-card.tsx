"use client";

import type { OpenCodePart } from "@/lib/opencode-client";
import { cn } from "@/lib/utils";
import {
	Bot,
	CheckCircle2,
	CheckSquare,
	ChevronRight,
	CircleDot,
	Clock,
	FileEdit,
	FilePlus,
	FileText,
	FolderOpen,
	GitBranch,
	Globe,
	ListTodo,
	Loader2,
	Search,
	Square,
	Terminal,
	Wrench,
	XCircle,
} from "lucide-react";
import { useState } from "react";
import { CopyButton, MarkdownRenderer } from "./markdown-renderer";

interface ToolCallCardProps {
	part: OpenCodePart;
	defaultCollapsed?: boolean;
	hideTodoTools?: boolean;
}

// Todo item structure from todowrite tool
export interface TodoItem {
	id: string;
	content: string;
	status: "pending" | "in_progress" | "completed" | "cancelled";
	priority: "high" | "medium" | "low";
}

// Type guards for different tool inputs
function isTodoList(
	input: Record<string, unknown>,
): input is { todos: TodoItem[] } {
	return (
		Array.isArray(input.todos) &&
		input.todos.length > 0 &&
		input.todos.every(
			(t: unknown) =>
				typeof t === "object" && t !== null && "content" in t && "status" in t,
		)
	);
}

function isFileRead(
	input: Record<string, unknown>,
): input is { filePath: string; offset?: number; limit?: number } {
	return (
		typeof input.filePath === "string" &&
		!("content" in input) &&
		!("oldString" in input)
	);
}

function isFileWrite(
	input: Record<string, unknown>,
): input is { filePath: string; content: string } {
	return (
		typeof input.filePath === "string" &&
		typeof input.content === "string" &&
		!("oldString" in input)
	);
}

function isFileEdit(input: Record<string, unknown>): input is {
	filePath: string;
	oldString: string;
	newString: string;
	replaceAll?: boolean;
} {
	return (
		typeof input.filePath === "string" &&
		typeof input.oldString === "string" &&
		typeof input.newString === "string"
	);
}

function isGlobSearch(
	input: Record<string, unknown>,
): input is { pattern: string; path?: string } {
	return (
		typeof input.pattern === "string" &&
		!("include" in input) &&
		!("command" in input)
	);
}

function isGrepSearch(
	input: Record<string, unknown>,
): input is { pattern: string; path?: string; include?: string } {
	return typeof input.pattern === "string" && !("command" in input);
}

function isBashCommand(input: Record<string, unknown>): input is {
	command: string;
	description?: string;
	workdir?: string;
	timeout?: number;
} {
	return typeof input.command === "string";
}

function isTaskCall(input: Record<string, unknown>): input is {
	prompt: string;
	description?: string;
	subagent_type?: string;
	session_id?: string;
} {
	return (
		typeof input.prompt === "string" &&
		(typeof input.subagent_type === "string" ||
			typeof input.description === "string")
	);
}

function isWebFetch(
	input: Record<string, unknown>,
): input is { url: string; format?: string; timeout?: number } {
	return typeof input.url === "string";
}

function isListDir(
	input: Record<string, unknown>,
): input is { path?: string; ignore?: string[] } {
	// List tool typically only has path and optionally ignore
	return (
		(typeof input.path === "string" || Object.keys(input).length === 0) &&
		!("filePath" in input) &&
		!("pattern" in input) &&
		!("command" in input) &&
		!("prompt" in input) &&
		!("url" in input) &&
		!("todos" in input) &&
		!("content" in input) &&
		!("oldString" in input)
	);
}

function getPriorityColor(priority: string) {
	switch (priority) {
		case "high":
			return "text-red-500 dark:text-red-400";
		case "medium":
			return "text-yellow-600 dark:text-yellow-400";
		case "low":
			return "text-muted-foreground";
		default:
			return "text-muted-foreground";
	}
}

// Get tool-specific icon
function getToolIcon(toolName: string, input?: Record<string, unknown>) {
	const name = toolName.toLowerCase();

	// Check by tool name first
	if (name.includes("read") || name === "read") {
		return (
			<FileText className="w-3.5 h-3.5 text-blue-500 dark:text-blue-400" />
		);
	}
	if (name.includes("write") || name === "write") {
		return (
			<FilePlus className="w-3.5 h-3.5 text-green-600 dark:text-green-400" />
		);
	}
	if (name.includes("edit") || name === "edit") {
		return (
			<FileEdit className="w-3.5 h-3.5 text-amber-500 dark:text-amber-400" />
		);
	}
	if (name.includes("glob") || name === "glob") {
		return (
			<FolderOpen className="w-3.5 h-3.5 text-purple-600 dark:text-purple-400" />
		);
	}
	if (name.includes("grep") || name === "grep") {
		return (
			<Search className="w-3.5 h-3.5 text-orange-600 dark:text-orange-400" />
		);
	}
	if (name.includes("bash") || name === "bash") {
		return (
			<Terminal className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
		);
	}
	if (name.includes("todo") || name === "todowrite" || name === "todoread") {
		return (
			<ListTodo className="w-3.5 h-3.5 text-pink-600 dark:text-pink-400" />
		);
	}
	if (name.includes("task") || name === "task") {
		return <Bot className="w-3.5 h-3.5 text-cyan-600 dark:text-cyan-400" />;
	}
	if (
		name.includes("webfetch") ||
		name.includes("fetch") ||
		name === "webfetch"
	) {
		return <Globe className="w-3.5 h-3.5 text-sky-600 dark:text-sky-400" />;
	}
	if (name.includes("list") || name === "list") {
		return (
			<FolderOpen className="w-3.5 h-3.5 text-violet-600 dark:text-violet-400" />
		);
	}
	if (name.includes("git")) {
		return (
			<GitBranch className="w-3.5 h-3.5 text-orange-600 dark:text-orange-400" />
		);
	}
	if (name.includes("exa") || name.includes("search")) {
		return (
			<Search className="w-3.5 h-3.5 text-indigo-600 dark:text-indigo-400" />
		);
	}

	// Fallback: try to detect from input shape
	if (input) {
		if (isBashCommand(input))
			return (
				<Terminal className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
			);
		if (isTaskCall(input))
			return <Bot className="w-3.5 h-3.5 text-cyan-600 dark:text-cyan-400" />;
		if (isWebFetch(input))
			return <Globe className="w-3.5 h-3.5 text-sky-600 dark:text-sky-400" />;
		if (isTodoList(input))
			return (
				<ListTodo className="w-3.5 h-3.5 text-pink-600 dark:text-pink-400" />
			);
		if (isFileEdit(input))
			return (
				<FileEdit className="w-3.5 h-3.5 text-amber-500 dark:text-amber-400" />
			);
		if (isFileWrite(input))
			return (
				<FilePlus className="w-3.5 h-3.5 text-green-600 dark:text-green-400" />
			);
		if (isFileRead(input))
			return (
				<FileText className="w-3.5 h-3.5 text-blue-500 dark:text-blue-400" />
			);
		if (isGrepSearch(input))
			return (
				<Search className="w-3.5 h-3.5 text-orange-600 dark:text-orange-400" />
			);
		if (isGlobSearch(input))
			return (
				<FolderOpen className="w-3.5 h-3.5 text-purple-600 dark:text-purple-400" />
			);
	}

	return <Wrench className="w-3.5 h-3.5 text-muted-foreground" />;
}

// Get file extension for syntax highlighting
function getFileExtension(filePath: string): string {
	const ext = filePath.split(".").pop()?.toLowerCase() || "";
	const extMap: Record<string, string> = {
		ts: "typescript",
		tsx: "tsx",
		js: "javascript",
		jsx: "jsx",
		py: "python",
		rb: "ruby",
		rs: "rust",
		go: "go",
		java: "java",
		kt: "kotlin",
		swift: "swift",
		css: "css",
		scss: "scss",
		less: "less",
		html: "html",
		xml: "xml",
		svg: "xml",
		json: "json",
		yaml: "yaml",
		yml: "yaml",
		toml: "toml",
		md: "markdown",
		sh: "bash",
		bash: "bash",
		zsh: "bash",
		sql: "sql",
		graphql: "graphql",
	};
	return extMap[ext] || "text";
}

function TodoListRenderer({ todos }: { todos: TodoItem[] }) {
	return (
		<div className="space-y-1.5">
			{todos.map((todo, idx) => (
				<div
					key={todo.id || idx}
					className={cn(
						"flex items-start gap-2 py-1 px-2 rounded",
						todo.status === "completed" && "opacity-60",
						todo.status === "cancelled" && "opacity-40 line-through",
					)}
				>
					{/* Status checkbox */}
					{todo.status === "completed" ? (
						<CheckSquare className="w-4 h-4 text-primary flex-shrink-0 mt-0.5" />
					) : todo.status === "in_progress" ? (
						<CircleDot className="w-4 h-4 text-primary animate-pulse flex-shrink-0 mt-0.5" />
					) : todo.status === "cancelled" ? (
						<XCircle className="w-4 h-4 text-muted-foreground flex-shrink-0 mt-0.5" />
					) : (
						<Square className="w-4 h-4 text-muted-foreground flex-shrink-0 mt-0.5" />
					)}

					{/* Content */}
					<span
						className={cn(
							"text-sm flex-1",
							todo.status === "completed"
								? "text-muted-foreground"
								: "text-foreground",
						)}
					>
						{todo.content}
					</span>

					{/* Priority indicator */}
					{todo.priority && (
						<span
							className={cn(
								"text-[10px] uppercase tracking-wide flex-shrink-0",
								getPriorityColor(todo.priority),
							)}
						>
							{todo.priority}
						</span>
					)}
				</div>
			))}
		</div>
	);
}

function FileReadRenderer({
	input,
}: { input: { filePath: string; offset?: number; limit?: number } }) {
	return (
		<div className="space-y-2">
			<div className="flex items-center gap-2 text-sm">
				<FileText className="w-4 h-4 text-primary" />
				<span className="text-foreground font-mono text-xs break-all">
					{input.filePath}
				</span>
			</div>
			{(input.offset !== undefined || input.limit !== undefined) && (
				<div className="flex items-center gap-3 text-xs text-foreground/60 dark:text-muted-foreground">
					{input.offset !== undefined && (
						<span>
							From line:{" "}
							<span className="text-foreground/80">{input.offset}</span>
						</span>
					)}
					{input.limit !== undefined && (
						<span>
							Lines: <span className="text-foreground/80">{input.limit}</span>
						</span>
					)}
				</div>
			)}
		</div>
	);
}

function FileWriteRenderer({
	input,
}: { input: { filePath: string; content: string } }) {
	const ext = getFileExtension(input.filePath);
	const lineCount = input.content.split("\n").length;

	return (
		<div className="space-y-2">
			<div className="flex items-center gap-2 text-sm">
				<FilePlus className="w-4 h-4 text-primary" />
				<span className="text-foreground font-mono text-xs break-all">
					{input.filePath}
				</span>
				<span className="text-xs text-foreground/60 dark:text-muted-foreground">
					({lineCount} lines)
				</span>
			</div>
			<div
				className="rounded-md overflow-hidden"
				style={{ backgroundColor: "var(--code-bg)" }}
			>
				<MarkdownRenderer
					content={`\`\`\`${ext}\n${input.content.length > 1500 ? `${input.content.slice(0, 1500)}\n// ... content truncated` : input.content}\n\`\`\``}
					className="text-xs [&_pre]:max-h-48 [&_pre]:overflow-y-auto"
				/>
			</div>
		</div>
	);
}

function FileEditRenderer({
	input,
}: {
	input: {
		filePath: string;
		oldString: string;
		newString: string;
		replaceAll?: boolean;
	};
}) {
	const ext = getFileExtension(input.filePath);

	return (
		<div className="space-y-2">
			<div className="flex items-center gap-2 text-sm">
				<FileEdit className="w-4 h-4 text-primary" />
				<span className="text-foreground font-mono text-xs break-all">
					{input.filePath}
				</span>
				{input.replaceAll && (
					<span className="text-[10px] uppercase tracking-wide text-amber-600 dark:text-amber-400 bg-amber-500/10 px-1.5 py-0.5 rounded">
						Replace All
					</span>
				)}
			</div>

			<div className="grid grid-cols-1 gap-2">
				<div>
					<div className="text-[10px] uppercase tracking-wide text-muted-foreground mb-1 flex items-center gap-1">
						<span className="w-3 h-3 rounded bg-muted flex items-center justify-center text-muted-foreground">
							-
						</span>
						Old
					</div>
					<div className="bg-muted/50 border border-border rounded-md overflow-hidden">
						<MarkdownRenderer
							content={`\`\`\`${ext}\n${input.oldString.length > 500 ? `${input.oldString.slice(0, 500)}\n// ... truncated` : input.oldString}\n\`\`\``}
							className="text-xs [&_pre]:max-h-32 [&_pre]:overflow-y-auto [&_.markdown-content]:bg-transparent"
						/>
					</div>
				</div>

				<div>
					<div className="text-[10px] uppercase tracking-wide text-primary mb-1 flex items-center gap-1">
						<span className="w-3 h-3 rounded bg-primary/20 flex items-center justify-center text-primary">
							+
						</span>
						New
					</div>
					<div className="bg-primary/5 border border-primary/20 rounded-md overflow-hidden">
						<MarkdownRenderer
							content={`\`\`\`${ext}\n${input.newString.length > 500 ? `${input.newString.slice(0, 500)}\n// ... truncated` : input.newString}\n\`\`\``}
							className="text-xs [&_pre]:max-h-32 [&_pre]:overflow-y-auto [&_.markdown-content]:bg-transparent"
						/>
					</div>
				</div>
			</div>
		</div>
	);
}

function GlobSearchRenderer({
	input,
}: { input: { pattern: string; path?: string } }) {
	return (
		<div className="flex items-center gap-2 text-sm flex-wrap">
			<FolderOpen className="w-4 h-4 text-primary" />
			<span className="text-foreground/60 dark:text-muted-foreground">
				Pattern:
			</span>
			<code className="text-primary bg-primary/10 px-1.5 py-0.5 rounded text-xs font-mono">
				{input.pattern}
			</code>
			{input.path && (
				<>
					<span className="text-foreground/60 dark:text-muted-foreground">
						in
					</span>
					<code className="text-foreground/80 font-mono text-xs">
						{input.path}
					</code>
				</>
			)}
		</div>
	);
}

function GrepSearchRenderer({
	input,
}: { input: { pattern: string; path?: string; include?: string } }) {
	return (
		<div className="flex items-center gap-2 text-sm flex-wrap">
			<Search className="w-4 h-4 text-primary" />
			<span className="text-foreground/60 dark:text-muted-foreground">
				Search:
			</span>
			<code className="text-primary bg-primary/10 px-1.5 py-0.5 rounded text-xs font-mono">
				{input.pattern}
			</code>
			{input.include && (
				<>
					<span className="text-foreground/60 dark:text-muted-foreground">
						in
					</span>
					<code className="text-foreground/80 bg-muted px-1.5 py-0.5 rounded text-xs font-mono">
						{input.include}
					</code>
				</>
			)}
			{input.path && (
				<>
					<span className="text-foreground/60 dark:text-muted-foreground">
						at
					</span>
					<code className="text-foreground/80 font-mono text-xs">
						{input.path}
					</code>
				</>
			)}
		</div>
	);
}

function BashCommandRenderer({
	input,
}: {
	input: {
		command: string;
		description?: string;
		workdir?: string;
		timeout?: number;
	};
}) {
	return (
		<div className="space-y-2">
			{input.description && (
				<div className="flex items-center gap-2 text-sm text-foreground">
					<Terminal className="w-4 h-4 text-emerald-600 dark:text-emerald-400 flex-shrink-0" />
					<span>{input.description}</span>
				</div>
			)}
			<div
				className="rounded-md p-3 font-mono text-xs border border-border"
				style={{ backgroundColor: "var(--code-bg)" }}
			>
				{input.workdir && (
					<div className="text-foreground/60 dark:text-muted-foreground mb-1 flex items-center gap-2">
						<span className="text-emerald-600 dark:text-emerald-400">$</span>
						<span>cd {input.workdir}</span>
					</div>
				)}
				<div className="text-foreground flex items-start gap-2">
					<span className="text-emerald-600 dark:text-emerald-400">$</span>
					<span className="break-all">{input.command}</span>
				</div>
			</div>
			{input.timeout && input.timeout !== 120000 && (
				<div className="text-[10px] text-foreground/60 dark:text-muted-foreground">
					Timeout: {(input.timeout / 1000).toFixed(0)}s
				</div>
			)}
		</div>
	);
}

function TaskCallRenderer({
	input,
}: {
	input: {
		prompt: string;
		description?: string;
		subagent_type?: string;
		session_id?: string;
	};
}) {
	const agentTypeLabels: Record<string, { label: string; className: string }> =
		{
			general: {
				label: "General Agent",
				className: "text-cyan-600 dark:text-cyan-400",
			},
			explore: {
				label: "Code Explorer",
				className: "text-purple-600 dark:text-purple-400",
			},
			"git-committer": {
				label: "Git Committer",
				className: "text-orange-600 dark:text-orange-400",
			},
			docs: {
				label: "Documentation",
				className: "text-blue-600 dark:text-blue-400",
			},
		};

	const agentInfo = input.subagent_type
		? agentTypeLabels[input.subagent_type]
		: null;

	return (
		<div className="space-y-2">
			<div className="flex items-center gap-2 flex-wrap">
				<Bot className="w-4 h-4 text-cyan-600 dark:text-cyan-400 flex-shrink-0" />
				{agentInfo ? (
					<span className={cn("text-sm font-medium", agentInfo.className)}>
						{agentInfo.label}
					</span>
				) : input.subagent_type ? (
					<span className="text-sm font-medium text-cyan-600 dark:text-cyan-400">
						{input.subagent_type}
					</span>
				) : null}
				{input.description && (
					<span className="text-xs text-foreground/60 dark:text-muted-foreground">
						- {input.description}
					</span>
				)}
			</div>

			<div
				className="rounded-md p-3 border border-border"
				style={{ backgroundColor: "var(--code-bg)" }}
			>
				<div className="text-[10px] uppercase tracking-wide text-foreground/60 dark:text-muted-foreground mb-1">
					Task
				</div>
				<div className="text-sm text-foreground whitespace-pre-wrap">
					{input.prompt.length > 500
						? `${input.prompt.slice(0, 500)}...`
						: input.prompt}
				</div>
			</div>

			{input.session_id && (
				<div className="text-[10px] text-foreground/60 dark:text-muted-foreground">
					Continuing session:{" "}
					<code className="text-foreground/80">{input.session_id}</code>
				</div>
			)}
		</div>
	);
}

function WebFetchRenderer({
	input,
}: { input: { url: string; format?: string; timeout?: number } }) {
	return (
		<div className="space-y-2">
			<div className="flex items-center gap-2">
				<Globe className="w-4 h-4 text-sky-600 dark:text-sky-400 flex-shrink-0" />
				<span className="text-sm text-foreground">Fetching URL</span>
			</div>
			<div
				className="rounded-md p-2 border border-border"
				style={{ backgroundColor: "var(--code-bg)" }}
			>
				<a
					href={input.url}
					target="_blank"
					rel="noopener noreferrer"
					className="text-xs font-mono text-sky-600 dark:text-sky-400 hover:text-sky-500 dark:hover:text-sky-300 break-all"
				>
					{input.url}
				</a>
			</div>
			{(input.format || input.timeout) && (
				<div className="flex items-center gap-3 text-[10px] text-foreground/60 dark:text-muted-foreground">
					{input.format && (
						<span>
							Format: <span className="text-foreground/80">{input.format}</span>
						</span>
					)}
					{input.timeout && (
						<span>
							Timeout:{" "}
							<span className="text-foreground/80">{input.timeout}s</span>
						</span>
					)}
				</div>
			)}
		</div>
	);
}

function ListDirRenderer({
	input,
}: { input: { path?: string; ignore?: string[] } }) {
	return (
		<div className="space-y-2">
			<div className="flex items-center gap-2 text-sm">
				<FolderOpen className="w-4 h-4 text-violet-600 dark:text-violet-400 flex-shrink-0" />
				<span className="text-foreground">List directory</span>
			</div>
			<div
				className="rounded-md p-2 border border-border"
				style={{ backgroundColor: "var(--code-bg)" }}
			>
				<code className="text-xs font-mono text-violet-600 dark:text-violet-400 break-all">
					{input.path || "(current directory)"}
				</code>
			</div>
			{input.ignore && input.ignore.length > 0 && (
				<div className="text-[10px] text-foreground/60 dark:text-muted-foreground">
					Ignoring:{" "}
					<span className="text-foreground/80">{input.ignore.join(", ")}</span>
				</div>
			)}
		</div>
	);
}

function FileOutputRenderer({
	output,
	filePath,
}: { output: string; filePath?: string }) {
	// Try to detect if this is file content with line numbers (from read tool)
	const hasLineNumbers = /^\s*\d+\|/.test(output) || /^\s*\d+\t/.test(output);
	const ext = filePath ? getFileExtension(filePath) : "text";

	if (hasLineNumbers) {
		return (
			<div
				className="rounded-md overflow-hidden"
				style={{ backgroundColor: "var(--code-bg)" }}
			>
				<pre className="text-xs text-foreground/70 dark:text-muted-foreground p-2 overflow-x-auto max-h-64 overflow-y-auto whitespace-pre font-mono">
					{output.length > 3000
						? `${output.slice(0, 3000)}\n... (truncated)`
						: output}
				</pre>
			</div>
		);
	}

	// For other file outputs, try syntax highlighting
	return (
		<div
			className="rounded-md overflow-hidden"
			style={{ backgroundColor: "var(--code-bg)" }}
		>
			<MarkdownRenderer
				content={`\`\`\`${ext}\n${output.length > 2000 ? `${output.slice(0, 2000)}\n// ... truncated` : output}\n\`\`\``}
				className="text-xs [&_pre]:max-h-64 [&_pre]:overflow-y-auto"
			/>
		</div>
	);
}

function getStatusIcon(
	status: string | undefined,
	toolName?: string,
	input?: Record<string, unknown>,
	output?: string,
) {
	// For running/pending, show spinner or clock
	if (status === "pending") {
		return <Clock className="w-3.5 h-3.5 text-muted-foreground" />;
	}
	if (status === "running") {
		return <Loader2 className="w-3.5 h-3.5 text-primary animate-spin" />;
	}

	// For completed status or tools with successful output, show tool-specific icon
	// This handles cases where status might be "error" but the output indicates success
	const hasSuccessOutput =
		output?.toLowerCase().includes("applied successfully") ||
		output?.toLowerCase().includes("success") ||
		output?.toLowerCase().includes("created") ||
		output?.toLowerCase().includes("written");

	if (status === "completed" || (toolName && hasSuccessOutput)) {
		if (toolName) {
			return getToolIcon(toolName, input);
		}
		return <CheckCircle2 className="w-3.5 h-3.5 text-primary" />;
	}

	if (status === "error") {
		return <XCircle className="w-3.5 h-3.5 text-red-500 dark:text-red-400" />;
	}

	// For unknown status, show tool-specific icon if available
	if (toolName) {
		return getToolIcon(toolName, input);
	}

	return <Wrench className="w-3.5 h-3.5 text-muted-foreground" />;
}

function getStatusClasses(status: string | undefined, output?: string) {
	// Check if output indicates success even if status is error
	const hasSuccessOutput =
		output?.toLowerCase().includes("applied successfully") ||
		output?.toLowerCase().includes("success") ||
		output?.toLowerCase().includes("created") ||
		output?.toLowerCase().includes("written");

	if (hasSuccessOutput) {
		return "border-border bg-card";
	}

	switch (status) {
		case "pending":
			return "border-border bg-card";
		case "running":
			return "border-primary/30 bg-primary/5";
		case "completed":
			return "border-border bg-card";
		case "error":
			return "border-red-500/30 dark:border-red-900/50 bg-red-500/5 dark:bg-red-950/20";
		default:
			return "border-border bg-card";
	}
}

function formatDuration(
	start: number | undefined,
	end: number | undefined,
): string {
	if (!start) return "";
	const endTime = end || Date.now();
	const duration = endTime - start;
	if (duration < 1000) return `${duration}ms`;
	if (duration < 60000) return `${(duration / 1000).toFixed(1)}s`;
	return `${Math.floor(duration / 60000)}m ${((duration % 60000) / 1000).toFixed(0)}s`;
}

export function ToolCallCard({
	part,
	defaultCollapsed = true,
	hideTodoTools = false,
}: ToolCallCardProps) {
	const [isOpen, setIsOpen] = useState(!defaultCollapsed);
	const { tool, state } = part;

	const toolName = tool || "Unknown Tool";
	const status = state?.status;
	const title = state?.title || toolName;
	const input = state?.input;
	const output = state?.output;
	const duration = formatDuration(state?.time?.start, state?.time?.end);

	// Hide todo tools if requested (they're shown in sidebar instead)
	if (hideTodoTools && toolName.toLowerCase().includes("todo")) {
		return null;
	}

	// Hide question tool - questions are shown in the UserQuestionDialog
	// instead of inline in the message stream
	if (toolName.toLowerCase() === "question") {
		return null;
	}

	// Check if we have meaningful content to show
	// For tool calls, input must have actual properties (not just an empty object)
	// and output must be a non-empty string
	const hasInput = input && Object.keys(input).length > 0;
	const hasOutput = output && output.trim().length > 0;
	const hasContent = hasInput || hasOutput;

	return (
		<div
			className={cn(
				"rounded-lg border transition-all duration-200",
				getStatusClasses(status, output),
			)}
		>
			<button
				type="button"
				onClick={() => hasContent && setIsOpen(!isOpen)}
				disabled={!hasContent}
				className={cn(
					"w-full flex items-center gap-2 px-3 py-2 text-left",
					hasContent && "cursor-pointer hover:bg-muted/50",
					!hasContent && "cursor-default",
				)}
			>
				{hasContent && (
					<ChevronRight
						className={cn(
							"w-4 h-4 text-muted-foreground transition-transform duration-200 flex-shrink-0",
							isOpen && "rotate-90",
						)}
					/>
				)}
				{!hasContent && <div className="w-4" />}

				{getStatusIcon(status, toolName, input, output)}

				<span className="flex-1 text-sm font-medium text-foreground truncate">
					{title}
				</span>

				{duration && (
					<span className="text-xs text-foreground/60 dark:text-muted-foreground flex-shrink-0">
						{duration}
					</span>
				)}
			</button>

			{isOpen && hasContent && (
				<div className="px-3 pb-3 space-y-2 border-t border-border pt-2">
					{input && Object.keys(input).length > 0 && (
						<div>
							{/* Render input based on tool type */}
							{isTodoList(input) ? (
								<>
									<div className="flex items-center justify-between mb-1">
										<span className="text-xs uppercase tracking-wide text-foreground/60 dark:text-muted-foreground">
											Todo List
										</span>
										<CopyButton text={JSON.stringify(input, null, 2)} />
									</div>
									<div
										className="rounded-md p-2 max-h-64 overflow-y-auto"
										style={{ backgroundColor: "var(--code-bg)" }}
									>
										<TodoListRenderer todos={input.todos} />
									</div>
								</>
							) : isTaskCall(input) ? (
								<TaskCallRenderer input={input} />
							) : isWebFetch(input) ? (
								<WebFetchRenderer input={input} />
							) : isBashCommand(input) ? (
								<BashCommandRenderer input={input} />
							) : isFileEdit(input) ? (
								<FileEditRenderer input={input} />
							) : isFileWrite(input) ? (
								<FileWriteRenderer input={input} />
							) : isFileRead(input) ? (
								<FileReadRenderer input={input} />
							) : isGlobSearch(input) ? (
								<GlobSearchRenderer input={input} />
							) : isGrepSearch(input) ? (
								<GrepSearchRenderer input={input} />
							) : isListDir(input) ? (
								<ListDirRenderer input={input} />
							) : (
								<>
									<div className="flex items-center justify-between mb-1">
										<span className="text-xs uppercase tracking-wide text-foreground/60 dark:text-muted-foreground">
											Input
										</span>
										<CopyButton text={JSON.stringify(input, null, 2)} />
									</div>
									<pre
										className="text-xs text-foreground/70 dark:text-muted-foreground rounded-md p-2 overflow-x-auto max-h-48 overflow-y-auto"
										style={{ backgroundColor: "var(--code-bg)" }}
									>
										{JSON.stringify(input, null, 2)}
									</pre>
								</>
							)}
						</div>
					)}

					{/* Hide output for todo lists since it's redundant */}
					{output && !(input && isTodoList(input)) && (
						<div>
							<div className="flex items-center justify-between mb-1">
								<span className="text-xs uppercase tracking-wide text-foreground/60 dark:text-muted-foreground">
									Output
								</span>
								<CopyButton text={output} />
							</div>
							{input && isFileRead(input) ? (
								<FileOutputRenderer output={output} filePath={input.filePath} />
							) : (
								<pre
									className="text-xs text-foreground/70 dark:text-muted-foreground rounded-md p-2 overflow-x-auto max-h-48 overflow-y-auto whitespace-pre-wrap"
									style={{ backgroundColor: "var(--code-bg)" }}
								>
									{output.length > 2000
										? `${output.slice(0, 2000)}\n... (truncated)`
										: output}
								</pre>
							)}
						</div>
					)}
				</div>
			)}
		</div>
	);
}

export default ToolCallCard;
