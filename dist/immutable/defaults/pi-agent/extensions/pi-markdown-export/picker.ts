/**
 * TUI multi-select picker for sessions, modeled on pi-trx-picker.
 *
 * Type to fuzzy-filter, ↑/↓ to navigate, Space/Tab to toggle selection, Enter to
 * confirm (the checked set, or the highlighted row when nothing is checked), Esc
 * to cancel. Returns the chosen sessions; the caller exports them.
 */

import type { Theme } from "@earendil-works/pi-coding-agent";
import { CURSOR_MARKER, type Component, matchesKey, truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";
import type { SessionMeta } from "./session.js";

function fuzzyMatch(needle: string, haystack: string): boolean {
	const lower = haystack.toLowerCase();
	return needle
		.toLowerCase()
		.split(/\s+/)
		.filter(Boolean)
		.every((term) => lower.includes(term));
}

function shortDate(ts: string): string {
	if (!ts) return "—";
	const d = new Date(ts);
	if (Number.isNaN(d.getTime())) return ts.slice(0, 16);
	return d.toISOString().slice(0, 16).replace("T", " ");
}

function projectLabel(cwd: string): string {
	const parts = cwd.split("/").filter(Boolean);
	return parts.length ? parts[parts.length - 1] : cwd;
}

function haystackFor(s: SessionMeta): string {
	return `${s.sessionId} ${projectLabel(s.cwd)} ${s.cwd} ${shortDate(s.timestamp)} ${s.title ?? ""}`;
}

function at<T>(arr: readonly T[], index: number): T {
	return arr[index] as T;
}

const MAX_VISIBLE = 18;

export class SessionPickerOverlay implements Component {
	focused = true;

	private filtered: SessionMeta[];
	private selected = 0;
	private checked = new Set<string>();
	private query = "";
	private scrollOffset = 0;

	constructor(
		private readonly theme: Theme,
		private readonly done: (result: SessionMeta[] | undefined) => void,
		private readonly sessions: SessionMeta[],
		private readonly showProject: boolean
	) {
		this.filtered = [...sessions];
	}

	invalidate(): void {
		/* stateless render */
	}

	private refilter(): void {
		this.filtered = this.query ? this.sessions.filter((s) => fuzzyMatch(this.query, haystackFor(s))) : [...this.sessions];
		this.selected = Math.min(this.selected, Math.max(0, this.filtered.length - 1));
		this.scrollOffset = 0;
	}

	private toggleCurrent(): void {
		if (this.filtered.length === 0) return;
		const item = at(this.filtered, this.selected);
		if (this.checked.has(item.sessionId)) this.checked.delete(item.sessionId);
		else this.checked.add(item.sessionId);
	}

	private chosen(): SessionMeta[] {
		if (this.checked.size > 0) return this.sessions.filter((s) => this.checked.has(s.sessionId));
		if (this.filtered.length > 0) return [at(this.filtered, this.selected)];
		return [];
	}

	private handleTextInput(data: string): void {
		if (matchesKey(data, "backspace")) {
			if (this.query) {
				this.query = this.query.slice(0, -1);
				this.refilter();
			}
			return;
		}
		if (matchesKey(data, "ctrl+u")) {
			this.query = "";
			this.refilter();
			return;
		}
		if (data.length === 1 && data.charCodeAt(0) >= 32 && data.charCodeAt(0) !== 127) {
			this.query += data;
			this.refilter();
		}
	}

	handleInput(data: string): void {
		if (matchesKey(data, "escape")) {
			this.done(undefined);
			return;
		}
		if (matchesKey(data, "return")) {
			const chosen = this.chosen();
			if (chosen.length > 0) this.done(chosen);
			return;
		}
		if (data === " ") {
			this.toggleCurrent();
			return;
		}
		if (matchesKey(data, "tab")) {
			this.toggleCurrent();
			if (this.selected < this.filtered.length - 1) this.selected++;
			return;
		}
		if (matchesKey(data, "ctrl+a")) {
			this.toggleAll();
			return;
		}
		if (matchesKey(data, "up") || matchesKey(data, "ctrl+p")) {
			if (this.selected > 0) this.selected--;
			return;
		}
		if (matchesKey(data, "down") || matchesKey(data, "ctrl+n")) {
			if (this.selected < this.filtered.length - 1) this.selected++;
			return;
		}
		this.handleTextInput(data);
	}

	private toggleAll(): void {
		const allChecked = this.filtered.length > 0 && this.filtered.every((s) => this.checked.has(s.sessionId));
		for (const s of this.filtered) {
			if (allChecked) this.checked.delete(s.sessionId);
			else this.checked.add(s.sessionId);
		}
	}

	private renderRow(s: SessionMeta, isSelected: boolean, innerW: number, row: (c: string) => string): string {
		const th = this.theme;
		const checkbox = this.checked.has(s.sessionId) ? th.fg("success", "◉") : th.fg("dim", "○");
		const pointer = isSelected ? th.fg("accent", "▸") : " ";
		const date = th.fg("dim", shortDate(s.timestamp));
		const proj = this.showProject ? `${th.fg("muted", truncateToWidth(projectLabel(s.cwd), 18))} ` : "";
		const prefix = `${pointer} ${checkbox} ${date} ${proj}`;
		const titleText = s.title ?? `(${s.messageCount} msgs) ${s.sessionId}`;
		const titleMaxW = innerW - visibleWidth(prefix) - 1;
		const styled = isSelected ? th.fg("text", titleText) : th.fg("muted", titleText);
		return row(`${prefix}${truncateToWidth(styled, Math.max(10, titleMaxW))}`);
	}

	private renderSearch(innerW: number, row: (c: string) => string): string[] {
		const th = this.theme;
		let display = this.query;
		if (this.focused) display = `${this.query}${CURSOR_MARKER}\x1b[7m \x1b[27m`;
		return [row(` ${th.fg("dim", "/")} ${display}`), row(th.fg("border", ` ${"─".repeat(innerW - 2)} `))];
	}

	render(width: number): string[] {
		const th = this.theme;
		const innerW = Math.min(width - 2, 120);
		const row = (content: string) => {
			const pad = Math.max(0, innerW - visibleWidth(content));
			return `${th.fg("border", "│")}${content}${" ".repeat(pad)}${th.fg("border", "│")}`;
		};

		const lines: string[] = [];
		lines.push(th.fg("border", `╭${"─".repeat(innerW)}╮`));
		const title = ` ${th.fg("accent", th.bold("Export sessions"))}`;
		const count = th.fg("dim", ` ${this.filtered.length}/${this.sessions.length}`);
		const sel = this.checked.size > 0 ? th.fg("warning", ` (${this.checked.size} selected)`) : "";
		lines.push(row(`${title}${count}${sel}`));
		lines.push(row(""));
		lines.push(...this.renderSearch(innerW, row));

		const total = this.filtered.length;
		if (this.selected < this.scrollOffset) this.scrollOffset = this.selected;
		else if (this.selected >= this.scrollOffset + MAX_VISIBLE) this.scrollOffset = this.selected - MAX_VISIBLE + 1;
		const start = this.scrollOffset;
		const end = Math.min(total, start + MAX_VISIBLE);

		if (total === 0) {
			lines.push(row(` ${th.fg("warning", "No matching sessions")}`));
		} else {
			for (let i = start; i < end; i++) {
				lines.push(this.renderRow(at(this.filtered, i), i === this.selected, innerW, row));
			}
			if (total > MAX_VISIBLE) lines.push(row(th.fg("dim", ` ${start + 1}-${end} of ${total}`)));
		}

		lines.push(row(""));
		const sep = th.fg("border", " │ ");
		const help = [
			`${th.fg("dim", "↑↓")} nav`,
			`${th.fg("dim", "Space")} toggle`,
			`${th.fg("dim", "Tab")} toggle+next`,
			`${th.fg("dim", "Ctrl+A")} all`,
			`${th.fg("dim", "Enter")} export`,
			`${th.fg("dim", "Esc")} cancel`,
		].join(sep);
		lines.push(row(` ${help}`));
		lines.push(th.fg("border", `╰${"─".repeat(innerW)}╯`));
		return lines;
	}
}
