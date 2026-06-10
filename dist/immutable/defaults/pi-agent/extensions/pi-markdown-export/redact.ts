/**
 * Redaction pipeline: literal/regex replacements followed by external CLIs.
 *
 * Both stages run over the rendered Markdown *before* it is written to disk, so
 * a missing tool or a detected secret never reaches the export file.
 */

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { MarkdownExportConfig, RedactionCommand, ReplacementRule } from "./config.js";

export interface RedactionResult {
	text: string;
	/** When true, the caller should not write this export (a scan-mode skip). */
	skipped: boolean;
	/** Human-readable notes (missing tools, unmaskable findings, errors). */
	warnings: string[];
}

const SECRET_KEYS = ["Secret", "Raw", "RawV2", "Match"];
const DEFAULT_MASK = "[REDACTED]";
const DEFAULT_TIMEOUT = 30000;

// ── Replacements ─────────────────────────────────────────────────────

function escapeRegExp(s: string): string {
	return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function applyReplacements(text: string, rules: ReplacementRule[]): string {
	let out = text;
	for (const rule of rules) {
		if (!rule.find) continue;
		try {
			if (rule.regex) {
				const flags = rule.flags ?? "g";
				out = out.replace(new RegExp(rule.find, flags), rule.replace);
			} else {
				out = out.replace(new RegExp(escapeRegExp(rule.find), "g"), rule.replace);
			}
		} catch {
			// A bad regex rule is skipped rather than failing the whole export.
		}
	}
	return out;
}

// ── Secret parsing (gitleaks / trufflehog and similar JSON) ──────────

function collectFromObject(obj: unknown, into: Set<string>): void {
	if (!obj || typeof obj !== "object") return;
	const rec = obj as Record<string, unknown>;
	for (const key of SECRET_KEYS) {
		const v = rec[key];
		if (typeof v === "string" && v.trim().length > 3) into.add(v);
	}
}

/** Pull candidate secret strings from JSON-array or NDJSON tool output. */
export function parseSecrets(raw: string): string[] {
	const found = new Set<string>();
	const trimmed = raw.trim();
	if (!trimmed) return [];

	try {
		const parsed = JSON.parse(trimmed);
		if (Array.isArray(parsed)) {
			for (const item of parsed) collectFromObject(item, found);
			return [...found];
		}
		collectFromObject(parsed, found);
		if (found.size > 0) return [...found];
	} catch {
		// Not a single JSON document — fall through to NDJSON.
	}

	for (const line of trimmed.split("\n")) {
		const l = line.trim();
		if (!l.startsWith("{")) continue;
		try {
			collectFromObject(JSON.parse(l), found);
		} catch {
			// skip non-JSON line
		}
	}
	return [...found];
}

function maskSecrets(text: string, secrets: string[], maskWith: string): string {
	let out = text;
	// Longest first so overlapping shorter substrings don't pre-empt full masks.
	for (const secret of [...secrets].sort((a, b) => b.length - a.length)) {
		out = out.replace(new RegExp(escapeRegExp(secret), "g"), maskWith);
	}
	return out;
}

// ── External command execution ───────────────────────────────────────

function substituteArgs(args: string[], filePath: string, reportPath: string): string[] {
	return args.map((a) => a.replace(/\{file\}/g, filePath).replace(/\{report\}/g, reportPath));
}

function label(cmd: RedactionCommand): string {
	return cmd.name ?? cmd.command;
}

interface RunContext {
	filePath: string;
	reportPath: string;
	tmpDir: string;
}

function runFilter(text: string, cmd: RedactionCommand, run: RunContext, warnings: string[]): string {
	const args = substituteArgs(cmd.args ?? [], run.filePath, run.reportPath);
	const res = spawnSync(cmd.command, args, {
		input: cmd.stdin ? text : undefined,
		encoding: "utf-8",
		timeout: cmd.timeoutMs ?? DEFAULT_TIMEOUT,
		maxBuffer: 64 * 1024 * 1024,
	});
	if (res.error) throw res.error;
	if (res.status !== 0 && cmd.required) {
		throw new Error(`${label(cmd)} exited ${res.status}: ${(res.stderr ?? "").slice(0, 200)}`);
	}
	if (cmd.inPlace) {
		try {
			return readFileSync(run.filePath, "utf-8");
		} catch {
			warnings.push(`${label(cmd)}: could not read filtered file; keeping previous content`);
			return text;
		}
	}
	const out = res.stdout ?? "";
	if (!out.trim()) {
		warnings.push(`${label(cmd)}: empty stdout; keeping previous content`);
		return text;
	}
	return out;
}

interface ScanOutcome {
	text: string;
	skipped: boolean;
}

function applyOnFinding(text: string, cmd: RedactionCommand, warnings: string[]): ScanOutcome {
	const policy = cmd.onFinding ?? "warn";
	const name = label(cmd);
	if (policy === "skip") {
		warnings.push(`${name}: flagged secrets that could not be parsed; skipping this export`);
		return { text, skipped: true };
	}
	if (policy === "mask") {
		warnings.push(`${name}: flagged secrets but reported no maskable strings; review the export`);
		return { text, skipped: false };
	}
	warnings.push(`${name}: reported potential secrets (exit non-zero); review the export`);
	return { text, skipped: false };
}

function runScan(text: string, cmd: RedactionCommand, run: RunContext, warnings: string[]): ScanOutcome {
	const args = substituteArgs(cmd.args ?? [], run.filePath, run.reportPath);
	const res = spawnSync(cmd.command, args, {
		encoding: "utf-8",
		timeout: cmd.timeoutMs ?? DEFAULT_TIMEOUT,
		maxBuffer: 64 * 1024 * 1024,
	});
	if (res.error) throw res.error;

	const reportRaw = existsSync(run.reportPath) ? safeRead(run.reportPath) : "";
	const secrets = [...new Set([...parseSecrets(res.stdout ?? ""), ...parseSecrets(reportRaw)])];

	if (secrets.length > 0) {
		const masked = maskSecrets(text, secrets, cmd.maskWith ?? DEFAULT_MASK);
		warnings.push(`${label(cmd)}: masked ${secrets.length} secret(s)`);
		return { text: masked, skipped: false };
	}

	// No maskable strings parsed. A non-zero exit may still signal findings.
	if (res.status !== 0) {
		if (cmd.required) throw new Error(`${label(cmd)} exited ${res.status} with unparseable findings`);
		return applyOnFinding(text, cmd, warnings);
	}
	return { text, skipped: false };
}

function safeRead(p: string): string {
	try {
		return readFileSync(p, "utf-8");
	} catch {
		return "";
	}
}

function isMissingTool(err: unknown): boolean {
	return !!err && typeof err === "object" && (err as { code?: string }).code === "ENOENT";
}

function runCommand(text: string, cmd: RedactionCommand, run: RunContext, warnings: string[]): ScanOutcome {
	// Refresh the temp file so each command sees the latest text.
	writeFileSync(run.filePath, text, "utf-8");
	try {
		if ((cmd.mode ?? "scan") === "filter") {
			return { text: runFilter(text, cmd, run, warnings), skipped: false };
		}
		return runScan(text, cmd, run, warnings);
	} catch (err) {
		if (isMissingTool(err)) {
			const msg = `${label(cmd)}: command not found (${cmd.command})`;
			if (cmd.required) throw new Error(msg);
			warnings.push(`${msg}; skipping`);
			return { text, skipped: false };
		}
		if (cmd.required) throw err;
		warnings.push(`${label(cmd)}: ${err instanceof Error ? err.message : String(err)}; skipping`);
		return { text, skipped: false };
	}
}

/**
 * Run all configured redaction commands in order. Each command sees the output
 * of the previous one. Throws only when a `required` command fails.
 */
export function runRedactionCommands(text: string, cmds: RedactionCommand[]): RedactionResult {
	const warnings: string[] = [];
	if (cmds.length === 0) return { text, skipped: false, warnings };

	const tmpDir = mkdtempSync(join(tmpdir(), "pi-md-redact-"));
	const run: RunContext = {
		tmpDir,
		filePath: join(tmpDir, "export.md"),
		reportPath: join(tmpDir, "report.json"),
	};

	let current = text;
	let skipped = false;
	try {
		for (const cmd of cmds) {
			const outcome = runCommand(current, cmd, run, warnings);
			current = outcome.text;
			if (outcome.skipped) {
				skipped = true;
				break;
			}
		}
	} finally {
		rmSync(tmpDir, { recursive: true, force: true });
	}

	return { text: current, skipped, warnings };
}

/** Full pipeline: replacements then external redaction commands. */
export function redactExport(text: string, config: MarkdownExportConfig): RedactionResult {
	const replaced = applyReplacements(text, config.replacements);
	return runRedactionCommands(replaced, config.redactionCommands);
}
