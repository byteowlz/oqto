/**
 * Auto-rename extension for pi sessions.
 *
 * Automatically generates a session name based on the first user query
 * using a configurable LLM model and prompt.
 *
 * By default, uses the current session model. Falls back to the cheapest
 * available model with an API key. Override via config
 * (auto-rename.json in cwd, .pi/, or ~/.pi/agent/):
 * {
 *   "model": { "provider": "anthropic", "id": "claude-3-5-haiku-20241022" },
 *   "fallbackModel": { "provider": "openai", "id": "gpt-4o-mini" },
 *   "fallbackDeterministic": "readable-id",
 *   "modelSelection": "current",
 *   "prompt": "Generate a short, descriptive title...",
 *   "prefix": "[auto] ",
 *   "prefixCommand": "basename $(git rev-parse --show-toplevel 2>/dev/null || pwd)",
 *   "prefixOnly": false,
 *   "readableIdSuffix": false,
 *   "readableIdEnv": "PI_SESSION_READABLE_ID",
 *   "enabled": true,
 *   "debug": false,
 *   "wordlistPath": "./word_lists.toml",
 *   "wordlist": { "adjectives": [], "nouns": [] },
 *   "maxQueryLength": 2000,
 *   "maxNameLength": 80
 * }
 *
 * Prefix options:
 * - "prefix": Static string prefix
 * - "prefixCommand": Shell command whose stdout becomes the prefix (trimmed)
 * - "prefixOnly": If true, skip LLM and use only the prefix as the full name
 *
 * Suffix options:
 * - "readableIdSuffix": If true, append "[readable-id]" to the generated name
 * - "readableIdEnv": Environment variable that can provide a readable-id suffix override
 *
 * Fallback options:
 * - "fallbackModel": Alternative model if primary fails
 * - "fallbackDeterministic": Function to use if all models fail
 *   - "readable-id": Deterministic adjective-noun-noun from session ID
 *   - "truncate": First 50 chars of query
 *   - "words": First 6 words of query
 *   - "none": Don't set a name if LLM fails
 *
 * Input/output guards:
 * - "maxQueryLength": Maximum characters of user query sent to the LLM (default 2000).
 *   Longer queries have their middle section cut out, preserving the beginning and end.
 * - "maxNameLength": Maximum characters for the generated name (default 80).
 *   LLM responses exceeding this are truncated at a word boundary with an ellipsis.
 *
 * The prefixCommand runs in the session's cwd. If it fails, falls back to static prefix.
 */

import { execSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { type Api, type Model, complete } from "@earendil-works/pi-ai";
import type { ExtensionAPI, ExtensionCommandContext, ExtensionContext } from "@earendil-works/pi-coding-agent";

// ============================================================================
// Types
// ============================================================================

interface ModelConfig {
	provider: string;
	id: string;
}

interface WordlistConfig {
	adjectives: string[];
	nouns: string[];
}

interface AutoRenameConfig {
	model?: ModelConfig;
	fallbackModel?: ModelConfig | null;
	fallbackDeterministic?: "truncate" | "words" | "none" | "readable-id";
	modelSelection?: "current" | "cheapest";
	prompt?: string;
	prefix?: string;
	prefixCommand?: string;
	prefixOnly?: boolean;
	readableIdSuffix?: boolean;
	readableIdEnv?: string;
	enabled?: boolean;
	debug?: boolean;
	wordlistPath?: string;
	wordlist?: WordlistConfig;
	maxQueryLength?: number;
	maxNameLength?: number;
}

type ResolvedConfig = {
	model: ModelConfig | null;
	fallbackModel: ModelConfig | null | undefined;
	fallbackDeterministic: "truncate" | "words" | "none" | "readable-id";
	modelSelection: "current" | "cheapest";
	prompt: string;
	prefix: string;
	prefixCommand: string | undefined;
	prefixOnly: boolean;
	readableIdSuffix: boolean;
	readableIdEnv: string | undefined;
	enabled: boolean;
	debug: boolean;
	wordlistPath: string | undefined;
	wordlist: WordlistConfig | undefined;
	maxQueryLength: number;
	maxNameLength: number;
};

interface ModelResolutionResult {
	model: Model<Api> | null;
	apiKey: string | null;
	error: string | null;
	source: "primary" | "fallback" | null;
}

interface NameGenerationResult {
	name: string | null;
	source: "llm-primary" | "llm-fallback" | "deterministic" | null;
	error: string | null;
}

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_PROMPT = `Generate a short, descriptive title (max 6 words) for a chat session based on this first message:

<message>
{{query}}
</message>

Rules:
- Be concise and specific
- Use title case
- No quotes or punctuation at the end
- Focus on the main topic or intent
- If unclear, use a generic but relevant title

Reply with ONLY the title, nothing else.`;

const DEFAULT_CONFIG: ResolvedConfig = {
	model: null,
	fallbackModel: null,
	fallbackDeterministic: "readable-id",
	modelSelection: "current",
	prompt: DEFAULT_PROMPT,
	prefix: "",
	prefixCommand: undefined,
	prefixOnly: false,
	readableIdSuffix: false,
	readableIdEnv: "PI_SESSION_READABLE_ID",
	enabled: true,
	debug: false,
	wordlistPath: undefined,
	wordlist: undefined,
	maxQueryLength: 2000,
	maxNameLength: 80,
};

const CONFIG_FILENAME = "auto-rename.json";
const SUBAGENT_PREFIX_ENV = "PI_SUBAGENT_PREFIX";
const DEFAULT_WORDLIST_PATH = resolveDefaultWordlistPath();

function resolveDefaultWordlistPath(): string {
	// Try import.meta.url (works in native ESM)
	try {
		const dir = dirname(fileURLToPath(import.meta.url));
		const candidate = join(dir, "wordlist", "word_lists.toml");
		if (existsSync(candidate)) return candidate;
	} catch {
		// import.meta.url may not work under jiti
	}

	// Try __dirname (available in CJS and some jiti contexts)
	try {
		// @ts-ignore - __dirname may be defined by jiti at runtime
		if (typeof __dirname === "string") {
			// @ts-ignore
			const candidate = join(__dirname, "wordlist", "word_lists.toml");
			if (existsSync(candidate)) return candidate;
		}
	} catch {
		// not available
	}

	// Fallback: look in the npm global install location
	try {
		const npmRoot = execSync("npm root -g", { encoding: "utf-8", timeout: 3000 }).trim();
		const candidate = join(npmRoot, "@byteowlz", "pi-auto-rename", "wordlist", "word_lists.toml");
		if (existsSync(candidate)) return candidate;
	} catch {
		// npm not available or timed out
	}

	// Last resort: return a path that won't exist but won't crash
	return join("wordlist", "word_lists.toml");
}

/**
 * Status key used to notify the Oqto runner of title changes.
 * The runner watches for this key on extension_ui_request events
 * and broadcasts a canonical session.title_changed event.
 */
const TITLE_CHANGED_STATUS_KEY = "oqto_title_changed";

// ============================================================================
// Config Loading
// ============================================================================

function loadConfig(cwd: string): ResolvedConfig {
	const paths = [join(cwd, CONFIG_FILENAME), join(cwd, ".pi", CONFIG_FILENAME), join(homedir(), ".pi", "agent", CONFIG_FILENAME)];

	for (const path of paths) {
		if (existsSync(path)) {
			try {
				const content = readFileSync(path, "utf-8");
				const userConfig: AutoRenameConfig = JSON.parse(content);
				return { ...DEFAULT_CONFIG, ...userConfig } as ResolvedConfig;
			} catch {
				// Invalid JSON, continue to next path
			}
		}
	}

	return DEFAULT_CONFIG;
}

// ============================================================================
// Wordlist Loading
// ============================================================================

function resolveWordlistPath(config: ResolvedConfig, cwd: string): string | null {
	if (!config.wordlistPath) return null;
	const rawPath = config.wordlistPath.trim();
	if (!rawPath) return null;

	if (rawPath.startsWith("~")) {
		return join(homedir(), rawPath.slice(1));
	}

	if (rawPath.startsWith("/") || rawPath.includes(":")) {
		return rawPath;
	}

	return resolve(cwd, rawPath);
}

function parseWordlistToml(content: string): WordlistConfig | null {
	const adjectives = extractTomlList(content, "adjectives");
	const nouns = extractTomlList(content, "nouns");
	if (adjectives.length === 0 || nouns.length === 0) {
		return null;
	}
	return { adjectives, nouns };
}

function extractTomlList(content: string, key: string): string[] {
	const regex = new RegExp(`${key}\\s*=\\s*\\[([\\s\\S]*?)\\]`, "m");
	const match = content.match(regex);
	if (!match) return [];
	const listBody = match[1];
	return Array.from(listBody.matchAll(/"([^"]+)"/g)).map((m) => m[1]);
}

function loadWordlist(config: ResolvedConfig, cwd: string): WordlistConfig | null {
	if (config.wordlist?.adjectives?.length && config.wordlist.nouns?.length) {
		return {
			adjectives: config.wordlist.adjectives,
			nouns: config.wordlist.nouns,
		};
	}

	const overridePath = resolveWordlistPath(config, cwd);
	const paths = overridePath ? [overridePath, DEFAULT_WORDLIST_PATH] : [DEFAULT_WORDLIST_PATH];

	for (const path of paths) {
		if (!path || !existsSync(path)) continue;
		try {
			const content = readFileSync(path, "utf-8");
			const parsed = parseWordlistToml(content);
			if (parsed) return parsed;
		} catch {
			// Ignore invalid or unreadable wordlist files
		}
	}

	return null;
}

// ============================================================================
// Shell Command Execution
// ============================================================================

function executeCommand(command: string, cwd: string): string | null {
	try {
		const result = execSync(command, {
			cwd,
			encoding: "utf-8",
			timeout: 5000,
			stdio: ["pipe", "pipe", "pipe"],
		});
		return result.trim() || null;
	} catch {
		return null;
	}
}

function resolvePrefix(config: ResolvedConfig, cwd: string, ctx: ExtensionContext): string {
	let basePrefix = config.prefix;
	if (config.prefixCommand) {
		const dynamicPrefix = executeCommand(config.prefixCommand, cwd);
		if (dynamicPrefix) {
			basePrefix = dynamicPrefix;
		} else if (config.debug && ctx.hasUI) {
			ctx.ui.notify("[auto-rename] prefixCommand failed, using static prefix", "warning");
		}
	}
	const subagentPrefix = process.env[SUBAGENT_PREFIX_ENV];
	if (subagentPrefix?.trim()) {
		return basePrefix ? `${subagentPrefix.trim()} ${basePrefix}` : subagentPrefix.trim();
	}
	return basePrefix;
}

// ============================================================================
// Query Extraction
// ============================================================================

function extractFirstUserQuery(ctx: ExtensionContext): string | null {
	const branch = ctx.sessionManager.getBranch();

	for (const entry of branch) {
		if (entry.type !== "message") continue;

		const msg = (entry as { message?: { role?: string; content?: unknown } }).message;
		if (!msg || msg.role !== "user") continue;

		if (!Array.isArray(msg.content)) {
			return typeof msg.content === "string" ? msg.content.trim() || null : null;
		}
		return extractTextFromContent(msg.content as Array<{ type: string; text?: string; thinking?: string }>);
	}

	return null;
}

function getSessionId(ctx: ExtensionContext): string | null {
	const manager = ctx.sessionManager as { getSessionId?: () => string };
	const id = manager.getSessionId?.();
	return id || null;
}

// ============================================================================
// Input / Output Guards
// ============================================================================

/**
 * Truncate the user query to maxQueryLength characters by cutting the middle.
 * Preserves the beginning (intent/context) and end (specific details/question)
 * of the query, joining them with a marker so the LLM understands content was
 * omitted. Both halves are trimmed at word boundaries.
 */
function truncateQuery(query: string, maxLength: number): string {
	if (query.length <= maxLength) return query;
	const marker = "\n[...truncated...]\n";
	const budget = maxLength - marker.length;
	if (budget <= 0) return query.slice(0, maxLength);
	const headBudget = Math.ceil(budget * 0.6);
	const tailBudget = budget - headBudget;

	let head = query.slice(0, headBudget);
	const headSpace = head.lastIndexOf(" ");
	if (headSpace > headBudget * 0.6) {
		head = head.slice(0, headSpace);
	}

	let tail = query.slice(query.length - tailBudget);
	const tailSpace = tail.indexOf(" ");
	if (tailSpace >= 0 && tailSpace < tailBudget * 0.4) {
		tail = tail.slice(tailSpace + 1);
	}

	return `${head}${marker}${tail}`;
}

/**
 * Enforce maxNameLength on an LLM-generated name. Strips trailing
 * punctuation, truncates at a word boundary, and appends an ellipsis when
 * trimming was needed.
 */
function enforceNameLength(name: string, maxLength: number): string {
	if (name.length <= maxLength) return name;
	// Leave room for the ellipsis
	const limit = maxLength - 3;
	if (limit <= 0) return name.slice(0, maxLength);
	const truncated = name.slice(0, limit);
	const lastSpace = truncated.lastIndexOf(" ");
	if (lastSpace > limit * 0.4) {
		return `${truncated.slice(0, lastSpace).replace(/[,;:\s]+$/, "")}...`;
	}
	return `${truncated.replace(/[,;:\s]+$/, "")}...`;
}

// ============================================================================
// Deterministic Name Generation
// ============================================================================

function generateDeterministicName(
	query: string,
	method: "truncate" | "words" | "none" | "readable-id",
	sessionId: string | null,
	wordlist: WordlistConfig | null
): string | null {
	if (method === "none") {
		return null;
	}

	if (method === "readable-id") {
		if (!sessionId || !wordlist) return null;
		return readableIdFromSessionId(sessionId, wordlist);
	}

	const cleaned = query
		.replace(/\s+/g, " ")
		.replace(/[^\w\s-]/g, "")
		.trim();

	if (!cleaned) {
		return null;
	}

	if (method === "words") {
		return generateWordsName(cleaned);
	}

	return generateTruncatedName(cleaned);
}

function hashString(value: string): number {
	let hash = 0;
	for (let i = 0; i < value.length; i++) {
		hash = (hash << 5) - hash + value.charCodeAt(i);
		hash |= 0;
	}
	return Math.abs(hash) >>> 0;
}

function readableIdFromSessionId(sessionId: string, wordlist: WordlistConfig): string {
	const hash = hashString(sessionId);
	const adjectives = wordlist.adjectives;
	const nouns = wordlist.nouns;
	const adjIndex = hash % adjectives.length;
	const noun1Index = Math.floor(hash / adjectives.length) % nouns.length;
	const noun2Index = Math.floor(hash / (adjectives.length * nouns.length)) % nouns.length;
	return `${adjectives[adjIndex]}-${nouns[noun1Index]}-${nouns[noun2Index]}`;
}

function generateWordsName(cleaned: string): string | null {
	const words = cleaned.split(" ").slice(0, 6);
	const titleCase = words.map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase()).join(" ");
	return titleCase || null;
}

