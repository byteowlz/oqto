/**
 * pi-history-search — let the agent search its own session history.
 *
 * Registers two LLM-callable tools:
 *   - HistorySearch: full-text search over past sessions (FTS5, BM25-ranked).
 *   - HistoryRead:   pull fuller context from a specific past session.
 *
 * The index is a SQLite FTS5 database colocated inside each project's session
 * directory (`<base>/--{cwd}--/.pi-history/index.db`), so it is reachable
 * exactly when the sessions it indexes are — which keeps an oqto sandbox's
 * "own sessions only" restriction intact, and works identically with no
 * sandbox at all. See indexer.ts for the rationale.
 */

import { StringEnum } from "@earendil-works/pi-ai";
import type { AgentToolResult, ExtensionAPI, ExtensionContext, Theme } from "@earendil-works/pi-coding-agent";
import { Text } from "@earendil-works/pi-tui";
import { Type } from "typebox";
import { type HistorySearchConfig, loadConfig, resolveSessionsBase } from "./config.js";
import {
	type HistoryHit,
	type ReadResult,
	type RoleFilter,
	closeAll,
	findSessionPath,
	getStats,
	listProjectDirs,
	listRecent,
	projectDir,
	readSession,
	rebuildProjectIndex,
	searchProject,
	updateProjectIndex,
} from "./indexer.js";
import { HistoryOverlay } from "./overlay.js";

// ── Tool parameter schemas ───────────────────────────────────────────

const HistorySearchParams = Type.Object({
	query: Type.Optional(
		Type.String({
			description:
				"Search terms, matched full-text (BM25-ranked). Omit or leave empty to list the most recent sessions (use this for 'what did we do here / recently').",
		})
	),
	scope: Type.Optional(
		StringEnum(["project", "all"] as const, {
			description:
				"'project' (default) searches only the current project's history. 'all' also searches other projects, but only those the environment exposes (e.g. a sandbox may restrict this).",
		})
	),
	project: Type.Optional(
		Type.String({ description: "When scope='all', only return sessions whose project label contains this string." })
	),
	roleFilter: Type.Optional(
		StringEnum(["all", "conversation", "user", "assistant", "tool"] as const, {
			description:
				"Which message roles to search. Default 'conversation' (user+assistant) — best signal for 'what did we do'. Use 'all' to also search tool output (error messages, file paths, command output), or 'tool' for only that.",
		})
	),
	limit: Type.Optional(Type.Number({ description: "Max sessions to return (default from config, usually 10)." })),
});

const HistoryReadParams = Type.Object({
	sessionId: Type.String({ description: "Session id from a HistorySearch result." }),
	query: Type.Optional(
		Type.String({ description: "Return only messages in this session matching these terms (with the message text)." })
	),
	around: Type.Optional(
		Type.Number({ description: "Return a window of messages centered on this msgIndex (from a HistorySearch match)." })
	),
	before: Type.Optional(Type.Number({ description: "Messages before `around` (default 3)." })),
	after: Type.Optional(Type.Number({ description: "Messages after `around` (default 3)." })),
	view: Type.Optional(
		StringEnum(["outline", "transcript"] as const, {
			description:
				"Whole-session rendering (ignored with `around`/`query`). Default 'outline' = conversation only (user+assistant), tool noise dropped — compact recall. 'transcript' = every non-empty message.",
		})
	),
	roleFilter: Type.Optional(
		StringEnum(["all", "conversation", "user", "assistant", "tool"] as const, {
			description:
				"Restrict returned roles (query mode, or override the outline/transcript default). E.g. 'tool' to inspect only tool output.",
		})
	),
	maxChars: Type.Optional(
		Type.Number({ description: "Per-message character cap (whole-session: total budget split across messages). Default 2000." })
	),
});

// ── Formatting helpers ───────────────────────────────────────────────

function formatTs(ts: string): string {
	if (!ts) return "unknown time";
	const d = new Date(ts);
	if (Number.isNaN(d.getTime())) return ts.slice(0, 16);
	return d.toISOString().slice(0, 16).replace("T", " ");
}

