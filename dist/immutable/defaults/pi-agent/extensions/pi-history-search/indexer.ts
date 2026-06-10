/**
 * History indexer — a SQLite FTS5 index, colocated per project.
 *
 * pi stores sessions at `<base>/--{safe_cwd}--/{ts}_{id}.jsonl`. We place one
 * index database *inside each project's session directory*:
 *
 *   <base>/--{safe_cwd}--/.pi-history/index.db
 *
 * This is the key design decision: the index is reachable exactly when the
 * sessions it indexes are reachable. Under an oqto sandbox that restricts an
 * agent to its own sessions, the matching index is restricted with it — there
 * is no shared global database that could leak other projects' history. With no
 * sandbox at all, the same layout simply gives one index per project.
 *
 * The current project is indexed read-write and incrementally (by mtime). Other
 * projects (reached only via `scope: "all"`) are queried read-only if an index
 * exists, otherwise scanned live — we never write into another project's dir.
 */

import * as fs from "node:fs";
import * as fsp from "node:fs/promises";
import { createRequire } from "node:module";
import * as path from "node:path";
import type { HistorySearchConfig } from "./config.js";

const require = createRequire(import.meta.url);

// ── Runtime-agnostic SQLite ──────────────────────────────────────────
//
// pi runs under Node, where `node:sqlite` (Node ≥ 22.5) provides FTS5. Bun
// ships an equivalent `bun:sqlite`. Both expose the same minimal surface we
// need, so we load whichever the host provides and fall back to a live JSONL
// scan when neither exists (older Node). This keeps the extension lean (no
// native dependency to build or version-match) and runtime-agnostic.

interface SqlStatement {
	all(...params: unknown[]): unknown[];
	get(...params: unknown[]): unknown;
	run(...params: unknown[]): unknown;
}
interface SqlDatabase {
	prepare(sql: string): SqlStatement;
	exec(sql: string): void;
	close(): void;
}
type SqlOpener = (dbPath: string, readonly: boolean) => SqlDatabase;

let _opener: SqlOpener | null | undefined;

function loadOpener(): SqlOpener | null {
	const isBun = typeof (globalThis as { Bun?: unknown }).Bun !== "undefined";
	const candidates = isBun ? ["bun:sqlite", "node:sqlite"] : ["node:sqlite", "bun:sqlite"];
	for (const mod of candidates) {
		try {
			const m = require(mod) as Record<string, unknown>;
			if (mod === "node:sqlite" && typeof m.DatabaseSync === "function") {
				const Ctor = m.DatabaseSync as new (p: string, o?: { readOnly?: boolean }) => SqlDatabase;
				return (p, ro) => new Ctor(p, ro ? { readOnly: true } : undefined);
			}
			if (mod === "bun:sqlite" && typeof m.Database === "function") {
				const Ctor = m.Database as new (p: string, o?: { readonly?: boolean }) => SqlDatabase;
				return (p, ro) => (ro ? new Ctor(p, { readonly: true }) : new Ctor(p));
			}
		} catch {
			// Module not available in this runtime — try the next candidate.
		}
	}
	return null;
}

function getOpener(): SqlOpener | null {
	if (_opener !== undefined) return _opener;
	_opener = loadOpener();
	return _opener;
}

/** Whether a SQLite FTS index is available; when false, search uses a live scan. */
export function sqliteAvailable(): boolean {
	return getOpener() !== null;
}

// ── Public shapes ────────────────────────────────────────────────────

export interface ExtractedMessage {
	role: "user" | "assistant" | "toolResult" | string;
	text: string;
}

export interface HistoryMatch {
	role: string;
	/** Ordinal of the message within the session (stable, config-independent). */
	msgIndex: number;
	snippet: string;
}

export interface HistoryHit {
	sessionId: string;
	project: string;
	timestamp: string;
	title: string | null;
	matches: HistoryMatch[];
}

export interface IndexStats {
	indexPath: string;
	totalSessions: number;
	totalChunks: number;
	lastUpdated: string | null;
}

const CHUNK_SIZE = 4000; // characters per FTS row
const INDEX_SUBDIR = ".pi-history";
const SNIPPET_OPEN = "«";
const SNIPPET_CLOSE = "»";