function generateTruncatedName(cleaned: string): string {
	if (cleaned.length <= 50) {
		return cleaned;
	}

	const truncated = cleaned.slice(0, 50);
	const lastSpace = truncated.lastIndexOf(" ");

	if (lastSpace > 30) {
		return `${truncated.slice(0, lastSpace)}...`;
	}

	return `${truncated}...`;
}

// ============================================================================
// Model Resolution
// ============================================================================

/**
 * Find the cheapest available model that has an API key configured.
 * Prefers small/fast models suitable for short title generation.
 */
async function findCheapestAvailableModel(ctx: ExtensionContext): Promise<{ model: Model<Api>; apiKey: string } | null> {
	const allModels = ctx.modelRegistry.getAll() as Model<Api>[];
	// Sort by total cost (input + output), cheapest first
	const sorted = [...allModels].sort((a, b) => {
		const costA = (a.cost?.input ?? 0) + (a.cost?.output ?? 0);
		const costB = (b.cost?.input ?? 0) + (b.cost?.output ?? 0);
		return costA - costB;
	});

	for (const model of sorted) {
		const auth = await ctx.modelRegistry.getApiKeyAndHeaders(model);
		if (auth.ok && auth.apiKey) return { model, apiKey: auth.apiKey };
	}
	return null;
}