function shortId(id: string): string {
	return id.length > 12 ? `${id.slice(0, 8)}…` : id;
}

function formatHits(query: string | undefined, scope: string, hits: HistoryHit[]): string {
	const q = query?.trim() ?? "";
	if (hits.length === 0) {
		return q ? `No past sessions matched "${q}" (scope: ${scope}).` : `No sessions found (scope: ${scope}).`;
	}
	const header = q
		? `Found ${hits.length} session(s) for "${q}" (scope: ${scope}):`
		: `${hits.length} most recent session(s) (scope: ${scope}):`;
	const lines: string[] = [header, ""];
	hits.forEach((h, i) => {
		lines.push(`[${i + 1}] ${h.project} · ${formatTs(h.timestamp)} · sessionId=${h.sessionId}`);
		if (h.title) lines.push(`    title: ${h.title.slice(0, 120)}`);
		for (const m of h.matches) {
			lines.push(`    ${m.role} (msg ${m.msgIndex}): ${m.snippet}`);
		}
		lines.push("");
	});
	lines.push(
		"Use HistoryRead{sessionId, around: <msg>} to expand a hit, or HistoryRead{sessionId, query} to pull all matches from a session."
	);
	return lines.join("\n").trimEnd();
}

function formatRead(r: ReadResult): string {
	const head = `Session ${shortId(r.sessionId)} · ${r.project} · ${formatTs(r.timestamp)} · ${r.totalMessages} messages · mode=${r.mode}`;
	if (r.messages.length === 0) {
		return `${head}\n(no matching messages)`;
	}
	const body = r.messages.map((m) => `[msg ${m.msgIndex}] ${m.role}:\n${m.text}`).join("\n\n");
	const tail = r.truncated ? "\n\n(some messages truncated — raise maxChars or narrow with query/around to see more)" : "";
	return `${head}\n\n${body}${tail}`;
}

function disabled(action: string): AgentToolResult<unknown> {
	return {
		content: [{ type: "text", text: "pi-history-search is disabled (set enabled=true in history-search.json)." }],
		details: { action, error: "disabled" },
	};
}

// ── Search execution ─────────────────────────────────────────────────

async function runSearch(
	ctx: ExtensionContext,
	config: HistorySearchConfig,
	params: { query?: string; scope?: string; project?: string; limit?: number; roleFilter?: RoleFilter }
): Promise<HistoryHit[]> {
	const base = resolveSessionsBase(config);
	const limit = params.limit ?? config.maxResults;
	const currentDir = projectDir(base, ctx.cwd);
	const scope = params.scope ?? "project";
	const query = params.query?.trim() ?? "";
	const roleFilter: RoleFilter = params.roleFilter ?? "conversation";

	if (scope !== "all") {
		// Blank query → most recent sessions ("what did we do here / recently").
		return query ? searchProject(currentDir, query, config, limit, true, roleFilter) : listRecent(currentDir, limit);
	}

	const dirs = listProjectDirs(base);
	const all: HistoryHit[] = [];
	for (const dir of dirs) {
		const isCurrent = dir === currentDir;
		const hits = query ? await searchProject(dir, query, config, limit, isCurrent, roleFilter) : listRecent(dir, limit);
		all.push(...hits);
	}
	const projFilter = params.project?.toLowerCase();
	const filtered = projFilter ? all.filter((h) => h.project.toLowerCase().includes(projFilter)) : all;
	// Sessions already come back per-project-ranked; order by recency across projects.
	filtered.sort((a, b) => (a.timestamp < b.timestamp ? 1 : -1));
	return filtered.slice(0, limit);
}

// ── Extension entry point ────────────────────────────────────────────

