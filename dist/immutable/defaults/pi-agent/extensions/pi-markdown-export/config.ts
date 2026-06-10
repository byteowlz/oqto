/**
 * Configuration loading for pi-markdown-export.
 *
 * Searched in order (first match wins), matching the repo convention:
 *   1. ./markdown-export.json            (cwd)
 *   2. ./.pi/markdown-export.json        (project-local)
 *   3. ~/.pi/agent/markdown-export.json  (global)
 */

import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/** A find/replace rule applied to export text before it is written. */
export interface ReplacementRule {
	/** Optional label, shown in notifications/logs. */
	name?: string;
	/** Literal string (default) or a regular expression source when `regex` is true. */
	find: string;
	/** Replacement text. Supports `$1`-style group refs when `regex` is true. */
	replace: string;
	/** Treat `find` as a regular expression source. */
	regex?: boolean;
	/** Regex flags (default "g"). Ignored unless `regex` is true. */
	flags?: string;
}

/** How an external redaction command participates in the pipeline. */
export type RedactionMode = "scan" | "filter";

/** What to do when a `scan`-mode command reports findings it cannot mask. */
export type OnFinding = "mask" | "skip" | "warn";

/**
 * An external CLI run over the export before it is written.
 *
 * Placeholders substituted in `args`:
 *   {file}   — path to a temp file holding the current export text
 *   {report} — path to a temp file the tool may write a JSON report to
 *
 * Modes:
 *   "filter" — the command transforms the text. The new content is taken from
 *              the command's stdout, or from {file} when `inPlace` is true.
 *   "scan"   — the command detects secrets (e.g. gitleaks, trufflehog). Detected
 *              secret strings parsed from its JSON output (stdout or {report})
 *              are masked in the text. `onFinding` controls the fallback when no
 *              maskable strings can be parsed but the tool still flagged secrets.
 */
export interface RedactionCommand {
	/** Label, shown in notifications/logs. */
	name?: string;
	/** Executable to run. */
	command: string;
	/** Arguments, with {file}/{report} placeholders substituted. */
	args?: string[];
	/** Pipeline role. Default "scan". */
	mode?: RedactionMode;
	/** filter mode: read the (possibly modified) {file} back instead of stdout. */
	inPlace?: boolean;
	/** filter mode: pipe the current text to the command's stdin. */
	stdin?: boolean;
	/** scan mode: fallback when findings cannot be parsed into maskable strings. Default "warn". */
	onFinding?: OnFinding;
	/** scan mode: token detected secrets are replaced with. Default "[REDACTED]". */
	maskWith?: string;
	/** When true, a missing executable or non-zero exit aborts the export. Default false. */
	required?: boolean;
	/** Per-command timeout in milliseconds. Default 30000. */
	timeoutMs?: number;
}

export interface MarkdownExportConfig {
	/** Directory bulk/picker exports are written to. Relative paths resolve against cwd. */
	exportDir: string;
	/** Include sessions from subdirectories of the cwd in bulk export / picker. */
	includeSubdirs: boolean;
	/** Include assistant thinking blocks in the rendered Markdown. */
	includeThinking: boolean;
	/** Render assistant tool calls as compact one-liners (🔧 `read(path)`). */
	includeToolCalls: boolean;
	/** Include tool result bodies (file dumps, command output). Usually the bulk of the noise. */
	includeToolResults: boolean;
	/** Cap each rendered message at this many characters (0 = unlimited). */
	maxCharsPerMessage: number;
	/**
	 * Override for pi's sessions base directory. When null, resolves from
	 * `$PI_SESSIONS_DIR` then `~/.pi/agent/sessions`. Supports a leading `~`.
	 */
	sessionsDir: string | null;
	/** Find/replace rules applied before any redaction command. */
	replacements: ReplacementRule[];
	/** External redaction CLIs run (in order) before the export is written. */
	redactionCommands: RedactionCommand[];
}

const CONFIG_FILENAME = "markdown-export.json";

export const DEFAULT_CONFIG: MarkdownExportConfig = {
	exportDir: "./pi-session-exports",
	includeSubdirs: false,
	includeThinking: false,
	includeToolCalls: true,
	includeToolResults: false,
	maxCharsPerMessage: 0,
	sessionsDir: null,
	replacements: [],
	redactionCommands: [],
};

export function loadConfig(cwd: string): MarkdownExportConfig {
	const paths = [join(cwd, CONFIG_FILENAME), join(cwd, ".pi", CONFIG_FILENAME), join(homedir(), ".pi", "agent", CONFIG_FILENAME)];

	for (const configPath of paths) {
		if (!existsSync(configPath)) continue;
		try {
			const userConfig = JSON.parse(readFileSync(configPath, "utf-8")) as Partial<MarkdownExportConfig>;
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
export function resolveSessionsBase(config: MarkdownExportConfig): string {
	if (config.sessionsDir) return expandHome(config.sessionsDir);
	const env = process.env.PI_SESSIONS_DIR;
	if (env) return expandHome(env);
	return join(homedir(), ".pi", "agent", "sessions");
}
