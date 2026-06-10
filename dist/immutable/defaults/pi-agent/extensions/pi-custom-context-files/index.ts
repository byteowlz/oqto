/**
 * Custom Context Files Extension
 *
 * Automatically loads additional context files beyond AGENTS.md/CLAUDE.md.
 * Configured via context.json in ~/.pi/agent/ (global) and .pi/ (project).
 * Project config merges with global config (project takes precedence).
 *
 * Configuration format:
 * {
 *   "contextFiles": [
 *     {
 *       "names": ["USER.md", "USERS.md"],
 *       "optional": true
 *     },
 *     {
 *       "names": ["PERSONA.md", "PERSONALITY.md"],
 *       "optional": false
 *     }
 *   ]
 * }
 *
 * For each entry, files are tried in order (e.g., USERS.md takes precedence over USER.md).
 * Optional files are silently skipped if not found. Non-optional files warn if missing.
 *
 * Usage:
 * 1. Copy this entire directory to ~/.pi/agent/extensions/ or .pi/extensions/
 * 2. Create ~/.pi/agent/context.json for global config
 * 3. Create .pi/context.json for project-specific overrides
 * 4. The extension automatically loads the specified files at agent start
 */

import { readFileSync } from "node:fs";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

interface ContextFileEntry {
	names: string[];
	optional?: boolean;
}

interface ContextConfig {
	contextFiles?: ContextFileEntry[];
}

function loadContextConfig(ctx: ExtensionContext): ContextConfig | null {
	const configs: ContextConfig[] = [];
	const cwd = process.cwd();

	// Load global config from ~/.pi/agent/context.json
	const globalConfigPath = join(homedir(), ".pi", "agent", "context.json");
	if (existsSync(globalConfigPath)) {
		try {
			const content = readFileSync(globalConfigPath, "utf-8");
			configs.push(JSON.parse(content));
		} catch (err) {
			ctx.ui.notify(`Failed to parse global context.json: ${(err as Error).message}`, "error");
		}
	}

	// Load project config from .pi/context.json
	const projectConfigPath = join(cwd, ".pi", "context.json");
	if (existsSync(projectConfigPath)) {
		try {
			const content = readFileSync(projectConfigPath, "utf-8");
			configs.push(JSON.parse(content));
		} catch (err) {
			ctx.ui.notify(`Failed to parse project context.json: ${(err as Error).message}`, "error");
		}
	}

	if (configs.length === 0) {
		return null;
	}

	// Merge configs (project takes precedence over global)
	const result = configs.reduce(
		(merged, config) => {
			merged.contextFiles = (merged.contextFiles || []).concat(config.contextFiles || []);
			return Object.assign(merged, config);
		},
		{} as Partial<ContextConfig>
	);

	return result as ContextConfig;
}

function loadFirstAvailableFile(entry: ContextFileEntry, ctx: ExtensionContext): string | null {
	const cwd = process.cwd();

	for (const filename of entry.names) {
		const filePath = join(cwd, filename);
		if (existsSync(filePath)) {
			try {
				return readFileSync(filePath, "utf-8");
			} catch (err) {
				ctx.ui.notify(`Failed to read ${filename}: ${(err as Error).message}`, "error");
			}
		}
	}

	return null;
}

export default function customContextFilesExtension(pi: ExtensionAPI) {
	pi.on("before_agent_start", async (event, ctx) => {
		const config = loadContextConfig(ctx);

		if (!config || !config.contextFiles || config.contextFiles.length === 0) {
			return undefined;
		}

		let additionalContent = "";
		const loadedFiles: string[] = [];

		for (const entry of config.contextFiles) {
			const content = loadFirstAvailableFile(entry, ctx);

			if (content) {
				const filename = entry.names[0]; // Show the primary name
				additionalContent += `\n\n<!-- ${filename} -->\n${content}`;
				loadedFiles.push(filename);
			} else if (!entry.optional) {
				const fileNames = entry.names.join(" or ");
				ctx.ui.notify(`Required context file not found: ${fileNames}`, "warning");
			}
		}

		if (additionalContent) {
			ctx.ui.notify(`Loaded ${loadedFiles.length} custom context file(s): ${loadedFiles.join(", ")}`, "info");
			return {
				systemPrompt: event.systemPrompt + additionalContent,
			};
		}

		return undefined;
	});
}
