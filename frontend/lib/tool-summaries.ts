/**
 * Generate a human-readable one-liner describing what a tool call is doing.
 * Pure pattern matching on tool name + arguments -- no LLM needed.
 *
 * Uses i18next for translations instead of inline locale ternaries.
 */

import { i18n } from "@/lib/i18n";

function truncPath(path: string, max = 50): string {
	if (path.length <= max) return path;
	// Keep filename, truncate directory
	const parts = path.split("/");
	const file = parts[parts.length - 1];
	if (file.length >= max - 4) return `...${file.slice(-(max - 3))}`;
	return `.../${parts.slice(-2).join("/")}`;
}

function truncStr(s: string, max = 60): string {
	if (s.length <= max) return s;
	return `${s.slice(0, max - 3)}...`;
}

// -- Bash command patterns --

type BashPattern = {
	match: (cmd: string) => boolean;
	summary: (cmd: string) => string;
};

const bashPatterns: BashPattern[] = [
	// Package managers
	{
		match: (cmd) => /\b(npm|yarn|pnpm|bun)\s+(install|add|i)\b/.test(cmd),
		summary: () => i18n.t("tools.installingPackages"),
	},
	{
		match: (cmd) => /\b(pip|uv)\s+(install|add)\b/.test(cmd),
		summary: () => i18n.t("tools.installingPythonPackages"),
	},
	{
		match: (cmd) => /\bcargo\s+(install|add|build)\b/.test(cmd),
		summary: (cmd) => {
			if (/\bbuild\b/.test(cmd)) return i18n.t("tools.buildingRustProject");
			return i18n.t("tools.installingRustPackages");
		},
	},
	// Git
	{
		match: (cmd) => /\bgit\s+clone\b/.test(cmd),
		summary: () => i18n.t("tools.cloningGitRepo"),
	},
	{
		match: (cmd) => /\bgit\s+pull\b/.test(cmd),
		summary: () => i18n.t("tools.pullingLatestChanges"),
	},
	{
		match: (cmd) => /\bgit\s+push\b/.test(cmd),
		summary: () => i18n.t("tools.pushingChanges"),
	},
	{
		match: (cmd) => /\bgit\s+commit\b/.test(cmd),
		summary: () => i18n.t("tools.committingChanges"),
	},
	{
		match: (cmd) => /\bgit\s+(status|diff|log)\b/.test(cmd),
		summary: (cmd) => {
			if (/\bstatus\b/.test(cmd)) return i18n.t("tools.checkingGitStatus");
			if (/\bdiff\b/.test(cmd)) return i18n.t("tools.showingChanges");
			return i18n.t("tools.viewingGitLog");
		},
	},
	// Search
	{
		match: (cmd) => /\b(grep|rg|ag)\s/.test(cmd),
		summary: (cmd) => {
			const m = cmd.match(/(?:grep|rg|ag)\s+(?:-[^\s]*\s+)*['"]?([^'"|\s]+)/);
			const query = m ? m[1] : null;
			if (query) {
				return i18n.t("tools.searchingFor", { query: truncStr(query, 30) });
			}
			return i18n.t("tools.searchingFiles");
		},
	},
	// Find
	{
		match: (cmd) => /\bfind\s/.test(cmd),
		summary: () => i18n.t("tools.findingFiles"),
	},
	// Directory listing
	{
		match: (cmd) => /\b(ls|dir|tree)\s/.test(cmd) || /^ls\s*$/.test(cmd.trim()),
		summary: () => i18n.t("tools.listingFiles"),
	},
	// File operations
	{
		match: (cmd) => /\b(cat|head|tail|less|more)\s/.test(cmd),
		summary: (cmd) => {
			const m = cmd.match(/(?:cat|head|tail|less|more)\s+(.+?)(?:\s*[|;]|$)/);
			const file = m ? truncPath(m[1].trim()) : null;
			if (file) return i18n.t("tools.readingPath", { path: file });
			return i18n.t("tools.readingFile");
		},
	},
	{
		match: (cmd) => /\bmkdir\b/.test(cmd),
		summary: () => i18n.t("tools.creatingDirectory"),
	},
	{
		match: (cmd) => /\b(rm|trash)\s/.test(cmd),
		summary: () => i18n.t("tools.removingFiles"),
	},
	{
		match: (cmd) => /\b(cp|rsync)\s/.test(cmd),
		summary: () => i18n.t("tools.copyingFiles"),
	},
	{
		match: (cmd) => /\bmv\s/.test(cmd),
		summary: () => i18n.t("tools.movingFiles"),
	},
	// Build/compile
	{
		match: (cmd) => /\b(make|cmake|ninja)\b/.test(cmd),
		summary: () => i18n.t("tools.buildingProject"),
	},
	// Test
	{
		match: (cmd) =>
			/\b(test|jest|vitest|pytest|cargo\s+test|bun\s+test|npm\s+test)\b/.test(
				cmd,
			),
		summary: () => i18n.t("tools.runningTests"),
	},
	// Docker
	{
		match: (cmd) => /\b(docker|podman)\s/.test(cmd),
		summary: () => i18n.t("tools.runningContainerCommand"),
	},
	// curl/wget
	{
		match: (cmd) => /\b(curl|wget)\s/.test(cmd),
		summary: () => i18n.t("tools.fetchingUrl"),
	},
	// Python
	{
		match: (cmd) => /\b(python3?|uv\s+run)\s/.test(cmd),
		summary: () => i18n.t("tools.runningPythonScript"),
	},
	// SSH/remote
	{
		match: (cmd) => /\bssh\s/.test(cmd),
		summary: () => i18n.t("tools.runningRemoteCommand"),
	},
	// systemctl
	{
		match: (cmd) => /\bsystemctl\s/.test(cmd),
		summary: () => i18n.t("tools.managingService"),
	},
	// Web search tools
	{
		match: (cmd) => /\b(sx|exa-web-search|exa-code-context)\s/.test(cmd),
		summary: (cmd) => {
			const m = cmd.match(/(?:sx|exa-web-search|exa-code-context)\s+['"]([^'"]+)/);
			const query = m ? m[1] : null;
			if (query) return i18n.t("tools.searchingQuery", { query: truncStr(query, 40) });
			return i18n.t("tools.webSearch");
		},
	},
	// Scheduler
	{
		match: (cmd) => /\bskdlr\s/.test(cmd),
		summary: () => i18n.t("tools.managingSchedule"),
	},
	// tmpltr/sldr
	{
		match: (cmd) => /\btmpltr\s/.test(cmd),
		summary: () => i18n.t("tools.generatingDocument"),
	},
	{
		match: (cmd) => /\bsldr\s/.test(cmd),
		summary: () => i18n.t("tools.buildingPresentation"),
	},
	// agntz memory
	{
		match: (cmd) => /\bagntz\s+memory\b/.test(cmd),
		summary: (cmd) => {
			if (/\bsearch\b/.test(cmd)) return i18n.t("tools.searchingMemories");
			if (/\badd\b/.test(cmd)) return i18n.t("tools.savingToMemory");
			if (/\blist\b/.test(cmd)) return i18n.t("tools.listingMemories");
			return i18n.t("tools.memoryOperation");
		},
	},
];

function summarizeBash(command: string): string | null {
	const cmd = command.trim();
	for (const pattern of bashPatterns) {
		if (pattern.match(cmd)) {
			return pattern.summary(cmd);
		}
	}
	// Fallback: show truncated command
	return null;
}

// -- Main entry point --
// The locale parameter is kept for API compatibility but no longer used
// internally. i18n.t() reads the current language from the i18n instance.

export function getToolSummary(
	toolName: string,
	input: Record<string, unknown> | undefined,
	_locale?: string,
): string | null {
	const name = toolName.toLowerCase();

	if (name === "bash" || name === "execute_command") {
		const cmd =
			(input?.command as string) ?? (input?.cmd as string) ?? null;
		if (cmd) {
			const summary = summarizeBash(cmd);
			if (summary) return summary;
			// Fallback: first meaningful part of command
			const clean = cmd
				.replace(/^(cd\s+[^\s;]+\s*[;&|]+\s*)+/, "") // strip leading cd
				.replace(/\s*2>&1.*$/, "") // strip redirects
				.replace(/\s*\|.*$/, "") // strip pipes (keep first command)
				.trim();
			return truncStr(clean, 60);
		}
	}

	if (name === "read") {
		const path = (input?.path as string) ?? null;
		if (path) return i18n.t("tools.readingPath", { path: truncPath(path) });
	}

	if (name === "write") {
		const path = (input?.path as string) ?? null;
		if (path) return i18n.t("tools.writingPath", { path: truncPath(path) });
	}

	if (name === "edit") {
		const path = (input?.path as string) ?? null;
		if (path) return i18n.t("tools.editingPath", { path: truncPath(path) });
	}

	if (name === "glob") {
		const pattern = (input?.pattern as string) ?? null;
		if (pattern) return i18n.t("tools.findingPattern", { pattern: truncStr(pattern, 40) });
	}

	if (name === "grep") {
		const pattern = (input?.pattern as string) ?? (input?.query as string) ?? null;
		if (pattern) return i18n.t("tools.searchingFor", { query: truncStr(pattern, 30) });
	}

	if (name === "todowrite" || name === "todo_write") {
		return i18n.t("tools.updatingTaskList");
	}

	if (name === "todoread" || name === "todo_read" || name === "todo") {
		const action = (input?.action as string) ?? null;
		if (action === "add") return i18n.t("tools.addingTask");
		if (action === "update") return i18n.t("tools.updatingTask");
		if (action === "remove") return i18n.t("tools.removingTask");
		return i18n.t("tools.managingTasks");
	}

	if (name === "self_reflection" || name === "selfReflection") {
		return i18n.t("tools.checkingContext");
	}

	if (name.includes("browser") || name.includes("screenshot")) {
		return i18n.t("tools.browserInteraction");
	}

	if (name.includes("search") || name.includes("web")) {
		return i18n.t("tools.webSearch");
	}

	// No summary available
	return null;
}
