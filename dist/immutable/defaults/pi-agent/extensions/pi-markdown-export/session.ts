/**
 * Session discovery and JSONL → Markdown rendering.
 *
 * pi stores sessions at `<base>/--{safe_cwd}--/{ts}_{id}.jsonl`, where `safe_cwd`
 * strips the leading `/` and replaces every `/` with `-`. That encoding is lossy
 * (a literal `-` in a path is indistinguishable from a path separator), so we use
 * the encoded directory name only as a *fast filter* and confirm each session's
 * real `cwd` from the `session` record on the file's first line.
 */

import { readFileSync, readdirSync } from "node:fs";
import { basename, join } from "node:path";

export type MessagePart = {
	type?: string;
	text?: string;
	thinking?: string;
	/** toolCall parts. */
	name?: string;
	arguments?: Record<string, unknown>;
};
export type BranchEntry = { type?: string; message?: { role?: string; content?: string | MessagePart[] } };

export interface RenderOptions {
	/** Include assistant thinking blocks. */
	includeThinking: boolean;
	/** Render assistant tool calls as compact one-liners. */
	includeToolCalls: boolean;
	/** Include tool result bodies. */
	includeToolResults: boolean;
	/** Cap each rendered message at this many characters (0 = unlimited). */
	maxCharsPerMessage: number;
}

export interface SessionMeta {
	/** Absolute path to the session JSONL file. */
	path: string;
	/** Session id (filename suffix). */
	sessionId: string;
	/** The cwd recorded in the session's `session` entry (real, not encoded). */
	cwd: string;
	/** ISO timestamp parsed from the filename, or "". */
	timestamp: string;
	/** First user message, used as a title. */
	title: string | null;
	/** Count of `message` entries. */
	messageCount: number;
}

// ── Path helpers ─────────────────────────────────────────────────────