// ── Path helpers ─────────────────────────────────────────────────────

/** Mirror pi's encoding: strip leading `/`, replace `/` with `-`, wrap in `--`. */
export function safeDirFromCwd(cwd: string): string {
	const safe = cwd.replace(/^\//, "").replace(/\//g, "-");
	return `--${safe}--`;
}

/** Human-ish project label from an encoded session directory name. */
export function prettyProject(dirName: string): string {
	return dirName.replace(/^--/, "").replace(/--$/, "") || "unknown";
}

export function projectDir(base: string, cwd: string): string {
	return path.join(base, safeDirFromCwd(cwd));
}

function indexDbPath(projDir: string): string {
	return path.join(projDir, INDEX_SUBDIR, "index.db");
}

/** Session id is the filename suffix: `{ts}_{id}.jsonl`. */
function sessionIdFromFilename(filename: string): string {
	const m = filename.replace(/\.jsonl$/, "").match(/_([^_]+)$/);
	return m ? m[1] : filename.replace(/\.jsonl$/, "");
}

/** Reconstruct an ISO timestamp from `2026-02-18T16-02-59-202Z_uuid.jsonl`. */
function timestampFromFilename(filename: string): string {
	if (!/^\d{4}-\d{2}-\d{2}T\d{2}-\d{2}-\d{2}/.test(filename)) return "";
	return filename
		.replace(/\.jsonl$/, "")
		.replace(/_[^_]+$/, "")
		.replace(/T(\d{2})-(\d{2})-(\d{2})-(\d+)Z/, "T$1:$2:$3.$4Z");
}

function listSessionFiles(projDir: string): string[] {
	let entries: string[];
	try {
		entries = fs.readdirSync(projDir);
	} catch {
		return [];
	}
	return entries.filter((f) => f.endsWith(".jsonl"));
}

// ── Extraction ───────────────────────────────────────────────────────

function blocksToText(content: unknown, includeTypes: Set<string>): string {
	if (typeof content === "string") return content;
	if (!Array.isArray(content)) return "";
	const parts: string[] = [];
	for (const block of content as Record<string, unknown>[]) {
		if (block && includeTypes.has(block.type as string) && typeof block.text === "string") {
			parts.push(block.text);
		}
	}
	return parts.join(" ");
}

const TEXT_ONLY = new Set(["text"]);

/**
 * Extract one entry per `message` record, in file order. The ordinal of each
 * entry is the stable `msgIndex` used by HistorySearch and HistoryRead. Assistant
 * thinking blocks and tool calls are dropped from the indexed text but the entry
 * is still emitted so ordinals never shift.
 */
export function extractMessages(data: string): { messages: ExtractedMessage[]; firstUserMessage: string | null } {
	const messages: ExtractedMessage[] = [];
	let firstUserMessage: string | null = null;

	for (const line of data.split("\n")) {
		if (!line.trim()) continue;
		let entry: { type?: string; message?: { role?: string; content?: unknown } };
		try {
			entry = JSON.parse(line);
		} catch {
			continue;
		}
		if (entry.type !== "message" || !entry.message) continue;

		const role = entry.message.role ?? "unknown";
		const text = blocksToText(entry.message.content, TEXT_ONLY).replace(/\s+/g, " ").trim();
		messages.push({ role, text });
		if (role === "user" && !firstUserMessage && text) {
			firstUserMessage = text.slice(0, 200);
		}
	}

	return { messages, firstUserMessage };
}

// ── Query sanitization (ported from the reference) ───────────────────

export function sanitizeTokens(query: string): string[] {
	return query
		.replace(/[^\p{L}\p{N}\s]/gu, " ")
		.trim()
		.split(/\s+/)
		.filter(Boolean);
}

function buildFtsQuery(tokens: string[]): string {
	if (tokens.length === 0) return "";
	return tokens.map((t, i) => (i === tokens.length - 1 ? `"${t}"*` : `"${t}"`)).join(" ");
}

// ── Role filtering ───────────────────────────────────────────────────
//
// Session content is noisy with tool output (file dumps, docs). Filtering by
// role lets callers focus on the conversation (user + assistant) — far higher
// signal for "what did we do" recall — or target tool output for error/path
// recall.

export type RoleFilter = "all" | "conversation" | "user" | "assistant" | "tool";

/** SQL roles for a filter, or null for "all" (no role clause). */
function rolesFor(filter: RoleFilter): string[] | null {
	switch (filter) {
		case "conversation":
			return ["user", "assistant"];
		case "user":
			return ["user"];
		case "assistant":
			return ["assistant"];
		case "tool":
			return ["toolResult"];
		default:
			return null;
	}
}

function roleAllowed(role: string, filter: RoleFilter): boolean {
	const roles = rolesFor(filter);
	return roles === null || roles.includes(role);
}

/** `AND role IN ('user','assistant')`-style clause for FTS queries (empty for "all"). */
function roleSqlClause(filter: RoleFilter): string {
	const roles = rolesFor(filter);
	if (!roles) return "";
	return ` AND role IN (${roles.map((r) => `'${r}'`).join(", ")})`;
}

// ── Database lifecycle ───────────────────────────────────────────────

const openDbs = new Map<string, SqlDatabase>();

/** Run a function inside a transaction; rolls back on throw. */
function tx(db: SqlDatabase, fn: () => void): void {
	db.exec("BEGIN");
	try {
		fn();
		db.exec("COMMIT");
	} catch (e) {
		try {
			db.exec("ROLLBACK");
		} catch {
			// ignore rollback failure
		}
		throw e;
	}
}

function initSchema(db: SqlDatabase): void {
	db.exec("PRAGMA journal_mode = WAL");
	db.exec("PRAGMA synchronous = NORMAL");
	db.exec("PRAGMA busy_timeout = 5000");
	db.exec(`
		CREATE TABLE IF NOT EXISTS sessions (
			path TEXT PRIMARY KEY,
			session_id TEXT,
			session_ts TEXT,
			mtime_ms INTEGER NOT NULL,
			first_user_message TEXT
		);
		CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
			content,
			session_path UNINDEXED,
			role UNINDEXED,
			msg_index UNINDEXED,
			tokenize='porter unicode61'
		);
		CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT);
	`);
}

/** Open (and cache) the index for a project dir. Returns null when unavailable. */
function openDb(projDir: string, mode: "rw" | "ro"): SqlDatabase | null {
	const dbPath = indexDbPath(projDir);
	const cached = openDbs.get(dbPath);
	if (cached) return cached;

	const opener = getOpener();
	if (!opener) return null;

	try {
		if (mode === "rw") {
			fs.mkdirSync(path.dirname(dbPath), { recursive: true });
			const db = opener(dbPath, false);
			initSchema(db);
			openDbs.set(dbPath, db);
			return db;
		}
		if (!fs.existsSync(dbPath)) return null;
		const db = opener(dbPath, true);
		openDbs.set(dbPath, db);
		return db;
	} catch {
		return null;
	}
}

export function closeAll(): void {
	for (const db of openDbs.values()) {
		try {
			db.close();
		} catch {
			// ignore
		}
	}
	openDbs.clear();
}

// ── Incremental indexing (current project, read-write) ───────────────

function yieldTick(): Promise<void> {
	return new Promise((resolve) => setImmediate(resolve));
}

interface PreparedStatements {
	upsert: SqlStatement;
	deleteFts: SqlStatement;
	insertFts: SqlStatement;
}

function indexOneFile(
	stmts: PreparedStatements,
	filePath: string,
	mtime: number,
	config: HistorySearchConfig,
	data: string
): void {
	const filename = path.basename(filePath);
	const { messages, firstUserMessage } = extractMessages(data);

	stmts.deleteFts.run(filePath);
	stmts.upsert.run(filePath, sessionIdFromFilename(filename), timestampFromFilename(filename), mtime, firstUserMessage);

	for (let i = 0; i < messages.length; i++) {
		const m = messages[i];
		if (!m.text) continue;
		if (m.role === "toolResult" && !config.includeToolResults) continue;
		for (let off = 0; off < m.text.length; off += CHUNK_SIZE) {
			stmts.insertFts.run(m.text.slice(off, off + CHUNK_SIZE), filePath, m.role, i);
		}
	}
}

/**
 * Bring the current project's index up to date. Returns the number of session
 * files re-indexed this run. Throws only if the index cannot be opened RW.
 */
export async function updateProjectIndex(projDir: string, config: HistorySearchConfig): Promise<number> {
	const db = openDb(projDir, "rw");
	if (!db) throw new Error(`Cannot open index (read-only?) for ${projDir}`);

	const files = listSessionFiles(projDir);
	const indexed = new Map<string, number>();
	for (const row of db.prepare("SELECT path, mtime_ms FROM sessions").all() as unknown as { path: string; mtime_ms: number }[]) {
		indexed.set(row.path, row.mtime_ms);
	}

	const present = new Set<string>();
	const toIndex: { filePath: string; mtime: number }[] = [];
	for (const filename of files) {
		const filePath = path.join(projDir, filename);
		present.add(filePath);
		let mtime: number;
		try {
			// Floor to integer ms: the mtime_ms column has INTEGER affinity, so a
			// fractional value would be coerced on store and re-trigger indexing.
			mtime = Math.floor((await fsp.stat(filePath)).mtimeMs);
		} catch {
			continue;
		}
		const prev = indexed.get(filePath);
		if (prev === undefined || mtime > prev) toIndex.push({ filePath, mtime });
	}

	// Drop sessions that disappeared.
	const removed = [...indexed.keys()].filter((p) => !present.has(p));
	if (removed.length > 0) {
		const delSession = db.prepare("DELETE FROM sessions WHERE path = ?");
		const delFts = db.prepare("DELETE FROM messages_fts WHERE session_path = ?");
		tx(db, () => {
			for (const p of removed) {
				delSession.run(p);
				delFts.run(p);
			}
		});
	}

	if (toIndex.length === 0) return 0;

	const stmts: PreparedStatements = {
		upsert: db.prepare(
			"INSERT OR REPLACE INTO sessions (path, session_id, session_ts, mtime_ms, first_user_message) VALUES (?, ?, ?, ?, ?)"
		),
		deleteFts: db.prepare("DELETE FROM messages_fts WHERE session_path = ?"),
		insertFts: db.prepare("INSERT INTO messages_fts (content, session_path, role, msg_index) VALUES (?, ?, ?, ?)"),
	};

	const BATCH = 20;
	for (let start = 0; start < toIndex.length; start += BATCH) {
		const batch = toIndex.slice(start, start + BATCH);
		const loaded = await Promise.all(
			batch.map(async (item) => {
				try {
					return { ...item, data: await fsp.readFile(item.filePath, "utf-8") };
				} catch {
					return null;
				}
			})
		);
		tx(db, () => {
			for (const item of loaded) {
				if (item) indexOneFile(stmts, item.filePath, item.mtime, config, item.data);
			}
		});
		if (start + BATCH < toIndex.length) await yieldTick();
	}

	db.prepare("INSERT OR REPLACE INTO meta (key, value) VALUES ('last_updated', ?)").run(new Date().toISOString());
	return toIndex.length;
}

export async function rebuildProjectIndex(projDir: string, config: HistorySearchConfig): Promise<number> {
	const db = openDb(projDir, "rw");
	if (!db) throw new Error(`Cannot open index (read-only?) for ${projDir}`);
	db.exec("DELETE FROM messages_fts; DELETE FROM sessions; DELETE FROM meta;");
	return updateProjectIndex(projDir, config);
}

// ── Searching one indexed project ────────────────────────────────────

function snippetsForSession(
	db: SqlDatabase,
	ftsQuery: string,
	sessionPath: string,
	limit: number,
	roleFilter: RoleFilter
): HistoryMatch[] {
	const rows = db
		.prepare(
			`SELECT role, msg_index AS msgIndex, snippet(messages_fts, 0, '${SNIPPET_OPEN}', '${SNIPPET_CLOSE}', ' … ', 18) AS snippet
			 FROM messages_fts
			 WHERE messages_fts MATCH ? AND session_path = ?${roleSqlClause(roleFilter)}
			 ORDER BY rank
			 LIMIT ?`
		)
		.all(ftsQuery, sessionPath, limit) as unknown as HistoryMatch[];
	return rows.map((r) => ({ role: r.role, msgIndex: Number(r.msgIndex), snippet: r.snippet }));
}

function searchDb(
	db: SqlDatabase,
	project: string,
	query: string,
	limit: number,
	snippetsPerSession: number,
	roleFilter: RoleFilter
): HistoryHit[] {
	const tokens = sanitizeTokens(query);
	const ftsQuery = buildFtsQuery(tokens);
	if (!ftsQuery) return [];

	const best = db
		.prepare(
			`SELECT session_path AS path, MIN(rank) AS br
			 FROM messages_fts WHERE messages_fts MATCH ?${roleSqlClause(roleFilter)}
			 GROUP BY session_path ORDER BY br LIMIT ?`
		)
		.all(ftsQuery, limit) as unknown as { path: string; br: number }[];

	const meta = db.prepare("SELECT session_id, session_ts, first_user_message FROM sessions WHERE path = ?");
	const hits: HistoryHit[] = [];
	for (const row of best) {
		const m = meta.get(row.path) as unknown as
			| { session_id: string; session_ts: string; first_user_message: string | null }
			| undefined;
		hits.push({
			sessionId: m?.session_id ?? path.basename(row.path),
			project,
			timestamp: m?.session_ts ?? "",
			title: m?.first_user_message ?? null,
			matches: snippetsForSession(db, ftsQuery, row.path, snippetsPerSession, roleFilter),
		});
	}
	return hits;
}

// ── Live scan fallback (read-only cross-project dirs) ─────────────────

function makeSnippet(text: string, tokens: string[]): string {
	const lower = text.toLowerCase();
	let at = -1;
	for (const t of tokens) {
		const idx = lower.indexOf(t.toLowerCase());
		if (idx >= 0 && (at < 0 || idx < at)) at = idx;
	}
	if (at < 0) at = 0;
	const from = Math.max(0, at - 60);
	const slice = text.slice(from, from + 200);
	return (from > 0 ? "… " : "") + slice + (from + 200 < text.length ? " …" : "");
}

function scanFile(
	filePath: string,
	tokens: string[],
	snippetsPerSession: number,
	roleFilter: RoleFilter
): { score: number; matches: HistoryMatch[]; firstUserMessage: string | null } {
	let data: string;
	try {
		data = fs.readFileSync(filePath, "utf-8");
	} catch {
		return { score: 0, matches: [], firstUserMessage: null };
	}
	const { messages, firstUserMessage } = extractMessages(data);
	const matches: HistoryMatch[] = [];
	let score = 0;
	for (let i = 0; i < messages.length; i++) {
		if (!roleAllowed(messages[i].role, roleFilter)) continue;
		const lower = messages[i].text.toLowerCase();
		if (!lower) continue;
		let hits = 0;
		for (const t of tokens) if (lower.includes(t.toLowerCase())) hits++;
		if (hits === 0) continue;
		score += hits;
		if (matches.length < snippetsPerSession) {
			matches.push({ role: messages[i].role, msgIndex: i, snippet: makeSnippet(messages[i].text, tokens) });
		}
	}
	return { score, matches, firstUserMessage };
}

function scanProject(
	projDir: string,
	project: string,
	query: string,
	limit: number,
	snippetsPerSession: number,
	roleFilter: RoleFilter
): HistoryHit[] {
	const tokens = sanitizeTokens(query);
	if (tokens.length === 0) return [];
	const scored: { hit: HistoryHit; score: number }[] = [];
	for (const filename of listSessionFiles(projDir)) {
		const filePath = path.join(projDir, filename);
		const { score, matches, firstUserMessage } = scanFile(filePath, tokens, snippetsPerSession, roleFilter);
		if (score === 0) continue;
		scored.push({
			score,
			hit: {
				sessionId: sessionIdFromFilename(filename),
				project,
				timestamp: timestampFromFilename(filename),
				title: firstUserMessage,
				matches,
			},
		});
	}
	scored.sort((a, b) => b.score - a.score);
	return scored.slice(0, limit).map((s) => s.hit);
}

/**
 * Search a single project dir. The current project gets fresh incremental
 * indexing; other dirs use an existing index if present, else a live scan.
 */
export async function searchProject(
	projDir: string,
	query: string,
	config: HistorySearchConfig,
	limit: number,
	isCurrent: boolean,
	roleFilter: RoleFilter = "all"
): Promise<HistoryHit[]> {
	const project = prettyProject(path.basename(projDir));
	if (isCurrent) {
		try {
			await updateProjectIndex(projDir, config);
			const db = openDb(projDir, "rw");
			if (db) return searchDb(db, project, query, limit, config.snippetsPerSession, roleFilter);
		} catch {
			// fall through to read-only / scan
		}
	}
	const ro = openDb(projDir, "ro");
	if (ro) return searchDb(ro, project, query, limit, config.snippetsPerSession, roleFilter);
	return scanProject(projDir, project, query, limit, config.snippetsPerSession, roleFilter);
}

/**
 * Query a project's existing index without re-indexing — for the interactive
 * overlay, which refreshes the index once on open then searches per keystroke.
 * Falls back to a live scan when no index is available.
 */
export function queryProject(
	projDir: string,
	query: string,
	config: HistorySearchConfig,
	limit: number,
	roleFilter: RoleFilter = "all"
): HistoryHit[] {
	const project = prettyProject(path.basename(projDir));
	const db = openDb(projDir, fs.existsSync(indexDbPath(projDir)) ? "ro" : "rw");
	if (db) return searchDb(db, project, query, limit, config.snippetsPerSession, roleFilter);
	return scanProject(projDir, project, query, limit, config.snippetsPerSession, roleFilter);
}

/** Most recent sessions in a project (for the overlay's empty-query view). */
export function listRecent(projDir: string, limit: number): HistoryHit[] {
	const project = prettyProject(path.basename(projDir));
	const db = openDb(projDir, fs.existsSync(indexDbPath(projDir)) ? "ro" : "rw");
	if (db) {
		const rows = db
			.prepare("SELECT session_id, session_ts, first_user_message FROM sessions ORDER BY session_ts DESC LIMIT ?")
			.all(limit) as unknown as { session_id: string; session_ts: string; first_user_message: string | null }[];
		return rows.map((r) => ({
			sessionId: r.session_id,
			project,
			timestamp: r.session_ts,
			title: r.first_user_message,
			matches: [],
		}));
	}
	// No index: derive from filenames, newest first.
	const files = listSessionFiles(projDir).sort().reverse().slice(0, limit);
	return files.map((filename) => ({
		sessionId: sessionIdFromFilename(filename),
		project,
		timestamp: timestampFromFilename(filename),
		title: null,
		matches: [],
	}));
}

/** All project dirs under the sessions base (each `--cwd--` directory). */
export function listProjectDirs(base: string): string[] {
	let entries: fs.Dirent[];
	try {
		entries = fs.readdirSync(base, { withFileTypes: true });
	} catch {
		return [];
	}
	return entries.filter((e) => e.isDirectory()).map((e) => path.join(base, e.name));
}

// ── Reading a specific session ───────────────────────────────────────

/** Resolve a session id to its JSONL path, scanning project dirs under base. */
export function findSessionPath(base: string, sessionId: string, preferDir?: string): string | null {
	const suffix = `_${sessionId}.jsonl`;
	const dirs = preferDir ? [preferDir, ...listProjectDirs(base).filter((d) => d !== preferDir)] : listProjectDirs(base);
	for (const dir of dirs) {
		for (const filename of listSessionFiles(dir)) {
			if (filename.endsWith(suffix) || filename === `${sessionId}.jsonl`) {
				return path.join(dir, filename);
			}
		}
	}
	return null;
}

export interface ReadResult {
	sessionId: string;
	project: string;
	timestamp: string;
	totalMessages: number;
	mode: "around" | "query" | "transcript" | "outline";
	messages: { role: string; msgIndex: number; text: string }[];
	truncated: boolean;
}

export interface ReadOptions {
	query?: string;
	around?: number;
	before?: number;
	after?: number;
	maxChars: number;
	/** Restrict which message roles are returned (query / whole-session modes). */
	roleFilter?: RoleFilter;
	/**
	 * Whole-session rendering when neither `query` nor `around` is given:
	 * "outline" = conversation only (user + assistant), tool noise dropped;
	 * "transcript" = every non-empty message. Defaults to "transcript".
	 */
	view?: "outline" | "transcript";
}

function clip(text: string, max: number): { text: string; truncated: boolean } {
	if (text.length <= max) return { text, truncated: false };
	return { text: `${text.slice(0, max)} …`, truncated: true };
}

export function readSession(filePath: string, opts: ReadOptions): ReadResult {
	const filename = path.basename(filePath);
	const data = fs.readFileSync(filePath, "utf-8");
	const { messages } = extractMessages(data);
	// Session files live directly in the `--cwd--` project dir.
	const project = prettyProject(path.basename(path.dirname(filePath)));
	const base = {
		sessionId: sessionIdFromFilename(filename),
		project,
		timestamp: timestampFromFilename(filename),
		totalMessages: messages.length,
	};

	// Context window around a hit — contiguous, so role filtering is not applied.
	if (typeof opts.around === "number") {
		const before = opts.before ?? 3;
		const after = opts.after ?? 3;
		const from = Math.max(0, opts.around - before);
		const to = Math.min(messages.length, opts.around + after + 1);
		const out = messages.slice(from, to).map((m, k) => {
			const c = clip(m.text, opts.maxChars);
			return { role: m.role, msgIndex: from + k, text: c.text, _t: c.truncated };
		});
		return {
			...base,
			mode: "around",
			messages: out.map(({ _t, ...m }) => m),
			truncated: out.some((m) => m._t),
		};
	}

	if (opts.query) {
		const roleFilter = opts.roleFilter ?? "all";
		const tokens = sanitizeTokens(opts.query);
		const out: ReadResult["messages"] = [];
		let truncated = false;
		for (let i = 0; i < messages.length; i++) {
			if (!roleAllowed(messages[i].role, roleFilter)) continue;
			const lower = messages[i].text.toLowerCase();
			if (!lower || !tokens.some((t) => lower.includes(t.toLowerCase()))) continue;
			const c = clip(messages[i].text, opts.maxChars);
			truncated = truncated || c.truncated;
			out.push({ role: messages[i].role, msgIndex: i, text: c.text });
		}
		return { ...base, mode: "query", messages: out, truncated };
	}

	// Whole-session. "outline" drops tool noise to a readable conversation thread;
	// "transcript" keeps every non-empty message.
	const view = opts.view ?? "transcript";
	const roleFilter = opts.roleFilter ?? (view === "outline" ? "conversation" : "all");
	const kept = messages.map((m, i) => ({ ...m, i })).filter((m) => m.text && roleAllowed(m.role, roleFilter));
	const per = Math.max(200, Math.floor(opts.maxChars / Math.max(1, kept.length)));
	let truncated = false;
	const out = kept.map((m) => {
		const c = clip(m.text, per);
		truncated = truncated || c.truncated;
		return { role: m.role, msgIndex: m.i, text: c.text };
	});
	return { ...base, mode: view, messages: out, truncated };
}

// ── Stats ────────────────────────────────────────────────────────────

export function getStats(projDir: string): IndexStats {
	const dbPath = indexDbPath(projDir);
	const db = openDb(projDir, fs.existsSync(dbPath) ? "ro" : "rw");
	if (!db) return { indexPath: dbPath, totalSessions: 0, totalChunks: 0, lastUpdated: null };
	const sessions = (db.prepare("SELECT COUNT(*) AS c FROM sessions").get() as unknown as { c: number }).c;
	const chunks = (db.prepare("SELECT COUNT(*) AS c FROM messages_fts").get() as unknown as { c: number }).c;
	const meta = db.prepare("SELECT value FROM meta WHERE key = 'last_updated'").get() as unknown as { value: string } | undefined;
	return { indexPath: dbPath, totalSessions: sessions, totalChunks: chunks, lastUpdated: meta?.value ?? null };
}
