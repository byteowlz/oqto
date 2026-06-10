/**
 * Interactive TUI overlay for searching session history.
 *
 * A lean live-search palette: type to filter the current project's history,
 * navigate hits, and preview the matched session inline. Composed as a single
 * `Component` (per pi-tui's interface) and shown via `ctx.ui.custom({overlay})`.
 *
 * Search is synchronous against the already-built index (the caller refreshes
 * the index once before opening), so there is no per-keystroke re-indexing.
 */

import type { Theme } from "@earendil-works/pi-coding-agent";
import { type Component, type TUI, matchesKey, truncateToWidth } from "@earendil-works/pi-tui";
import type { HistorySearchConfig } from "./config.js";
import { type HistoryHit, type ReadResult, findSessionPath, listRecent, queryProject, readSession } from "./indexer.js";

export interface OverlayDeps {
	base: string;
	dir: string;
	config: HistorySearchConfig;
	/** Optional seed query (e.g. from `/history <query>`). */
	initialQuery?: string;
}

const RESULT_ROWS = 12;
const PREVIEW_ROWS = 22;

function shortDate(ts: string): string {
	if (!ts) return "—";
	const d = new Date(ts);
	if (Number.isNaN(d.getTime())) return ts.slice(0, 16);
	return d.toISOString().slice(0, 16).replace("T", " ");
}

function shortProject(project: string, max: number): string {
	if (project.length <= max) return project;
	return `…${project.slice(project.length - max + 1)}`;
}

function oneLine(s: string): string {
	return s.replace(/\s+/g, " ").trim();
}

/** Basic greedy word-wrap to a column width. */
function wrap(text: string, width: number): string[] {
	const out: string[] = [];
	for (const para of text.split("\n")) {
		let line = "";
		for (const word of para.split(/\s+/)) {
			if (!word) continue;
			if (line.length === 0) {
				line = word;
			} else if (line.length + 1 + word.length <= width) {
				line += ` ${word}`;
			} else {
				out.push(line);
				line = word;
			}
		}
		out.push(line);
	}
	return out.length ? out : [""];
}

function isPrintable(data: string): boolean {
	if (!data || data.startsWith("\x1b")) return false;
	for (const ch of data) {
		const code = ch.codePointAt(0) ?? 0;
		if (code < 0x20 || code === 0x7f) return false;
	}
	return true;
}

export class HistoryOverlay implements Component {
	private query = "";
	private results: HistoryHit[];
	private selected = 0;
	private mode: "search" | "preview" = "search";
	private preview: { read: ReadResult; lines: string[]; scroll: number } | null = null;

	constructor(
		private readonly done: () => void,
		private readonly tui: TUI,
		private readonly theme: Theme,
		private readonly deps: OverlayDeps
	) {
		this.query = deps.initialQuery?.trim() ?? "";
		this.results = this.query
			? queryProject(deps.dir, this.query, deps.config, deps.config.maxResults)
			: listRecent(deps.dir, deps.config.maxResults);
	}

	invalidate(): void {
		// Stateless render — nothing cached to invalidate.
	}

	// ── Input ─────────────────────────────────────────────────────────

	handleInput(data: string): void {
		if (this.mode === "preview") {
			this.handlePreviewInput(data);
		} else {
			this.handleSearchInput(data);
		}
	}

	private runSearch(): void {
		this.results = this.query.trim()
			? queryProject(this.deps.dir, this.query, this.deps.config, this.deps.config.maxResults)
			: listRecent(this.deps.dir, this.deps.config.maxResults);
		this.selected = 0;
	}

	private handleSearchInput(data: string): void {
		if (matchesKey(data, "escape")) {
			this.done();
			return;
		}
		if (matchesKey(data, "return")) {
			if (this.results.length > 0) this.enterPreview();
		} else if (matchesKey(data, "up")) {
			this.selected = Math.max(0, this.selected - 1);
		} else if (matchesKey(data, "down")) {
			this.selected = Math.min(this.results.length - 1, this.selected + 1);
		} else if (matchesKey(data, "backspace")) {
			if (this.query) {
				this.query = this.query.slice(0, -1);
				this.runSearch();
			}
		} else if (matchesKey(data, "ctrl+u")) {
			this.query = "";
			this.runSearch();
		} else if (isPrintable(data)) {
			this.query += data;
			this.runSearch();
		}
		this.tui.requestRender();
	}