/** Mirror pi's encoding: strip leading `/`, replace `/` with `-`, wrap in `--`. */
export function safeDirFromCwd(cwd: string): string {
	const safe = cwd.replace(/^\//, "").replace(/\//g, "-");
	return `--${safe}--`;
}

/** Inner (unwrapped) encoded form, for prefix comparison. */
function safeInner(cwd: string): string {
	return cwd.replace(/^\//, "").replace(/\//g, "-");
}

/** Session id is the filename suffix: `{ts}_{id}.jsonl`. */
function sessionIdFromFilename(filename: string): string {
	const m = filename.replace(/\.jsonl$/, "").match(/_([^_]+)$/);
	return m ? m[1] : filename.replace(/\.jsonl$/, "");
}

/** Reconstruct an ISO timestamp from `2026-02-18T16-02-59-202Z_uuid.jsonl`. */
export function timestampFromFilename(filename: string): string {
	if (!/^\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}/.test(filename)) return "";
	return filename
		.replace(/\.jsonl$/, "")
		.replace(/_[^_]+$/, "")
		.replace(/T(\d{2})-(\d{2})-(\d{2})-(\d+)Z/, "T$1:$2:$3.$4Z");
}

function listDirs(base: string): string[] {
	try {
		return readdirSync(base, { withFileTypes: true })
			.filter((e) => e.isDirectory())
			.map((e) => e.name);
	} catch {
		return [];
	}
}

function listSessionFiles(dir: string): string[] {
	try {
		return readdirSync(dir).filter((f) => f.endsWith(".jsonl"));
	} catch {
		return [];
	}
}

// ── Parsing ──────────────────────────────────────────────────────────

/** Read the JSONL lines of a session file (skipping blanks/invalid). */
function parseEntries(filePath: string): Record<string, unknown>[] {
	let data: string;
	try {
		data = readFileSync(filePath, "utf-8");
	} catch {
		return [];
	}
	const out: Record<string, unknown>[] = [];
	for (const line of data.split("\n")) {
		if (!line.trim()) continue;
		try {
			out.push(JSON.parse(line) as Record<string, unknown>);
		} catch {
			// skip malformed line
		}
	}
	return out;
}

function extractMeta(filePath: string): SessionMeta | null {
	const entries = parseEntries(filePath);
	if (entries.length === 0) return null;
	const filename = basename(filePath);

	let cwd = "";
	let title: string | null = null;
	let messageCount = 0;

	for (const entry of entries) {
		if (entry.type === "session" && typeof entry.cwd === "string") {
			cwd = entry.cwd;
			continue;
		}
		if (entry.type !== "message") continue;
		messageCount++;
		const message = entry.message as { role?: string; content?: string | MessagePart[] } | undefined;
		if (!message) continue;
		if (!title && message.role === "user") {
			const text = extractText(message.content, false);
			if (text) title = text.replace(/\s+/g, " ").slice(0, 200);
		}
	}

	return {
		path: filePath,
		sessionId: sessionIdFromFilename(filename),
		cwd,
		timestamp: timestampFromFilename(filename),
		title,
		messageCount,
	};
}

// ── Discovery ────────────────────────────────────────────────────────

/**
 * Discover sessions whose recorded cwd is `cwd` (and, when `includeSubdirs`,
 * any subdirectory of it). Encoded directory names narrow the search; each
 * session's real cwd is confirmed from its `session` record.
 */
export function discoverSessions(base: string, cwd: string, includeSubdirs: boolean): SessionMeta[] {
	const wantInner = safeInner(cwd);
	const candidateDirs = listDirs(base).filter((name) => {
		const inner = name.replace(/^--/, "").replace(/--$/, "");
		if (inner === wantInner) return true;
		// Fast pre-filter for subdirs; confirmed per-file below.
		return includeSubdirs && inner.startsWith(`${wantInner}-`);
	});

	const results: SessionMeta[] = [];
	for (const dirName of candidateDirs) {
		const dir = join(base, dirName);
		for (const filename of listSessionFiles(dir)) {
			const meta = extractMeta(join(dir, filename));
			if (!meta) continue;
			if (!cwdMatches(meta.cwd, cwd, includeSubdirs)) continue;
			results.push(meta);
		}
	}

	results.sort((a, b) => b.timestamp.localeCompare(a.timestamp));
	return results;
}

function cwdMatches(sessionCwd: string, cwd: string, includeSubdirs: boolean): boolean {
	if (!sessionCwd) return false;
	if (sessionCwd === cwd) return true;
	return includeSubdirs && sessionCwd.startsWith(`${cwd}/`);
}

// ── Rendering ────────────────────────────────────────────────────────

export function extractText(content: string | MessagePart[] | undefined, includeThinking: boolean): string {
	if (typeof content === "string") return content.trim();
	if (!Array.isArray(content)) return "";
	const parts: string[] = [];
	for (const p of content) {
		if (p.type === "text" && typeof p.text === "string") {
			const t = p.text.trim();
			if (t) parts.push(t);
		} else if (includeThinking && p.type === "thinking" && typeof p.thinking === "string") {
			const t = p.thinking.trim();
			if (t) parts.push(`> ${t.replace(/\n/g, "\n> ")}`);
		}
	}
	return parts.join("\n");
}

function roleLabel(role: string | undefined): string {
	if (role === "assistant") return "Assistant";
	if (role === "user") return "User";
	if (role === "toolResult") return "Tool";
	return "System";
}

/** Keys that best identify what a tool call is doing, in priority order. */
const TOOL_ARG_KEYS = ["path", "file_path", "command", "pattern", "query", "url", "name"];

function summarizeArgs(args: Record<string, unknown> | undefined): string {
	if (!args || typeof args !== "object") return "";
	for (const key of TOOL_ARG_KEYS) {
		const v = args[key];
		if (typeof v === "string" && v.trim()) return clip(v.replace(/\s+/g, " "), 80).text;
	}
	for (const [key, v] of Object.entries(args)) {
		if (typeof v === "string" && v.trim()) return clip(v.replace(/\s+/g, " "), 80).text;
		if (typeof v === "number" || typeof v === "boolean") return String(v);
		if (Array.isArray(v)) return `${key}: ${v.length} item(s)`;
	}
	return "…";
}

/** Compact one-line summaries of the toolCall parts in a message (plain ASCII, no emoji). */
function toolCallLines(content: string | MessagePart[] | undefined): string[] {
	if (!Array.isArray(content)) return [];
	const out: string[] = [];
	for (const p of content) {
		if (p.type !== "toolCall" || !p.name) continue;
		const summary = summarizeArgs(p.arguments);
		out.push(`- \`${p.name}(${summary})\``);
	}
	return out;
}

function clip(text: string, max: number): { text: string; truncated: boolean } {
	if (max <= 0 || text.length <= max) return { text, truncated: false };
	return { text: text.slice(0, max), truncated: true };
}

function truncatedNote(text: string, max: number): string {
	const { text: kept, truncated } = clip(text, max);
	return truncated ? `${kept}\n\n_... (truncated, ${text.length} chars)_` : kept;
}

function renderToolResult(content: string | MessagePart[] | undefined, opts: RenderOptions): string[] {
	if (!opts.includeToolResults) return [];
	const text = extractText(content, false);
	if (!text) return [];
	return ["## Tool", "", truncatedNote(text, opts.maxCharsPerMessage), ""];
}

function renderMessage(message: { role?: string; content?: string | MessagePart[] }, opts: RenderOptions): string[] {
	if (message.role === "toolResult") return renderToolResult(message.content, opts);

	const text = extractText(message.content, opts.includeThinking);
	const calls = message.role === "assistant" && opts.includeToolCalls ? toolCallLines(message.content) : [];
	if (!text && calls.length === 0) return [];

	const body = [truncatedNote(text, opts.maxCharsPerMessage), calls.length ? calls.join("\n") : ""].filter(Boolean).join("\n\n");
	return [`## ${roleLabel(message.role)}`, "", body, ""];
}

/** Shared renderer for both live-branch and file-based exports. */
export function entriesToMarkdown(entries: BranchEntry[], title: string, opts: RenderOptions): string {
	const heading = title.trim() || "Pi Session Export";
	const lines: string[] = [`# ${heading}`, "", `Exported: ${new Date().toISOString()}`, ""];

	for (const entry of entries) {
		if (entry.type !== "message" || !entry.message) continue;
		lines.push(...renderMessage(entry.message, opts));
	}

	return `${lines.join("\n")}\n`;
}

/** Render a session file on disk to Markdown. */
export function sessionToMarkdown(filePath: string, fallbackTitle: string | null, opts: RenderOptions): string {
	const entries = parseEntries(filePath) as BranchEntry[];
	const meta = extractMeta(filePath);
	const title = fallbackTitle?.trim() || meta?.title || `Pi Session ${meta?.sessionId ?? basename(filePath)}`;
	return entriesToMarkdown(entries, title, opts);
}

/** A filesystem-safe slug for building export filenames. */
export function exportFileName(meta: SessionMeta, withProject: boolean): string {
	const ts = (meta.timestamp || "unknown").replace(/[:.]/g, "-");
	const id = meta.sessionId.slice(0, 8);
	const project = withProject ? `${safeInner(meta.cwd).slice(0, 40)}__` : "";
	return `${project}${ts}__${id}.md`;
}
