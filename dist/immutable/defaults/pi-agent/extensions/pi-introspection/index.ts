import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

// Use unknown + optional chaining instead of any
interface SessionManagerLike {
	getMetadata?: () => { title?: string } | undefined;
	getEntries?: () => { metadata?: { title?: string }; title?: string }[];
	[key: string]: unknown;
}

interface ExtensionAPIWithTitle extends ExtensionAPI {
	getSessionTitle?: () => string | undefined;
}

function getSessionTitle(pi: ExtensionAPI, sessionManager: SessionManagerLike): string | null {
	const directTitle = (pi as ExtensionAPIWithTitle).getSessionTitle?.();
	if (directTitle) return directTitle;

	const metadata = sessionManager.getMetadata?.();
	if (metadata?.title) return metadata.title;

	const entries = sessionManager.getEntries?.() || [];
	for (const entry of entries) {
		if (entry.metadata?.title) return entry.metadata.title;
		if (entry.title) return entry.title;
	}

	return null;
}

interface ExtensionInfo {
	name: string;
	description?: string;
	hasPackageJson: boolean;
}

function discoverExtensions(cwd: string): ExtensionInfo[] {
	const extensions: ExtensionInfo[] = [];
	const home = process.env.HOME ?? "";
	const dirs = [join(home, ".pi", "agent", "extensions"), join(cwd, ".pi", "extensions")];

	for (const dir of dirs) {
		if (!existsSync(dir)) continue;
		try {
			const entries = readdirSync(dir, { withFileTypes: true });
			for (const entry of entries) {
				if (!entry.isDirectory() || entry.name.startsWith(".")) continue;
				const extDir = join(dir, entry.name);
				if (!existsSync(join(extDir, "index.ts"))) continue;
				const info = readExtensionInfo(extDir, entry.name);
				extensions.push(info);
			}
		} catch {
			// directory not readable
		}
	}
	return extensions;
}

function readExtensionInfo(extDir: string, name: string): ExtensionInfo {
	let description: string | undefined;
	let hasPackageJson = false;
	const pkgPath = join(extDir, "package.json");
	if (existsSync(pkgPath)) {
		hasPackageJson = true;
		try {
			const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
			description = pkg.description;
		} catch {
			// ignore parse errors
		}
	}
	return { name, description, hasPackageJson };
}

interface ModelWithThinking {
	supportsThinking?: boolean;
}

function collectModelInfo(ctx: ExtensionContext, pi: ExtensionAPI): Record<string, unknown> {
	const model = ctx.model;
	return {
		current: model
			? {
					id: model.id,
					name: model.name,
					provider: model.provider,
					contextWindow: model.contextWindow,
					maxTokens: model.maxTokens,
					reasoning: model.reasoning,
					supportsThinking: (model as unknown as ModelWithThinking).supportsThinking,
					inputTypes: model.input,
					cost: model.cost,
				}
			: null,
		thinkingLevel: pi.getThinkingLevel?.(),
		allAvailable: ctx.modelRegistry.getAll().map((m) => ({
			id: m.id,
			name: m.name,
			provider: m.provider,
		})),
	};
}

function collectSessionInfo(ctx: ExtensionContext, pi: ExtensionAPI): Record<string, unknown> {
	const sessionManager = ctx.sessionManager;
	const entries = sessionManager.getEntries();
	const branch = sessionManager.getBranch();
	const { firstTimestamp, lastTimestamp } = getTimestampRange(entries);

	return {
		title: getSessionTitle(pi, sessionManager as unknown as SessionManagerLike),
		file: sessionManager.getSessionFile(),
		workingDirectory: ctx.cwd,
		totalEntries: entries.length,
		branchEntries: branch.length,
		currentLeafId: sessionManager.getLeafId(),
		firstMessageDate: firstTimestamp ? new Date(firstTimestamp).toISOString() : null,
		lastMessageDate: lastTimestamp ? new Date(lastTimestamp).toISOString() : null,
		labels: entries
			.filter((e) => sessionManager.getLabel(e.id))
			.map((e) => ({
				entryId: e.id,
				label: sessionManager.getLabel(e.id),
			})),
	};
}