async function resolveModel(
	modelConfig: ModelConfig,
	ctx: ExtensionContext
): Promise<{ model: Model<Api> | null; apiKey: string | null; error: string | null }> {
	const model = ctx.modelRegistry.find(modelConfig.provider, modelConfig.id) as Model<Api> | undefined;

	if (!model) {
		const providers = Array.from(new Set(ctx.modelRegistry.getAll().map((m) => m.provider)));
		const availableProviders = providers.slice(0, 5).join(", ");
		const suffix = providers.length > 5 ? "..." : "";
		return {
			model: null,
			apiKey: null,
			error: `Model "${modelConfig.id}" not found for provider "${modelConfig.provider}". Available providers: ${availableProviders}${suffix}`,
		};
	}

	const auth = await ctx.modelRegistry.getApiKeyAndHeaders(model);
	if (!auth.ok) {
		return {
			model: null,
			apiKey: null,
			error: `Failed to get API key: ${auth.error}`,
		};
	}
	return { model, apiKey: auth.apiKey ?? null, error: null };
}

function debugNotify(
	ctx: ExtensionContext,
	config: ResolvedConfig,
	message: string,
	level: "info" | "warning" | "error" = "info"
): void {
	if (config.debug && ctx.hasUI) {
		ctx.ui.notify(message, level);
	}
}

async function resolveCurrentModel(ctx: ExtensionContext): Promise<{ model: Model<Api>; apiKey: string } | null> {
	const currentModel = ctx.model as Model<Api> | undefined;
	if (!currentModel) return null;
	const auth = await ctx.modelRegistry.getApiKeyAndHeaders(currentModel);
	if (!auth.ok || !auth.apiKey) return null;
	return { model: currentModel, apiKey: auth.apiKey };
}

