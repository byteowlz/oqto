/**
 * Error Recovery Extension
 *
 * Automatically detects and recovers from common provider errors that
 * pi's built-in retry logic does not handle (e.g. 400 Bad Request with
 * unsupported parameters, model-specific incompatibilities).
 *
 * Recovery strategies:
 * - Unsupported thinking/reasoning level: parses supported values from the
 *   error message, adjusts thinking level, and retries.
 * - Reasoning not supported at all: disables thinking and retries.
 * - Unsupported image input: strips image content and retries.
 * - Preemptive payload patching via before_provider_request to avoid
 *   known-bad parameters per model.
 *
 * Configuration (pi-error-recovery.json in cwd or ~/.pi/agent/):
 * {
 *   "enabled": true,
 *   "maxRetries": 3,
 *   "retryMessage": "continue",
 *   "debug": false,
 *   "handlers": {
 *     "thinkingLevel": true,
 *     "imageInput": true,
 *     "genericParameter": true
 *   }
 * }
 */

import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import type { AgentMessage } from "@earendil-works/pi-agent-core";
import type { ThinkingLevel } from "@earendil-works/pi-ai";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

// ============================================================================
// Local type helpers
// ============================================================================

/** Narrow AgentMessage to the assistant shape we need for error recovery.
 *  AgentMessage is a union that may include custom messages; we cast after
 *  checking role === "assistant". */
interface AssistantErrorMessage {
	role: "assistant";
	stopReason?: string;
	errorMessage?: string;
	content: unknown[];
}

function asAssistant(msg: AgentMessage): AssistantErrorMessage | undefined {
	if (msg.role !== "assistant") return undefined;
	return msg as unknown as AssistantErrorMessage;
}

// ============================================================================
// Types
// ============================================================================

interface HandlerConfig {
	thinkingLevel: boolean;
	imageInput: boolean;
	genericParameter: boolean;
}

interface RecoveryConfig {
	enabled: boolean;
	maxRetries: number;
	retryMessage: string;
	debug: boolean;
	handlers: HandlerConfig;
}

interface RecoveryAction {
	handler: string;
	fixDescription: string;
	applyFix: (pi: ExtensionAPI, ctx: ExtensionContext) => void | Promise<void>;
	shouldFilterMessage: (msg: AgentMessage) => boolean;
}

interface PendingRecovery {
	action: RecoveryAction;
	attemptCount: number;
	errorTimestamp: number;
	/** Set to true after the first context hook has applied filtering */
	consumed: boolean;
}

interface ModelRestriction {
	provider: string;
	modelId: string;
	parameter: string;
	restriction: string;
	timestamp: number;
}

// ============================================================================
// Constants
// ============================================================================

const CONFIG_FILENAME = "pi-error-recovery.json";

const DEFAULT_CONFIG: RecoveryConfig = {
	enabled: true,
	maxRetries: 3,
	retryMessage: "continue",
	debug: false,
	handlers: {
		thinkingLevel: true,
		imageInput: true,
		genericParameter: true,
	},
};

const THINKING_LEVELS: ThinkingLevel[] = ["minimal", "low", "medium", "high", "xhigh"];

// ============================================================================
// Config
// ============================================================================

function loadConfig(cwd: string): RecoveryConfig {
	const paths = [join(cwd, CONFIG_FILENAME), join(cwd, ".pi", CONFIG_FILENAME), join(homedir(), ".pi", "agent", CONFIG_FILENAME)];

	for (const path of paths) {
		if (existsSync(path)) {
			try {
				const content = readFileSync(path, "utf-8");
				const userConfig = JSON.parse(content) as Partial<RecoveryConfig>;
				return {
					...DEFAULT_CONFIG,
					...userConfig,
					handlers: { ...DEFAULT_CONFIG.handlers, ...userConfig.handlers },
				};
			} catch {
				// Invalid JSON, continue
			}
		}
	}

	return DEFAULT_CONFIG;
}

// ============================================================================
// Error Detection Patterns
// ============================================================================

/**
 * Detect unsupported thinking/reasoning level errors.
 *
 * Matches errors like:
 * - "Unsupported value: 'minimal' is not supported with the 'gpt-5.3-codex' model. Supported values are: 'none', 'low', 'medium', 'high', 'xhigh'."
 * - "Invalid 'reasoning_effort': 'xhigh' is not supported ..."
 * - "reasoning_effort' : 'minimal' is not supported ..."
 */