function collectExtensionsInfo(ctx: ExtensionContext, pi: ExtensionAPI): Record<string, unknown> {
	const activeTools = pi.getActiveTools?.() ?? [];
	const allTools = pi.getAllTools?.()?.map((t) => ({ name: t.name, description: t.description })) ?? [];
	const commands = pi.getCommands?.() ?? [];
	const installed = discoverExtensions(ctx.cwd);

	return {
		installed: installed.map((e) => ({
			name: e.name,
			...(e.description ? { description: e.description } : {}),
		})),
		activeTools,
		allTools,
		commands,
	};
}

function getTimestampRange(entries: { timestamp?: unknown }[]): { firstTimestamp: number | null; lastTimestamp: number | null } {
	const timestamps = entries.map((e) => e.timestamp).filter((t): t is number => typeof t === "number");
	if (timestamps.length === 0) return { firstTimestamp: null, lastTimestamp: null };
	return { firstTimestamp: Math.min(...timestamps), lastTimestamp: Math.max(...timestamps) };
}

export default function (pi: ExtensionAPI) {
	pi.registerTool({
		name: "self_reflection",
		label: "Self Reflection",
		description:
			"Query information about the current pi session, including the active model, context usage, and configuration. " +
			"Use this when you need to know which model you are, what your capabilities are, or session metadata.",
		parameters: Type.Object({
			info: Type.Optional(
				Type.String({
					description: "Specific information to query (model, session, context, all). Defaults to 'all'.",
					enum: ["model", "session", "context", "all"],
				})
			),
		}),

		async execute(_toolCallId, params, _signal, _onUpdate, ctx: ExtensionContext) {
			const infoType = params.info ?? "all";
			const result: Record<string, unknown> = {};

			if (infoType === "model" || infoType === "all") {
				result.model = collectModelInfo(ctx, pi);
			}
			if (infoType === "session" || infoType === "all") {
				result.session = collectSessionInfo(ctx, pi);
			}
			if (infoType === "context" || infoType === "all") {
				result.context = ctx.getContextUsage?.() ?? null;
			}
			if (infoType === "all") {
				result.extensions = collectExtensionsInfo(ctx, pi);
			}

			return {
				content: [{ type: "text", text: formatResult(result, infoType) }],
				details: result,
			};
		},
	});

	pi.registerCommand("self", {
		description: "Display current session and model information",
		handler: async (_args, ctx) => {
			ctx.ui.notify(buildSelfCommandOutput(ctx, pi), "info");
		},
	});
}

function buildSelfCommandOutput(ctx: ExtensionContext, pi: ExtensionAPI): string {
	const model = ctx.model;
	const usage = ctx.getContextUsage?.();
	const sessionManager = ctx.sessionManager;
	const entries = sessionManager.getEntries();
	const { firstTimestamp, lastTimestamp } = getTimestampRange(entries);
	const title = getSessionTitle(pi, sessionManager as unknown as SessionManagerLike);
	const installed = discoverExtensions(ctx.cwd);

	const lines: string[] = [];
	lines.push(`**Title**: ${title ?? "(untitled - use /name to set)"}\n`);
	lines.push(`**Model**: ${model?.name ?? "Unknown"} (${model?.provider ?? "?"}/${model?.id ?? "?"})`);
	lines.push(`**Thinking Level**: ${pi.getThinkingLevel?.() ?? "off"}`);
	lines.push(
		`**Context**: ${usage ? `${usage.tokens?.toLocaleString() ?? "?"} tokens (${usage.percent?.toFixed(1) ?? "?"}%)` : "unknown"}`
	);
	lines.push(`**Working Directory**: ${ctx.cwd}`);
	lines.push(`**Session File**: ${sessionManager.getSessionFile() ?? "ephemeral"}`);
	lines.push(`**Entries**: ${entries.length} total`);

	if (firstTimestamp) lines.push(`**First Message**: ${new Date(firstTimestamp).toLocaleString()}`);
	if (lastTimestamp) lines.push(`**Last Message**: ${new Date(lastTimestamp).toLocaleString()}`);

	if (installed.length > 0) {
		lines.push(`\n**Extensions** (${installed.length}):`);
		for (const ext of installed) {
			lines.push(`  - ${ext.name}${ext.description ? `: ${ext.description}` : ""}`);
		}
	}

	return lines.join("\n");
}