async function resolveModelWithFallback(config: ResolvedConfig, ctx: ExtensionContext): Promise<ModelResolutionResult> {
	// 1. Try explicitly configured primary model
	if (config.model) {
		const primary = await resolveModel(config.model, ctx);
		if (primary.model) return { ...primary, source: "primary" };
		if (primary.error) debugNotify(ctx, config, `[auto-rename] Primary model failed: ${primary.error}`, "warning");
	}

	// 2. Try explicitly configured fallback model
	if (config.fallbackModel) {
		const fallback = await resolveModel(config.fallbackModel, ctx);
		if (fallback.model) {
			debugNotify(ctx, config, `[auto-rename] Using fallback model: ${config.fallbackModel.provider}/${config.fallbackModel.id}`);
			return { ...fallback, source: "fallback" };
		}
		if (fallback.error) debugNotify(ctx, config, `[auto-rename] Fallback model failed: ${fallback.error}`, "warning");
	}

	// 3. Strategy-based auto selection
	if (config.modelSelection === "cheapest") {
		const cheapest = await findCheapestAvailableModel(ctx);
		if (cheapest) {
			debugNotify(ctx, config, `[auto-rename] Auto-selected cheapest model: ${cheapest.model.provider}/${cheapest.model.id}`);
			return { model: cheapest.model, apiKey: cheapest.apiKey, error: null, source: "primary" };
		}

		const current = await resolveCurrentModel(ctx);
		if (current) {
			debugNotify(ctx, config, `[auto-rename] Using current model: ${current.model.provider}/${current.model.id}`);
			return { model: current.model, apiKey: current.apiKey, error: null, source: "primary" };
		}
	} else {
		const current = await resolveCurrentModel(ctx);
		if (current) {
			debugNotify(ctx, config, `[auto-rename] Using current model: ${current.model.provider}/${current.model.id}`);
			return { model: current.model, apiKey: current.apiKey, error: null, source: "primary" };
		}

		const cheapest = await findCheapestAvailableModel(ctx);
		if (cheapest) {
			debugNotify(ctx, config, `[auto-rename] Auto-selected cheapest model: ${cheapest.model.provider}/${cheapest.model.id}`);
			return { model: cheapest.model, apiKey: cheapest.apiKey, error: null, source: "primary" };
		}
	}

	return {
		model: null,
		apiKey: null,
		error: "No model available. Configure an API key or set model in auto-rename.json",
		source: null,
	};
}

// ============================================================================
// LLM Name Generation
// ============================================================================

function stripThinkTags(text: string): string {
	return (
		text
			// Strip closed think/thinking blocks
			.replace(/<think>[\s\S]*?<\/think>/gi, "")
			.replace(/<thinking>[\s\S]*?<\/thinking>/gi, "")
			// Strip unclosed think/thinking tags (model didn't close them)
			.replace(/<think>[\s\S]*/gi, "")
			.replace(/<thinking>[\s\S]*/gi, "")
			.trim()
	);
}

function extractTextFromContent(content: Array<{ type: string; text?: string; thinking?: string }>): string {
	// First try text blocks
	const textContent = content
		.filter((c): c is { type: "text"; text: string } => c.type === "text" && typeof c.text === "string")
		.map((c) => c.text)
		.join("")
		.trim();

	if (textContent) return textContent;

	// If no text content, try to extract from thinking blocks
	// (some models put the answer in the thinking block when the response is short)
	const thinkingContent = content
		.filter((c): c is { type: "thinking"; thinking: string } => c.type === "thinking" && typeof c.thinking === "string")
		.map((c) => c.thinking)
		.join("")
		.trim();

	if (!thinkingContent) return "";

	// The thinking block may contain reasoning + the actual title
	// Take the last non-empty line as the most likely title
	const lines = thinkingContent
		.split("\n")
		.map((l) => l.trim())
		.filter((l) => l.length > 0 && !l.startsWith("*") && !l.startsWith("-") && !l.startsWith("#"));

	return lines.length > 0 ? lines[lines.length - 1] : "";
}