	private handlePreviewInput(data: string): void {
		if (matchesKey(data, "escape") || matchesKey(data, "left") || matchesKey(data, "return")) {
			this.mode = "search";
			this.preview = null;
		} else if (this.preview) {
			const maxScroll = Math.max(0, this.preview.lines.length - PREVIEW_ROWS);
			if (matchesKey(data, "up")) this.preview.scroll = Math.max(0, this.preview.scroll - 1);
			else if (matchesKey(data, "down")) this.preview.scroll = Math.min(maxScroll, this.preview.scroll + 1);
			else if (matchesKey(data, "pageUp")) this.preview.scroll = Math.max(0, this.preview.scroll - PREVIEW_ROWS);
			else if (matchesKey(data, "pageDown")) this.preview.scroll = Math.min(maxScroll, this.preview.scroll + PREVIEW_ROWS);
		}
		this.tui.requestRender();
	}

	private enterPreview(): void {
		const hit = this.results[this.selected];
		const path = findSessionPath(this.deps.base, hit.sessionId, this.deps.dir);
		const firstMatch = hit.matches[0]?.msgIndex;
		const read: ReadResult = path
			? readSession(
					path,
					typeof firstMatch === "number" ? { around: firstMatch, before: 4, after: 12, maxChars: 600 } : { maxChars: 8000 }
				)
			: {
					sessionId: hit.sessionId,
					project: hit.project,
					timestamp: hit.timestamp,
					totalMessages: 0,
					mode: "transcript",
					messages: [],
					truncated: false,
				};
		this.preview = { read, lines: [], scroll: 0 };
		this.mode = "preview";
	}

	// ── Render ────────────────────────────────────────────────────────

	render(width: number): string[] {
		const w = Math.max(20, width);
		return this.mode === "preview" ? this.renderPreview(w) : this.renderSearch(w);
	}

	private renderSearch(w: number): string[] {
		const t = this.theme;
		const lines: string[] = [];
		lines.push(t.fg("accent", t.bold("History search")) + t.fg("muted", "  ↑↓ select · ⏎ preview · Esc close"));
		lines.push(t.fg("text", "> ") + this.query + t.fg("accent", "▏"));
		lines.push("");

		if (this.results.length === 0) {
			lines.push(t.fg("muted", this.query ? `No sessions match "${this.query}"` : "No sessions yet"));
			return lines;
		}

		const start = Math.max(0, Math.min(this.selected - RESULT_ROWS + 1, this.results.length - RESULT_ROWS));
		const view = this.results.slice(Math.max(0, start), Math.max(0, start) + RESULT_ROWS);
		view.forEach((h, k) => {
			const idx = Math.max(0, start) + k;
			const isSel = idx === this.selected;
			const marker = isSel ? t.fg("success", "▸ ") : "  ";
			const meta = `${shortProject(h.project, 22)} · ${shortDate(h.timestamp)}`;
			const titleText = oneLine(h.title ?? h.sessionId);
			const headRaw = `${meta} · ${titleText}`;
			const head = isSel ? t.fg("text", t.bold(truncateToWidth(headRaw, w - 2))) : t.fg("text", truncateToWidth(headRaw, w - 2));
			lines.push(marker + head);
			const snippet = h.matches[0]?.snippet;
			if (snippet) {
				lines.push(t.fg("dim", truncateToWidth(`    ${oneLine(snippet)}`, w)));
			}
		});

		lines.push("");
		lines.push(t.fg("muted", `${this.results.length} session(s)${this.query ? "" : " — recent"}`));
		return lines;
	}

	private renderPreview(w: number): string[] {
		const t = this.theme;
		if (!this.preview) return [t.fg("muted", "…")];
		// Build wrapped lines lazily once we know the width.
		if (this.preview.lines.length === 0) this.preview.lines = this.buildPreviewLines(w);
		const r = this.preview.read;
		const lines: string[] = [];
		lines.push(
			t.fg("accent", t.bold(`${shortProject(r.project, 30)} · ${shortDate(r.timestamp)}`)) +
				t.fg("muted", `  (${r.totalMessages} msgs) · ↑↓ scroll · Esc back`)
		);
		lines.push("");
		const window = this.preview.lines.slice(this.preview.scroll, this.preview.scroll + PREVIEW_ROWS);
		lines.push(...window);
		if (this.preview.lines.length > PREVIEW_ROWS) {
			const shown = Math.min(this.preview.scroll + PREVIEW_ROWS, this.preview.lines.length);
			lines.push("");
			lines.push(t.fg("muted", `lines ${this.preview.scroll + 1}–${shown} / ${this.preview.lines.length}`));
		}
		return lines;
	}

	private buildPreviewLines(w: number): string[] {
		const t = this.theme;
		const r = this.preview?.read;
		if (!r || r.messages.length === 0) return [t.fg("muted", "(no content)")];
		const out: string[] = [];
		for (const m of r.messages) {
			out.push(
				t.fg(m.role === "user" ? "success" : m.role === "assistant" ? "accent" : "muted", t.bold(`[${m.msgIndex}] ${m.role}`))
			);
			for (const ln of wrap(m.text, w)) out.push(t.fg("text", ln));
			out.push("");
		}
		return out;
	}
}