export default function historySearch(pi: ExtensionAPI): void {
	let indexing = false;

	async function ensureCurrentIndex(ctx: ExtensionContext): Promise<void> {
		if (indexing) return;
		const config = loadConfig(ctx.cwd);
		if (!config.enabled || !config.indexOnStart) return;
		indexing = true;
		try {
			const base = resolveSessionsBase(config);
			await updateProjectIndex(projectDir(base, ctx.cwd), config);
		} catch {
			// Index not writable here (e.g. read-only mount) — tools fall back at query time.
		} finally {
			indexing = false;
		}
	}

	pi.on("session_start", async (_event, ctx) => {
		// Defer slightly so startup isn't blocked by a first-run full index.
		setTimeout(() => {
			void ensureCurrentIndex(ctx);
		}, 100);
	});

	pi.on("session_shutdown", async () => {
		closeAll();
	});

	// ── Interactive overlay (humans) ──────────────────────────────────
	async function openOverlay(ctx: ExtensionContext, initialQuery?: string): Promise<void> {
		if (!ctx.hasUI) {
			ctx.ui.notify("History overlay needs an interactive TUI", "warning");
			return;
		}
		const config = loadConfig(ctx.cwd);
		if (!config.enabled) {
			ctx.ui.notify("pi-history-search is disabled", "warning");
			return;
		}
		const base = resolveSessionsBase(config);
		const dir = projectDir(base, ctx.cwd);
		// Refresh the index once up front so per-keystroke search stays instant.
		ctx.ui.setStatus("history-search", "🔍 indexing…");
		try {
			await updateProjectIndex(dir, config);
		} catch {
			// Read-only: the overlay falls back to an existing index or live scan.
		} finally {
			ctx.ui.setStatus("history-search", undefined);
		}
		await ctx.ui.custom<void>(
			(tui, theme, _kb, done) => new HistoryOverlay(done, tui, theme, { base, dir, config, initialQuery }),
			{
				overlay: true,
				overlayOptions: { anchor: "center", width: "80%", maxHeight: "80%" },
			}
		);
	}

	pi.registerShortcut("ctrl+shift+f", {
		description: "Search session history",
		handler: (ctx) => openOverlay(ctx as ExtensionContext),
	});

	// ── HistorySearch ────────────────────────────────────────────────
	pi.registerTool({
		name: "HistorySearch",
		label: "History Search",
		description:
			"Search your own past session history (previous conversations and tool activity) with full-text ranked search. " +
			"Use this to recall earlier decisions, prior solutions, file paths, error messages, or what was already tried — " +
			"before re-deriving them. Returns matching sessions with snippets and a sessionId + msgIndex for each hit; " +
			"follow up with HistoryRead to expand any hit. Defaults to the current project; pass scope='all' to search " +
			"other projects the environment exposes.",
		promptSnippet: "HistorySearch — full-text search over your own past sessions (recall prior work).",
		parameters: HistorySearchParams,

		async execute(_id, params, _signal, _onUpdate, ctx): Promise<AgentToolResult<unknown>> {
			const config = loadConfig(ctx.cwd);
			if (!config.enabled) return disabled("search");
			try {
				const hits = await runSearch(ctx, config, params);
				const scope = params.scope ?? "project";
				return {
					content: [{ type: "text", text: formatHits(params.query, scope, hits) }],
					details: { action: "search", query: params.query, scope, count: hits.length, hits },
				};
			} catch (e) {
				const msg = e instanceof Error ? e.message : String(e);
				return {
					content: [{ type: "text", text: `History search failed: ${msg}` }],
					details: { action: "search", error: msg },
				};
			}
		},

		renderCall(args, theme: Theme) {
			const q = (args.query as string) ?? "";
			const scope = (args.scope as string) ?? "project";
			return new Text(theme.fg("toolTitle", theme.bold("HistorySearch ")) + theme.fg("muted", `"${q}" (${scope})`), 0, 0);
		},

		renderResult(result, _opts, theme: Theme) {
			const d = result.details as { count?: number; error?: string } | undefined;
			if (d?.error) return new Text(theme.fg("error", `Error: ${d.error}`), 0, 0);
			const text = result.content?.[0]?.type === "text" ? result.content[0].text : "";
			return new Text(theme.fg("muted", `${d?.count ?? 0} session(s)\n`) + text, 0, 0);
		},
	});

	// ── HistoryRead ──────────────────────────────────────────────────
	pi.registerTool({
		name: "HistoryRead",
		label: "History Read",
		description:
			"Read fuller context from one past session returned by HistorySearch. Modes: pass `around` (a msgIndex) for a " +
			"window of surrounding messages; pass `query` to return every message matching terms; pass neither for the whole " +
			"session, which defaults to a compact `outline` (user+assistant only, tool noise dropped) — set view='transcript' " +
			"for everything. Identify the session with `sessionId` from a HistorySearch result.",
		parameters: HistoryReadParams,

		async execute(_id, params, _signal, _onUpdate, ctx): Promise<AgentToolResult<unknown>> {
			const config = loadConfig(ctx.cwd);
			if (!config.enabled) return disabled("read");
			try {
				const base = resolveSessionsBase(config);
				const filePath = findSessionPath(base, params.sessionId, projectDir(base, ctx.cwd));
				if (!filePath) {
					return {
						content: [{ type: "text", text: `No accessible session found with id "${params.sessionId}".` }],
						details: { action: "read", error: "not found" },
					};
				}
				const r = readSession(filePath, {
					query: params.query,
					around: params.around,
					before: params.before,
					after: params.after,
					roleFilter: params.roleFilter,
					view: params.view ?? "outline",
					maxChars: params.maxChars ?? 2000,
				});
				return {
					content: [{ type: "text", text: formatRead(r) }],
					details: { action: "read", sessionId: r.sessionId, mode: r.mode, returned: r.messages.length },
				};
			} catch (e) {
				const msg = e instanceof Error ? e.message : String(e);
				return {
					content: [{ type: "text", text: `History read failed: ${msg}` }],
					details: { action: "read", error: msg },
				};
			}
		},

		renderCall(args, theme: Theme) {
			const id = shortId((args.sessionId as string) ?? "");
			const mode = args.around !== undefined ? `around ${args.around}` : args.query ? `query "${args.query}"` : "transcript";
			return new Text(theme.fg("toolTitle", theme.bold("HistoryRead ")) + theme.fg("muted", `${id} · ${mode}`), 0, 0);
		},

		renderResult(result, _opts, theme: Theme) {
			const d = result.details as { error?: string; returned?: number } | undefined;
			if (d?.error) return new Text(theme.fg("error", `Error: ${d.error}`), 0, 0);
			const text = result.content?.[0]?.type === "text" ? result.content[0].text : "";
			return new Text(text, 0, 0);
		},
	});

	// ── /history command (humans: stats / reindex / quick search) ─────
	pi.registerCommand("history", {
		description: "Search session history, or manage the index: /history [stats|reindex|<query>]",
		handler: async (args, ctx) => {
			const config = loadConfig(ctx.cwd);
			const base = resolveSessionsBase(config);
			const dir = projectDir(base, ctx.cwd);
			const arg = args?.trim() ?? "";

			if (arg === "stats") {
				const s = getStats(dir);
				ctx.ui.notify(
					`History index: ${s.totalSessions} sessions, ${s.totalChunks} chunks · updated ${s.lastUpdated ?? "never"} · ${s.indexPath}`,
					"info"
				);
				return;
			}

			if (arg === "reindex") {
				ctx.ui.notify("Rebuilding history index…", "info");
				try {
					const n = await rebuildProjectIndex(dir, config);
					ctx.ui.notify(`Rebuilt history index: ${n} sessions`, "info");
				} catch (e) {
					ctx.ui.notify(`Reindex failed: ${e instanceof Error ? e.message : String(e)}`, "error");
				}
				return;
			}

			// Interactive TUI: open the live overlay (optionally seeded with the query).
			if (ctx.hasUI) {
				await openOverlay(ctx, arg || undefined);
				return;
			}

			if (!arg) {
				console.log("Usage: /history [stats|reindex|<query>] — agents use the HistorySearch tool");
				return;
			}

			// Headless (rpc/print): fall back to printing ranked results.
			try {
				const hits = await runSearch(ctx, config, { query: arg, scope: "project" });
				console.log(formatHits(arg, "project", hits));
			} catch (e) {
				ctx.ui.notify(`Search failed: ${e instanceof Error ? e.message : String(e)}`, "error");
			}
		},
	});
}