function normalizeCandidateLine(line: string): string {
	return line
		.replace(/<[^>]+>/g, " ")
		.replace(/^[\s\-*>#]+/, "")
		.replace(/^\d+[.)]\s*/, "")
		.replace(/\*\*/g, "")
		.replace(/`/g, "")
		.replace(/^["']+|["']+$/g, "")
		.replace(/\s+/g, " ")
		.trim();
}

function looksLikeReasoning(line: string): boolean {
	if (!line) return true;
	if (line.includes("|")) return true;
	if (/^(step|analysis|reasoning|thinking|selecting|here'?s|let'?s)\b/i.test(line)) return true;
	if (/\b(best title|final title|option \d|candidate \d)\b/i.test(line)) return true;
	if (/^\d{1,2}\/\d{1,2}\/\d{2,4}$/.test(line)) return true;
	if (/[{}<>]/.test(line)) return true;
	const words = line.split(/\s+/).filter(Boolean);
	if (words.length < 2 || words.length > 10) return true;
	return false;
}

function parseNameFromResponse(
	response: { content: Array<{ type: string; text?: string; thinking?: string }> },
	maxNameLength: number
): string | null {
	const raw = stripThinkTags(extractTextFromContent(response.content));
	if (!raw) return null;

	const lines = raw
		.split("\n")
		.map(normalizeCandidateLine)
		.filter((l) => l.length > 0);

	// Prefer last clean short line (models often reason first, answer last)
	let name = "";
	for (let i = lines.length - 1; i >= 0; i--) {
		if (!looksLikeReasoning(lines[i])) {
			name = lines[i];
			break;
		}
	}
	if (!name && lines.length > 0) {
		// As a last resort, use the shortest line
		name = [...lines].sort((a, b) => a.length - b.length)[0];
	}

	name = name.replace(/\.+$/, "").trim();
	if (!name || looksLikeReasoning(name)) return null;

	name = enforceNameLength(name, maxNameLength);
	return name || null;
}

function handleLlmError(errorMsg: string, config: ResolvedConfig, ctx: ExtensionContext): void {
	if (config.debug && ctx.hasUI) {
		ctx.ui.notify(`[auto-rename] LLM call failed: ${errorMsg}`, "warning");
	}

	if (!ctx.hasUI) return;

	const providerName = config.model?.provider ?? "unknown";
	if (errorMsg.includes("401") || errorMsg.includes("403") || errorMsg.includes("authentication")) {
		ctx.ui.notify(`[auto-rename] Authentication failed for ${providerName}. Check your API key.`, "error");
	} else if (errorMsg.includes("429") || errorMsg.includes("rate limit")) {
		ctx.ui.notify(`[auto-rename] Rate limited by ${providerName}. Using fallback.`, "warning");
	} else if (errorMsg.includes("timeout") || errorMsg.includes("ETIMEDOUT")) {
		ctx.ui.notify("[auto-rename] Request timed out. Using fallback.", "warning");
	}
}

async function tryLlmGeneration(
	query: string,
	config: ResolvedConfig,
	ctx: ExtensionContext,
	resolution: ModelResolutionResult
): Promise<NameGenerationResult | null> {
	if (!resolution.model) {
		return null;
	}

	const trimmedQuery = truncateQuery(query, config.maxQueryLength);
	if (trimmedQuery.length < query.length && config.debug && ctx.hasUI) {
		ctx.ui.notify(`[auto-rename] Query truncated from ${query.length} to ${trimmedQuery.length} chars`, "info");
	}
	const prompt = config.prompt.replace("{{query}}", trimmedQuery);

	try {
		const controller = new AbortController();
		const timeout = setTimeout(() => controller.abort(), 15000);

		const response = await complete(
			resolution.model,
			{
				messages: [
					{
						role: "user",
						content: [{ type: "text", text: prompt }],
						timestamp: Date.now(),
					},
				],
			},
			{ apiKey: resolution.apiKey ?? undefined, signal: controller.signal }
		);
		clearTimeout(timeout);

		const name = parseNameFromResponse(response, config.maxNameLength);

		if (name) {
			// Check if the raw response was longer (name was truncated by enforceNameLength)
			const rawName = response.content
				.filter((c): c is { type: "text"; text: string } => c.type === "text")
				.map((c) => c.text)
				.join("")
				.trim();
			if (rawName.length > config.maxNameLength && config.debug && ctx.hasUI) {
				ctx.ui.notify(
					`[auto-rename] LLM name truncated from ${rawName.length} to ${name.length} chars (max ${config.maxNameLength})`,
					"info"
				);
			}
			return {
				name,
				source: resolution.source === "primary" ? "llm-primary" : "llm-fallback",
				error: null,
			};
		}

		if (config.debug && ctx.hasUI) {
			ctx.ui.notify("[auto-rename] LLM returned empty response, using deterministic fallback", "warning");
		}
	} catch (error) {
		const errorMsg = error instanceof Error ? error.message : String(error);
		handleLlmError(errorMsg, config, ctx);
	}

	return null;
}

async function generateSessionName(
	query: string,
	config: ResolvedConfig,
	ctx: ExtensionContext,
	sessionId: string | null,
	wordlist: WordlistConfig | null
): Promise<NameGenerationResult> {
	const resolution = await resolveModelWithFallback(config, ctx);

	const llmResult = await tryLlmGeneration(query, config, ctx, resolution);
	if (llmResult) {
		return llmResult;
	}

	const deterministicName = generateDeterministicName(query, config.fallbackDeterministic, sessionId, wordlist);

	if (deterministicName) {
		if (config.debug && ctx.hasUI) {
			ctx.ui.notify(`[auto-rename] Using deterministic fallback (${config.fallbackDeterministic})`, "info");
		}
		return {
			name: deterministicName,
			source: "deterministic",
			error: resolution.error,
		};
	}

	return {
		name: null,
		source: null,
		error: resolution.error || "All name generation methods failed",
	};
}

// ============================================================================
// Session Naming Helpers
// ============================================================================

function formatFullName(prefix: string, name: string, suffix: string | null): string {
	const baseName = prefix ? `${prefix}: ${name}` : name;
	return suffix ? `${baseName} [${suffix}]` : baseName;
}

function resolveReadableIdOverride(config: ResolvedConfig): string | null {
	const envKey = config.readableIdEnv?.trim();
	if (!envKey) return null;
	const value = process.env[envKey];
	if (!value) return null;
	const trimmed = value.trim();
	return trimmed ? trimmed : null;
}

function resolveReadableIdSuffix(
	config: ResolvedConfig,
	sessionId: string | null,
	wordlist: WordlistConfig | null,
	name: string,
	ctx: ExtensionContext
): string | null {
	if (!config.readableIdSuffix) return null;
	const override = resolveReadableIdOverride(config);
	if (override) {
		return override === name ? null : override;
	}
	if (!sessionId || !wordlist) {
		debugNotify(ctx, config, "[auto-rename] readableIdSuffix enabled but wordlist or sessionId missing", "warning");
		return null;
	}
	const readableId = readableIdFromSessionId(sessionId, wordlist);
	return readableId === name ? null : readableId;
}

// ============================================================================
// Command Handlers
// ============================================================================

async function handleRegen(
	ctx: ExtensionCommandContext,
	config: ResolvedConfig,
	prefix: string,
	pi: ExtensionAPI,
	setRenamed: () => void
): Promise<void> {
	if (config.prefixOnly) {
		if (!prefix) {
			ctx.ui.notify("prefixOnly set but no prefix available", "warning");
			return;
		}
		pi.setSessionName(prefix);
		setRenamed();
		ctx.ui.notify(`Session renamed (prefix only): ${prefix}`, "info");
		return;
	}

	const query = extractFirstUserQuery(ctx);

	if (!query) {
		ctx.ui.notify("No user query found to generate name from", "warning");
		return;
	}

	ctx.ui.notify("Regenerating session name...", "info");
	const sessionId = getSessionId(ctx);
	const wordlist = loadWordlist(config, ctx.cwd);
	const result = await generateSessionName(query, config, ctx, sessionId, wordlist);

	if (result.name) {
		const suffix = resolveReadableIdSuffix(config, sessionId, wordlist, result.name, ctx);
		const fullName = formatFullName(prefix, result.name, suffix);
		pi.setSessionName(fullName);
		setRenamed();
		ctx.ui.notify(`Session renamed (${result.source}): ${fullName}`, "info");
	} else {
		ctx.ui.notify(`Failed to generate name: ${result.error || "unknown error"}`, "error");
	}
}

function handleConfig(ctx: ExtensionCommandContext, config: ResolvedConfig): void {
	const parts = [
		config.model ? `model=${config.model.provider}/${config.model.id}` : "model=auto (current session model, then cheapest)",
		config.fallbackModel ? `fallback=${config.fallbackModel.provider}/${config.fallbackModel.id}` : null,
		`deterministic=${config.fallbackDeterministic}`,
		`modelSelection=${config.modelSelection}`,
		config.wordlistPath ? `wordlistPath=${config.wordlistPath}` : null,
		config.wordlist ? "wordlist=inline" : null,
		config.readableIdEnv ? `readableIdEnv=${config.readableIdEnv}` : null,
		config.prefix ? `prefix="${config.prefix}"` : null,
		config.prefixCommand
			? `prefixCmd="${config.prefixCommand.slice(0, 30)}${config.prefixCommand.length > 30 ? "..." : ""}"`
			: null,
		config.prefixOnly ? "prefixOnly=true" : null,
		config.readableIdSuffix ? "readableIdSuffix=true" : null,
		`maxQuery=${config.maxQueryLength}`,
		`maxName=${config.maxNameLength}`,
		`enabled=${config.enabled}`,
	].filter(Boolean);

	ctx.ui.notify(`Config: ${parts.join(", ")}`, "info");
}

function handleInit(ctx: ExtensionCommandContext, cwd: string): void {
	const configPath = join(cwd, CONFIG_FILENAME);

	if (existsSync(configPath)) {
		ctx.ui.notify(`Config already exists: ${configPath}`, "warning");
		return;
	}

	writeFileSync(configPath, JSON.stringify(DEFAULT_CONFIG, null, 2));
	ctx.ui.notify(`Created config: ${configPath}`, "info");
}

async function handleTest(ctx: ExtensionCommandContext, config: ResolvedConfig): Promise<void> {
	ctx.ui.notify("Testing model connectivity...", "info");

	const resolution = await resolveModelWithFallback(config, ctx);

	if (resolution.model) {
		ctx.ui.notify(`Model OK: ${resolution.model.provider}/${resolution.model.id}`, "info");
	} else {
		ctx.ui.notify(`Model error: ${resolution.error}`, "error");
	}
}

// ============================================================================
// Extension Entry Point
// ============================================================================

export default function (pi: ExtensionAPI) {
	let sessionRenamed = false;
	let firstPromptHandled = false;

	const setRenamed = () => {
		sessionRenamed = true;
	};

	/**
	 * Set the session name and notify the Oqto runner via a status event.
	 * The runner picks up the TITLE_CHANGED_STATUS_KEY and broadcasts
	 * a canonical session.title_changed event to the frontend.
	 */
	const setNameAndNotify = (name: string, ctx: ExtensionContext) => {
		pi.setSessionName(name);
		// Keep interactive TUI status line clean; emit status only for non-TUI/rpc consumers.
		if (!ctx.hasUI) {
			ctx.ui.setStatus(TITLE_CHANGED_STATUS_KEY, name);
		}
	};

	const checkExistingName = () => {
		const existingName = pi.getSessionName();
		if (!existingName) return;
		const normalized = existingName.trim().toLowerCase();
		if (!normalized || normalized === "chat") return;
		sessionRenamed = true;
	};

	pi.on("session_start", async () => {
		sessionRenamed = false;
		firstPromptHandled = false;
		checkExistingName();
	});

	pi.on("session_tree", async () => {
		sessionRenamed = false;
		firstPromptHandled = false;
		checkExistingName();
	});

	const renameFromQuery = async (query: string, ctx: ExtensionContext): Promise<void> => {
		const config = loadConfig(ctx.cwd);
		if (!config.enabled) return;

		if (pi.getSessionName()) {
			sessionRenamed = true;
			return;
		}

		const prefix = resolvePrefix(config, ctx.cwd, ctx);
		if (config.prefixOnly) {
			if (!prefix) {
				debugNotify(ctx, config, "[auto-rename] prefixOnly set but no prefix available", "warning");
				return;
			}
			setNameAndNotify(prefix, ctx);
			sessionRenamed = true;
			debugNotify(ctx, config, `[auto-rename] Named (prefix only): ${prefix}`, "info");
			return;
		}

		const sessionId = getSessionId(ctx);
		const wordlist = loadWordlist(config, ctx.cwd);
		const result = await generateSessionName(query, config, ctx, sessionId, wordlist);

		if (!result.name) {
			debugNotify(ctx, config, `[auto-rename] Failed to generate name: ${result.error || "unknown error"}`, "warning");
			return;
		}

		const suffix = resolveReadableIdSuffix(config, sessionId, wordlist, result.name, ctx);
		const fullName = formatFullName(prefix, result.name, suffix);
		setNameAndNotify(fullName, ctx);
		sessionRenamed = true;
		debugNotify(ctx, config, `[auto-rename] Named (${result.source}): ${fullName}`, "info");
	};

	pi.on("before_agent_start", async (event, ctx) => {
		const config = loadConfig(ctx.cwd);
		debugNotify(ctx, config, `[auto-rename] before_agent_start: renamed=${sessionRenamed} handled=${firstPromptHandled}`);
		if (sessionRenamed || firstPromptHandled) return;
		firstPromptHandled = true;

		const prompt = event.prompt?.trim();
		if (!prompt) {
			debugNotify(ctx, config, "[auto-rename] before_agent_start: no prompt", "warning");
			return;
		}
		debugNotify(ctx, config, `[auto-rename] before_agent_start: triggering rename for "${prompt.slice(0, 30)}..."`);
		void renameFromQuery(prompt, ctx);
	});

	pi.on("agent_end", async (_event, ctx) => {
		if (sessionRenamed) return;

		const config = loadConfig(ctx.cwd);
		if (!config.enabled) return;

		if (pi.getSessionName()) {
			sessionRenamed = true;
			return;
		}

		const query = extractFirstUserQuery(ctx);
		if (!query) {
			debugNotify(ctx, config, "[auto-rename] No user query found", "warning");
			return;
		}
		void renameFromQuery(query, ctx);
	});

	pi.registerCommand("auto-rename", {
		description: "Auto-rename: show name, force regenerate, or set manually",
		handler: async (args: string, ctx: ExtensionCommandContext) => {
			const trimmed = args.trim();
			const config = loadConfig(ctx.cwd);

			if (trimmed === "regen" || trimmed === "regenerate") {
				const prefix = resolvePrefix(config, ctx.cwd, ctx);
				await handleRegen(ctx, config, prefix, pi, setRenamed);
			} else if (trimmed === "config") {
				handleConfig(ctx, config);
			} else if (trimmed === "init") {
				handleInit(ctx, ctx.cwd);
			} else if (trimmed === "test") {
				await handleTest(ctx, config);
			} else if (trimmed) {
				pi.setSessionName(trimmed);
				sessionRenamed = true;
				ctx.ui.notify(`Session renamed: ${trimmed}`, "info");
			} else {
				const name = pi.getSessionName();
				ctx.ui.notify(name ? `Current name: ${name}` : "No session name set", "info");
			}
		},
	});
}