function formatModelSection(modelInfo: Record<string, unknown>): string[] {
	const lines: string[] = [];
	if (!modelInfo?.current) return lines;
	const current = modelInfo.current as Record<string, unknown>;
	lines.push("**Current Model:**");
	lines.push(`  Name: ${current.name}`);
	lines.push(`  ID: ${current.provider}/${current.id}`);
	lines.push(`  Context Window: ${current.contextWindow?.toLocaleString()} tokens`);
	lines.push(`  Max Output: ${current.maxTokens?.toLocaleString()} tokens`);
	lines.push(`  Reasoning: ${current.reasoning ? "yes" : "no"}`);
	lines.push(`  Thinking Level: ${modelInfo.thinkingLevel ?? "off"}`);
	lines.push("");
	return lines;
}

function formatSessionSection(sessionInfo: Record<string, unknown>): string[] {
	const lines: string[] = [];
	if (!sessionInfo) return lines;
	lines.push("**Session:**");
	lines.push(`  Title: ${sessionInfo.title ?? "(untitled)"}`);
	lines.push(`  File: ${sessionInfo.file ?? "ephemeral"}`);
	lines.push(`  Working Directory: ${sessionInfo.workingDirectory}`);
	lines.push(`  Total Entries: ${sessionInfo.totalEntries}`);
	lines.push(`  Current Branch: ${sessionInfo.branchEntries} entries`);
	if (sessionInfo.firstMessageDate) lines.push(`  First Message: ${sessionInfo.firstMessageDate}`);
	if (sessionInfo.lastMessageDate) lines.push(`  Last Message: ${sessionInfo.lastMessageDate}`);
	lines.push("");
	return lines;
}

function formatContextSection(contextInfo: Record<string, unknown> | null, isContextOnly: boolean): string[] {
	if (contextInfo) {
		const pct = contextInfo.percent;
		return [
			"**Context Usage:**",
			`  Tokens: ${contextInfo.tokens?.toLocaleString() ?? "unknown"}`,
			`  Percentage: ${typeof pct === "number" ? `${pct.toFixed(1)}%` : "unknown"}`,
			"",
		];
	}
	if (isContextOnly) return ["Context usage information not available."];
	return [];
}

function formatExtensionsSection(extInfo: Record<string, unknown>): string[] {
	const lines: string[] = [];
	if (!extInfo) return lines;
	const installed = extInfo.installed as { name: string; description?: string }[];
	if (installed?.length > 0) {
		lines.push(`**Extensions (${installed.length}):**`);
		for (const ext of installed) {
			lines.push(`  - ${ext.name}${ext.description ? ` -- ${ext.description}` : ""}`);
		}
		lines.push("");
	}
	const activeTools = extInfo.activeTools as string[];
	lines.push("**Active Tools:**");
	lines.push(`  ${activeTools.join(", ")}`);
	lines.push("");
	return lines;
}

function formatResult(result: Record<string, unknown>, infoType: string): string {
	const lines: string[] = [];

	if (infoType === "model" || infoType === "all") {
		lines.push(...formatModelSection(result.model as Record<string, unknown>));
	}
	if (infoType === "session" || infoType === "all") {
		lines.push(...formatSessionSection(result.session as Record<string, unknown>));
	}
	if (infoType === "context" || infoType === "all") {
		lines.push(...formatContextSection(result.context as Record<string, unknown> | null, infoType === "context"));
	}
	if (infoType === "all") {
		lines.push(...formatExtensionsSection(result.extensions as Record<string, unknown>));
	}

	return lines.join("\n");
}
