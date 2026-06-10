/**
 * pi-markdown-export — export Pi sessions to Markdown.
 *
 * Commands:
 *   /export-md [filename.md]   Export the current session (optionally to a path).
 *   /export-md-all [--subdirs] Export every session for the cwd to the export dir.
 *   /export-md-pick            TUI multi-select picker → export dir.
 *
 * All paths run the rendered Markdown through the redaction pipeline (config-driven
 * string replacements + external scan/filter CLIs) before writing.
 */

import { mkdirSync, writeFileSync } from "node:fs";
import { isAbsolute, join, resolve } from "node:path";
import type { ExtensionAPI, ExtensionCommandContext, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { type MarkdownExportConfig, loadConfig, resolveSessionsBase } from "./config.js";
import { SessionPickerOverlay } from "./picker.js";
import { redactExport } from "./redact.js";
import {
	type BranchEntry,
	type RenderOptions,
	type SessionMeta,
	discoverSessions,
	entriesToMarkdown,
	exportFileName,
	sessionToMarkdown,
} from "./session.js";

function timestampSlug(): string {
	return new Date().toISOString().replace(/[:.]/g, "-");
}

function renderOpts(config: MarkdownExportConfig): RenderOptions {
	return {
		includeThinking: config.includeThinking,
		includeToolCalls: config.includeToolCalls,
		includeToolResults: config.includeToolResults,
		maxCharsPerMessage: config.maxCharsPerMessage,
	};
}

function reportWarnings(ctx: ExtensionContext, warnings: string[]): void {
	for (const w of warnings) ctx.ui.notify(`redaction: ${w}`, "warning");
}

/** Redact then write `markdown` to `outPath`. Returns false when redaction skipped it. */
function redactAndWrite(ctx: ExtensionContext, config: MarkdownExportConfig, markdown: string, outPath: string): boolean {
	const result = redactExport(markdown, config);
	reportWarnings(ctx, result.warnings);
	if (result.skipped) {
		ctx.ui.notify(`Skipped (redaction): ${outPath}`, "warning");
		return false;
	}
	writeFileSync(outPath, result.text, "utf-8");
	return true;
}

// ── /export-md — current session ─────────────────────────────────────

function exportCurrent(pi: ExtensionAPI, ctx: ExtensionContext, config: MarkdownExportConfig, args: string): void {
	const provided = args.trim();
	const fileName = provided || `pi-session-${timestampSlug()}.md`;
	const outPath = isAbsolute(fileName) ? fileName : join(ctx.cwd, fileName);

	const branch = ctx.sessionManager.getBranch() as BranchEntry[];
	const title = pi.getSessionName()?.trim() || "Pi Session Export";
	const markdown = entriesToMarkdown(branch, title, renderOpts(config));

	if (redactAndWrite(ctx, config, markdown, outPath)) {
		ctx.ui.notify(`Markdown export written: ${outPath}`, "info");
	}
}

// ── Bulk export to the export dir ────────────────────────────────────

function ensureExportDir(ctx: ExtensionContext, config: MarkdownExportConfig): string {
	const dir = isAbsolute(config.exportDir) ? config.exportDir : resolve(ctx.cwd, config.exportDir);
	mkdirSync(dir, { recursive: true });
	return dir;
}

function exportSessionsToDir(
	ctx: ExtensionContext,
	config: MarkdownExportConfig,
	sessions: SessionMeta[],
	withProject: boolean
): void {
	const dir = ensureExportDir(ctx, config);
	let written = 0;
	let skipped = 0;
	for (const meta of sessions) {
		const markdown = sessionToMarkdown(meta.path, meta.title, renderOpts(config));
		const outPath = join(dir, exportFileName(meta, withProject));
		if (redactAndWrite(ctx, config, markdown, outPath)) written++;
		else skipped++;
	}
	const skipNote = skipped > 0 ? `, ${skipped} skipped` : "";
	ctx.ui.notify(`Exported ${written} session(s) to ${dir}${skipNote}`, "info");
}

function exportAll(ctx: ExtensionContext, config: MarkdownExportConfig, args: string): void {
	const includeSubdirs = config.includeSubdirs || /(^|\s)--subdirs(\s|$)/.test(args);
	const base = resolveSessionsBase(config);
	const sessions = discoverSessions(base, ctx.cwd, includeSubdirs);
	if (sessions.length === 0) {
		ctx.ui.notify("No sessions found for this directory", "warning");
		return;
	}
	exportSessionsToDir(ctx, config, sessions, includeSubdirs);
}

// ── Interactive picker ───────────────────────────────────────────────

async function exportPick(ctx: ExtensionCommandContext, config: MarkdownExportConfig, args: string): Promise<void> {
	const includeSubdirs = config.includeSubdirs || /(^|\s)--subdirs(\s|$)/.test(args);
	const base = resolveSessionsBase(config);

	ctx.ui.setStatus("export-md", "Loading sessions...");
	const sessions = discoverSessions(base, ctx.cwd, includeSubdirs);
	ctx.ui.setStatus("export-md", undefined);

	if (sessions.length === 0) {
		ctx.ui.notify("No sessions found for this directory", "warning");
		return;
	}

	const chosen = await ctx.ui.custom<SessionMeta[] | undefined>(
		(tui, theme, _keybindings, done) => {
			const overlay = new SessionPickerOverlay(theme, done, sessions, includeSubdirs);
			return {
				render: (w: number) => overlay.render(w),
				invalidate: () => overlay.invalidate(),
				handleInput: (data: string) => {
					overlay.handleInput(data);
					tui.requestRender();
				},
				get focused() {
					return overlay.focused;
				},
				set focused(v: boolean) {
					overlay.focused = v;
				},
			};
		},
		{ overlay: true }
	);

	if (!chosen || chosen.length === 0) return;
	exportSessionsToDir(ctx, config, chosen, includeSubdirs);
}

// ── Registration ─────────────────────────────────────────────────────

export default function (pi: ExtensionAPI) {
	pi.registerCommand("export-md", {
		description: "Export current session to Markdown. Usage: /export-md [filename.md]",
		handler: async (args, ctx) => {
			exportCurrent(pi, ctx, loadConfig(ctx.cwd), args);
		},
	});

	pi.registerCommand("export-md-all", {
		description: "Export all sessions for this directory to the export dir. Usage: /export-md-all [--subdirs]",
		handler: async (args, ctx) => {
			exportAll(ctx, loadConfig(ctx.cwd), args);
		},
	});

	pi.registerCommand("export-md-pick", {
		description: "Pick sessions (fuzzy, multi-select) to export. Usage: /export-md-pick [--subdirs]",
		handler: async (args, ctx) => {
			await exportPick(ctx, loadConfig(ctx.cwd), args);
		},
	});
}
