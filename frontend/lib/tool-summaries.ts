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

function stripLeadingEnvAssignments(command: string): string {
	let rest = command.trim();
	const envPrefix = /^[A-Za-z_][A-Za-z0-9_]*=[^\s]+\s+/;
	while (envPrefix.test(rest)) {
		rest = rest.replace(envPrefix, "").trimStart();
	}
	return rest;
}

function primaryCommandSegment(command: string): string {
	const noEnv = stripLeadingEnvAssignments(command);
	const noLeadingCd = noEnv.replace(/^(cd\s+[^\s;]+\s*[;&|]+\s*)+/, "").trim();
	return noLeadingCd.split(/[|;]/)[0]?.trim() ?? noLeadingCd;
}

function summarizeAgentBrowser(command: string): string | null {
	const normalized = stripLeadingEnvAssignments(command);
	const m = normalized.match(/^(agent-browser|oqto-browser)\s+([a-z-]+)/i);
	if (!m) return null;
	const sub = (m[2] || "").toLowerCase();
	switch (sub) {
		case "open":
			return i18n.t("tools.browserOpen", { defaultValue: "Opening browser" });
		case "snapshot":
			return i18n.t("tools.browserSnapshot", {
				defaultValue: "Capturing browser snapshot",
			});
		case "click":
			return i18n.t("tools.browserClick", {
				defaultValue: "Clicking in browser",
			});
		case "fill":
			return i18n.t("tools.browserFill", {
				defaultValue: "Filling browser input",
			});
		case "press":
			return i18n.t("tools.browserPress", {
				defaultValue: "Pressing key in browser",
			});
		case "screenshot":
			return i18n.t("tools.browserScreenshot", {
				defaultValue: "Taking browser screenshot",
			});
		case "console":
			return i18n.t("tools.browserConsole", {
				defaultValue: "Reading browser console",
			});
		case "eval":
			return i18n.t("tools.browserEval", {
				defaultValue: "Running browser script",
			});
		case "wait":
			return i18n.t("tools.browserWait", {
				defaultValue: "Waiting in browser",
			});
		case "scroll":
			return i18n.t("tools.browserScroll", {
				defaultValue: "Scrolling browser page",
			});
		case "close":
			return i18n.t("tools.browserClose", { defaultValue: "Closing browser" });
		default:
			return i18n.t("tools.browserInteraction");
	}
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
		match: (cmd) =>
			/^cargo\s+/.test(cmd.replace(/^(cd\s+[^\s;]+\s*[;&|]+\s*)+/, "").trim()),
		summary: (cmd) => {
			const clean = cmd.replace(/^(cd\s+[^\s;]+\s*[;&|]+\s*)+/, "").trim();
			if (/^cargo\s+install\b/.test(clean))
				return i18n.t("tools.installingRustPackages");
			if (/^cargo\s+(build|b)\b/.test(clean))
				return i18n.t("tools.buildingRustProject");
			if (/^cargo\s+(check|c)\b/.test(clean))
				return i18n.t("tools.checkingRustProject", {
					defaultValue: "Checking Rust project",
				});
			if (/^cargo\s+(test|t)\b/.test(clean))
				return i18n.t("tools.runningTests");
			if (/^cargo\s+(clippy)\b/.test(clean))
				return i18n.t("tools.lintingRustProject", {
					defaultValue: "Linting Rust project",
				});
			if (/^cargo\s+fmt\b/.test(clean))
				return i18n.t("tools.formattingRustCode", {
					defaultValue: "Formatting Rust code",
				});
			if (/^cargo\s+add\b/.test(clean))
				return i18n.t("tools.installingRustPackages");
			if (/^cargo\s+run\b/.test(clean))
				return i18n.t("tools.runningRustProject", {
					defaultValue: "Running Rust project",
				});
			return i18n.t("tools.runningCargoCommand", {
				defaultValue: "Running cargo command",
			});
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
		match: (cmd) =>
			/^(cat|head|tail|less|more)\b/.test(primaryCommandSegment(cmd)),
		summary: (cmd) => {
			const primary = primaryCommandSegment(cmd);
			// Extract file path, stripping flags like -n 20, -20, -c 100, etc.
			const m = primary.match(/(?:cat|head|tail|less|more)\s+(.+?)$/);
			if (m) {
				// Remove flags and their numeric arguments to isolate the file path
				const cleaned = m[1]
					.replace(/(?:^|\s)-[a-zA-Z]+\s*\d*/g, "") // -n 20, -c 100
					.replace(/(?:^|\s)-\d+/g, "") // -20 (shorthand)
					.trim();
				if (cleaned && !cleaned.startsWith("-")) {
					return i18n.t("tools.readingPath", {
						path: truncPath(cleaned),
					});
				}
			}
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
			const m = cmd.match(
				/(?:sx|exa-web-search|exa-code-context)\s+['"]([^'"]+)/,
			);
			const query = m ? m[1] : null;
			if (query)
				return i18n.t("tools.searchingQuery", { query: truncStr(query, 40) });
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
	const browserSummary = summarizeAgentBrowser(cmd);
	if (browserSummary) return browserSummary;
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
		const cmd = (input?.command as string) ?? (input?.cmd as string) ?? null;
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
		// Input not yet available (streaming) — return generic label
		return i18n.t("tools.runningCommand", {
			defaultValue: "Running command",
		});
	}

	if (name === "read") {
		const path =
			(input?.path as string) ??
			(input?.file_path as string) ??
			(input?.filePath as string) ??
			null;
		if (path) return i18n.t("tools.readingPath", { path: truncPath(path) });
		return i18n.t("tools.readingFile");
	}

	if (name === "write") {
		const path =
			(input?.path as string) ??
			(input?.file_path as string) ??
			(input?.filePath as string) ??
			null;
		if (path) return i18n.t("tools.writingPath", { path: truncPath(path) });
		return i18n.t("tools.writingFile", { defaultValue: "Writing file" });
	}

	if (name === "edit") {
		const path =
			(input?.path as string) ??
			(input?.file_path as string) ??
			(input?.filePath as string) ??
			null;
		if (path) return i18n.t("tools.editingPath", { path: truncPath(path) });
		return i18n.t("tools.editingFile", { defaultValue: "Editing file" });
	}

	if (name === "glob") {
		const pattern = (input?.pattern as string) ?? null;
		if (pattern)
			return i18n.t("tools.findingPattern", { pattern: truncStr(pattern, 40) });
	}

	if (name === "grep") {
		const pattern =
			(input?.pattern as string) ?? (input?.query as string) ?? null;
		if (pattern)
			return i18n.t("tools.searchingFor", { query: truncStr(pattern, 30) });
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