function detectThinkingLevelError(errorMessage: string): { current: string; supported: string[] } | null {
	// Extract the unsupported value and supported values list
	const unsupportedMatch = errorMessage.match(/['"`](\w+)['"`].*?is not supported/i);
	if (!unsupportedMatch) return null;

	const supportedMatch = errorMessage.match(/supported values are:([^\n]+)/i);
	if (!supportedMatch) return null;

	const current = (unsupportedMatch[1] ?? "").toLowerCase();
	const supportedRaw = supportedMatch[1] ?? "";
	const supported = supportedRaw
		.split(/[,\s]+/)
		.map((s) =>
			s
				.trim()
				.replace(/^['"`]+|['"`]+$/g, "")
				.toLowerCase()
		)
		.filter((s) => s.length > 0 && s !== "and");

	if (supported.length === 0) return null;

	// Accept explicit reasoning/thinking errors
	const isReasoningError = /reasoning|thinking|reasoning_effort|effort/i.test(errorMessage);
	if (isReasoningError) return { current, supported };

	// Also accept generic "unsupported value" errors when the value space clearly
	// matches reasoning/thinking levels (e.g. none/low/medium/high/xhigh).
	const known = new Set<string>(["none", "off", ...THINKING_LEVELS]);
	const looksLikeThinkingLevels = known.has(current) || supported.some((s) => known.has(s));
	if (looksLikeThinkingLevels) return { current, supported };

	return null;
}

/**
 * Detect when reasoning/thinking is not supported at all by the model.
 */
function detectReasoningNotSupported(errorMessage: string): boolean {
	const patterns = [
		/this model does not support reasoning/i,
		/reasoning.*not supported/i,
		/thinking.*not supported/i,
		/reasoning_effort.*not supported/i,
		/does not support (?:the )?reasoning/i,
		/unsupported parameter.*reasoning/i,
	];
	return patterns.some((p) => p.test(errorMessage));
}

/**
 * Detect when image input is not supported.
 */
function detectImageNotSupported(errorMessage: string): boolean {
	const patterns = [
		/image input is not supported/i,
		/does not support image/i,
		/vision.*not supported/i,
		/multimodal.*not supported/i,
		/unsupported image content/i,
		/prompt contains unsupported image content/i,
	];
	return patterns.some((p) => p.test(errorMessage));
}

// ============================================================================
// Recovery Logic
// ============================================================================

/**
 * Map a thinking level to the closest supported alternative.
 *
 * Supported values from the provider may include "none" (meaning off).
 * We map "none" to our "off" level internally.
 */
function mapToSupportedThinkingLevel(current: string, supported: string[]): ThinkingLevel | "off" {
	// Normalize supported values to our level names
	const normalizedSupported = supported.map((s) => {
		if (s === "none" || s === "off" || s === "false" || s === "disabled") return "off";
		const level = s.toLowerCase();
		if (THINKING_LEVELS.includes(level as ThinkingLevel)) return level as ThinkingLevel | "off";
		return null;
	});

	const validSupported = normalizedSupported.filter((s): s is ThinkingLevel | "off" => s !== null);
	if (validSupported.length === 0) return "off";

	// If current is off/none and not supported, stay off
	if (current === "off" || current === "none") {
		return validSupported.includes("off") ? "off" : (validSupported[0] ?? "off");
	}

	const currentIndex = THINKING_LEVELS.indexOf(current as ThinkingLevel);

	// If current level is supported, keep it (shouldn't happen, but defensive)
	if (validSupported.includes(current as ThinkingLevel)) {
		return current as ThinkingLevel;
	}

	// Find the closest lower supported level
	if (currentIndex >= 0) {
		for (let i = currentIndex - 1; i >= 0; i--) {
			const level = THINKING_LEVELS[i];
			if (validSupported.includes(level)) return level;
		}
	}

	// Find the closest higher supported level
	if (currentIndex >= 0) {
		for (let i = currentIndex + 1; i < THINKING_LEVELS.length; i++) {
			const level = THINKING_LEVELS[i];
			if (validSupported.includes(level)) return level;
		}
	}

	// Fallback: if "off" is supported, use it; otherwise use the lowest available
	return validSupported.includes("off") ? "off" : (validSupported[0] ?? "off");
}

/**
 * Build a recovery action for thinking level errors.
 */
function buildThinkingLevelRecovery(errorMessage: string): RecoveryAction | null {
	// Case 1: specific level not supported, but reasoning is
	const levelError = detectThinkingLevelError(errorMessage);
	if (levelError) {
		const fallback = mapToSupportedThinkingLevel(levelError.current, levelError.supported);
		return {
			handler: "thinkingLevel",
			fixDescription: `thinking level '${levelError.current}' → '${fallback}'`,
			applyFix: (pi: ExtensionAPI) => {
				if (fallback === "off") {
					pi.setThinkingLevel("off");
				} else {
					pi.setThinkingLevel(fallback as ThinkingLevel);
				}
			},
			shouldFilterMessage: (msg) => {
				const a = asAssistant(msg);
				return a !== undefined && a.stopReason === "error" && a.errorMessage?.includes(levelError.current) === true;
			},
		};
	}

	// Case 2: reasoning not supported at all
	if (detectReasoningNotSupported(errorMessage)) {
		return {
			handler: "thinkingLevel",
			fixDescription: "thinking level → 'off' (reasoning not supported)",
			applyFix: (pi: ExtensionAPI) => {
				pi.setThinkingLevel("off");
			},
			shouldFilterMessage: (msg) => {
				const a = asAssistant(msg);
				return a !== undefined && a.stopReason === "error";
			},
		};
	}

	return null;
}

/**
 * Build a recovery action for unsupported image input.
 *
 * Note: actual image stripping happens in the context hook because
 * we cannot modify the raw payload easily from the error handler.
 */
function buildImageInputRecovery(_errorMessage: string): RecoveryAction | null {
	return {
		handler: "imageInput",
		fixDescription: "strip image content and retry",
		applyFix: () => {
			// No global state to change; filtering happens in context hook
		},
		shouldFilterMessage: (msg) => {
			const a = asAssistant(msg);
			return a !== undefined && a.stopReason === "error";
		},
	};
}

/**
 * Analyze an error message and return a recovery action if recoverable.
 */
function analyzeError(errorMessage: string, config: HandlerConfig): RecoveryAction | null {
	if (config.thinkingLevel) {
		const recovery = buildThinkingLevelRecovery(errorMessage);
		if (recovery) return recovery;
	}

	if (config.imageInput) {
		if (detectImageNotSupported(errorMessage)) {
			return buildImageInputRecovery(errorMessage);
		}
	}

	return null;
}

// ============================================================================
// Preemptive Payload Patching
// ============================================================================

/**
 * Learn from errors and preemptively patch payloads on future requests.
 *
 * This is a lightweight cache of model restrictions learned at runtime.
 */
function createModelRestrictionCache() {
	const restrictions: ModelRestriction[] = [];

	return {
		add(provider: string, modelId: string, parameter: string, restriction: string) {
			// Remove old entries for same model+parameter
			for (let i = restrictions.length - 1; i >= 0; i--) {
				if (
					restrictions[i].provider === provider &&
					restrictions[i].modelId === modelId &&
					restrictions[i].parameter === parameter
				) {
					restrictions.splice(i, 1);
				}
			}
			restrictions.push({ provider, modelId, parameter, restriction, timestamp: Date.now() });
		},
		get(provider: string, modelId: string, parameter: string): string | undefined {
			return restrictions.find((r) => r.provider === provider && r.modelId === modelId && r.parameter === parameter)?.restriction;
		},
		dump(): ModelRestriction[] {
			return [...restrictions];
		},
		clear() {
			restrictions.length = 0;
		},
	};
}

// ============================================================================
// Session Analysis
// ============================================================================

function findLastAssistant(messages: AgentMessage[]): AgentMessage | null {
	for (let i = messages.length - 1; i >= 0; i--) {
		if (messages[i].role === "assistant") return messages[i];
	}
	return null;
}

function isErrorMessage(msg: AgentMessage): boolean {
	const a = asAssistant(msg);
	return a !== undefined && a.stopReason === "error" && !!a.errorMessage;
}

// ============================================================================
// Notifications
// ============================================================================

function notifyRecovery(ctx: ExtensionContext, action: RecoveryAction, attempt: number, maxRetries: number): void {
	if (!ctx.hasUI) return;
	ctx.ui.notify(`[error-recovery] Auto-fixing: ${action.fixDescription} (attempt ${attempt}/${maxRetries})`, "warning");
}

function notifyRetry(ctx: ExtensionContext, config: RecoveryConfig): void {
	if (!ctx.hasUI) return;
	ctx.ui.notify(`[error-recovery] Retrying with '${config.retryMessage}'...`, "info");
}

function notifyDebug(ctx: ExtensionContext, config: RecoveryConfig, info: string): void {
	if (!config.debug || !ctx.hasUI) return;
	ctx.ui.notify(`[error-recovery] ${info}`, "info");
}

function notifyGiveUp(ctx: ExtensionContext, errorMessage: string, maxRetries: number): void {
	if (!ctx.hasUI) return;
	ctx.ui.notify(
		`[error-recovery] Gave up after ${maxRetries} recovery attempts. Last error: ${errorMessage.slice(0, 80)}`,
		"error"
	);
}

// ============================================================================
// Extension
// ============================================================================

export default function (pi: ExtensionAPI) {
	let pendingRecovery: PendingRecovery | null = null;
	let lastRecoveryTimestamp = 0;
	const modelCache = createModelRestrictionCache();

	const resetState = () => {
		pendingRecovery = null;
		lastRecoveryTimestamp = 0;
		modelCache.clear();
	};

	pi.on("session_start", async () => {
		resetState();
	});

	pi.on("session_tree", async () => {
		resetState();
	});

	// Reset recovery counter when user sends a genuine new message (not our retry)
	pi.on("input", async () => {
		const now = Date.now();
		if (now - lastRecoveryTimestamp < 2000) return;
		pendingRecovery = null;
	});

	// Preemptive payload patching via before_provider_request
	pi.on("before_provider_request", (_event, ctx) => {
		const config = loadConfig(ctx.cwd);
		if (!config.enabled) return;

		const model = ctx.model;
		if (!model) return;

		// Patch reasoning parameter if we know this model has restrictions
		if (config.handlers.thinkingLevel) {
			const restriction = modelCache.get(model.provider, model.id, "thinkingLevel");
			if (restriction) {
				// restriction is something like "off" or a max level
				// We can't easily patch the raw payload here without knowing its shape,
				// but we can ensure the thinking level is set correctly before the request.
				const currentLevel = pi.getThinkingLevel();
				if (restriction === "off" && currentLevel !== "off") {
					pi.setThinkingLevel("off");
					notifyDebug(ctx, config, `Preemptively disabled thinking for ${model.provider}/${model.id}`);
				}
			}
		}

		return undefined; // Keep payload unchanged
	});

	// Filter out recovered error messages before LLM calls
	pi.on("context", async (event, ctx) => {
		const config = loadConfig(ctx.cwd);
		if (!config.enabled || !pendingRecovery || pendingRecovery.consumed) return;

		// Only filter for image input recovery (others are handled by parameter change)
		if (pendingRecovery.action.handler !== "imageInput") return;

		notifyDebug(ctx, config, "Filtering image content from context for retry");

		const filtered = event.messages.map((msg) => {
			if (msg.role !== "user" || typeof msg.content === "string") return msg;
			const content = msg.content.filter((block) => block.type !== "image");
			return { ...msg, content };
		});

		pendingRecovery.consumed = true;
		return { messages: filtered };
	});

	// Also filter out the error assistant message itself on retry
	pi.on("context", async (event, ctx) => {
		if (!pendingRecovery || pendingRecovery.consumed) return;

		// Find and remove the error message that triggered recovery
		const filterFn = pendingRecovery.action.shouldFilterMessage;
		const filtered = event.messages.filter((msg) => {
			if (msg.role !== "assistant") return true;
			return !filterFn(msg);
		});

		if (filtered.length !== event.messages.length) {
			const config = loadConfig(ctx.cwd);
			notifyDebug(ctx, config, "Filtered error assistant message from context");
		}

		pendingRecovery.consumed = true;
		return { messages: filtered };
	});

	// Main error detection and recovery on agent_end
	pi.on("agent_end", async (event, ctx) => {
		const config = loadConfig(ctx.cwd);
		if (!config.enabled) return;

		const messages = event.messages as AgentMessage[];
		if (!messages || messages.length === 0) return;

		const lastAssistant = findLastAssistant(messages);
		if (!lastAssistant || !isErrorMessage(lastAssistant)) {
			// Success — clear any stale recovery state
			pendingRecovery = null;
			return;
		}

		const a = asAssistant(lastAssistant);
		const errorMessage = a?.errorMessage ?? "";
		const model = ctx.model;

		notifyDebug(ctx, config, `Detected error: ${errorMessage.slice(0, 120)}`);

		// Check if this is the same ongoing recovery attempt
		if (pendingRecovery && pendingRecovery.errorTimestamp > Date.now() - 5000) {
			pendingRecovery.attemptCount++;
		} else {
			pendingRecovery = null;
		}

		// Analyze the error
		const recovery = analyzeError(errorMessage, config.handlers);
		if (!recovery) {
			notifyDebug(ctx, config, "No recovery handler matched this error");
			return;
		}

		// Initialize or update pending recovery
		if (!pendingRecovery) {
			pendingRecovery = {
				action: recovery,
				attemptCount: 1,
				errorTimestamp: Date.now(),
				consumed: false,
			};
		}

		if (pendingRecovery.attemptCount > config.maxRetries) {
			notifyGiveUp(ctx, errorMessage, config.maxRetries);
			pendingRecovery = null;
			return;
		}

		// Apply the fix
		notifyRecovery(ctx, recovery, pendingRecovery.attemptCount, config.maxRetries);
		await recovery.applyFix(pi, ctx);

		// Learn from this error for preemptive patching
		if (model) {
			if (recovery.handler === "thinkingLevel") {
				const levelError = detectThinkingLevelError(errorMessage);
				if (levelError) {
					const fallback = mapToSupportedThinkingLevel(levelError.current, levelError.supported);
					modelCache.add(model.provider, model.id, "thinkingLevel", fallback);
				} else if (detectReasoningNotSupported(errorMessage)) {
					modelCache.add(model.provider, model.id, "thinkingLevel", "off");
				}
			}
		}

		// Trigger retry
		notifyRetry(ctx, config);
		lastRecoveryTimestamp = Date.now();
		pi.sendUserMessage(config.retryMessage, { deliverAs: "followUp" });
	});

	// Keep TUI status line clean; no persistent status badge.
	pi.on("session_start", async (_event, ctx) => {
		const config = loadConfig(ctx.cwd);
		if (ctx.hasUI && config.enabled) {
			ctx.ui.setStatus("error-recovery", undefined);
		}
	});

	// Manual control command
	pi.registerCommand("error-recovery", {
		description: "Error recovery: status, reset, or debug",
		handler: async (args, ctx) => {
			const trimmed = args.trim();
			const config = loadConfig(ctx.cwd);

			if (trimmed === "status" || !trimmed) {
				const handlers = Object.entries(config.handlers)
					.filter(([, v]) => v)
					.map(([k]) => k)
					.join(", ");
				const pending = pendingRecovery ? `${pendingRecovery.action.handler}#${pendingRecovery.attemptCount}` : "none";
				const cache = modelCache.dump();
				ctx.ui.notify(
					`[error-recovery] enabled=${config.enabled} maxRetries=${config.maxRetries} pending=${pending} handlers=${handlers} learnedRestrictions=${cache.length}`,
					"info"
				);
			} else if (trimmed === "reset") {
				pendingRecovery = null;
				modelCache.clear();
				ctx.ui.notify("[error-recovery] Recovery state and model cache cleared.", "info");
			} else if (trimmed === "debug") {
				const cache = modelCache.dump();
				if (cache.length === 0) {
					ctx.ui.notify("[error-recovery] No learned restrictions.", "info");
				} else {
					for (const r of cache) {
						ctx.ui.notify(`[error-recovery] ${r.provider}/${r.modelId}: ${r.parameter}=${r.restriction}`, "info");
					}
				}
			} else {
				ctx.ui.notify("[error-recovery] Usage: /error-recovery [status|reset|debug]", "info");
			}
		},
	});
}
