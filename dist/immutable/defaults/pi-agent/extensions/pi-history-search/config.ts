/**
 * Configuration loading for pi-history-search.
 *
 * Searched in order (first match wins), matching the repo convention:
 *   1. ./history-search.json                 (cwd)
 *   2. ./.pi/history-search.json             (project-local)
 *   3. ~/.pi/agent/history-search.json       (global)
 */

import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export interface HistorySearchConfig {
	/** Master switch. When false, the tools return a disabled notice. */
	enabled: boolean;
	/**
	 * Override for pi's sessions base directory. When null, resolves from
	 * `$PI_SESSIONS_DIR` then `~/.pi/agent/sessions`. Supports a leading `~`.
	 */
	sessionsDir: string | null;
	/** Run an incremental index of the current project on session_start. */
	indexOnStart: boolean;
	/** Index tool-result messages too (larger index, more recall). */
	includeToolResults: boolean;
	/** Default number of sessions returned by HistorySearch. */
	maxResults: number;
	/** Snippets returned per matching session. */
	snippetsPerSession: number;
}

const CONFIG_FILENAME = "history-search.json";

export const DEFAULT_CONFIG: HistorySearchConfig = {
	enabled: true,
	sessionsDir: null,
	indexOnStart: true,
	includeToolResults: true,
	maxResults: 10,
	snippetsPerSession: 3,
};

export function loadConfig(cwd: string): HistorySearchConfig {
	const paths = [join(cwd, CONFIG_FILENAME), join(cwd, ".pi", CONFIG_FILENAME), join(homedir(), ".pi", "agent", CONFIG_FILENAME)];

	for (const configPath of paths) {
		if (!existsSync(configPath)) continue;
		try {
			const userConfig = JSON.parse(readFileSync(configPath, "utf-8")) as Partial<HistorySearchConfig>;
			return { ...DEFAULT_CONFIG, ...userConfig };
		} catch {
			// Invalid JSON — fall through to the next candidate.
		}
	}

	return DEFAULT_CONFIG;
}

/** Expand a leading `~` to the user's home directory. */
export function expandHome(p: string): string {
	if (p === "~") return homedir();
	if (p.startsWith("~/")) return join(homedir(), p.slice(2));
	return p;
}

/**
 * Resolve pi's sessions base directory.
 * Precedence: explicit config → $PI_SESSIONS_DIR → ~/.pi/agent/sessions.
 */
export function resolveSessionsBase(config: HistorySearchConfig): string {
	if (config.sessionsDir) return expandHome(config.sessionsDir);
	const env = process.env.PI_SESSIONS_DIR;
	if (env) return expandHome(env);
	return join(homedir(), ".pi", "agent", "sessions");
}
