/**
 * Oqto Bridge Extension for Pi
 *
 * Emits granular agent phase status via ctx.ui.setStatus() so that the
 * Oqto runner can translate pi's native events into canonical
 * `agent.working { phase }` events.
 *
 * In RPC mode, setStatus calls become `extension_ui_request` JSON events
 * on stdout with method "setStatus", which the runner reads and maps to
 * canonical events.
 *
 * The runner uses these in combination with pi's native lifecycle events
 * (agent_start, agent_end, tool_execution_start, etc.) and PID-based
 * process monitoring to produce the full canonical event stream.
 *
 * Status key: "oqto_phase"
 * Status values:
 *   "generating"                - LLM is producing tokens
 *   "thinking"                  - LLM is in extended thinking mode
 *   "tool_running:<tool_name>"  - Tool is executing
 *   "compacting"                - Context compaction in progress
 *   undefined                   - Clear phase (runner determines state from native events)
 *
 * The runner's state machine rules:
 *   - agent_start  -> working("generating")
 *   - agent_end    -> idle
 *   - oqto_phase=X -> refine phase within working state (never transitions to idle)
 *   - oqto_phase=  -> clear phase, fall back to native event inference
 */

import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

// ============================================================================
// Status helpers
// ============================================================================

const STATUS_KEY = "oqto_phase";

/** Set the current phase. The runner reads this from stdout. */
function setPhase(ctx: ExtensionContext, phase: string, detail?: string): void {
	const value = detail ? `${phase}:${detail}` : phase;
	ctx.ui.setStatus(STATUS_KEY, value);
}

/** Clear the current phase. Runner falls back to native event inference. */
function clearPhase(ctx: ExtensionContext): void {
	ctx.ui.setStatus(STATUS_KEY, undefined);
}

// ============================================================================
// Session env helpers
// ============================================================================

/**
 * Export session identity as env vars so child processes (agntz, etc.)
 * can identify which harness, session, and model they're running under.
 *
 * Uses generic AGENT_ prefix so any harness can follow the same convention.
 *
 * Called on session_start and turn_end (name may appear after auto-rename).
 */
function exportSessionEnv(ctx: ExtensionContext): void {
	process.env.AGENT_HARNESS = "pi";
	process.env.AGENT_SESSION_ID = ctx.sessionManager.getSessionId?.() ?? "";
	process.env.AGENT_SESSION_FILE = ctx.sessionManager.getSessionFile?.() ?? "";
	process.env.AGENT_CWD = ctx.cwd;

	const name = ctx.sessionManager.getSessionName?.();
	if (name) {
		process.env.AGENT_SESSION_NAME = name;
	}

	if (ctx.model) {
		process.env.AGENT_MODEL = `${ctx.model.provider}/${ctx.model.id}`;
	}
}

// ============================================================================
// Extension entry point
// ============================================================================

type QueueIntent = "default" | "steer" | "followUp";

type QueueEntry = {
	bridgeSeq: number;
	clientId: string;
	intent: QueueIntent;
	enqueuedAt: number;
	promptPreview: string;
};

type InputMeta = {
	clientId: string;
	intent: QueueIntent;
};

const QUEUE_EVENT_KEY = "oqto_queue_event";
const META_TAG_REGEX = /\s*\[\[oqto_meta:(\{[\s\S]*\})\]\]\s*$/;

function detectRpcMode(): boolean {
	const argv = process.argv.join(" ");
	return argv.includes("--mode rpc") || argv.includes("--mode=rpc");
}

function parseInputMeta(text: string): {
	cleanText: string;
	meta?: InputMeta;
} {
	const match = text.match(META_TAG_REGEX);
	if (!match) {
		return { cleanText: text };
	}

	try {
		const raw = JSON.parse(match[1] ?? "{}") as Partial<InputMeta>;
		const clientId = typeof raw.clientId === "string" ? raw.clientId.trim() : "";
		const intent = raw.intent === "steer" || raw.intent === "followUp" ? raw.intent : "default";
		if (!clientId) {
			return { cleanText: text.replace(META_TAG_REGEX, "") };
		}
		return {
			cleanText: text.replace(META_TAG_REGEX, ""),
			meta: { clientId, intent },
		};
	} catch {
		return { cleanText: text.replace(META_TAG_REGEX, "") };
	}
}

function emitQueueEvent(ctx: ExtensionContext, eventType: string, payload: Record<string, unknown>): void {
	// Keep queue telemetry out of interactive TUI status line.
	// It is only needed for machine-consumed RPC streams.
	if (ctx.hasUI) return;
	ctx.ui.setStatus(
		QUEUE_EVENT_KEY,
		JSON.stringify({
			type: eventType,
			ts: Date.now(),
			sessionId: ctx.sessionManager.getSessionId?.() ?? "",
			...payload,
		})
	);
}

export default function oqtoBridge(pi: ExtensionAPI) {
	const rpcMode = detectRpcMode();
	let agentRunning = false;
	let queueSeq = 0;
	const pendingQueue: QueueEntry[] = [];

	pi.on("input", (event, ctx) => {
		if (!rpcMode || event.source !== "rpc") {
			return { action: "continue" as const };
		}

		const { cleanText, meta } = parseInputMeta(event.text);
		if (!meta) {
			emitQueueEvent(ctx, "rpc_input_untracked", {
				reason: "missing_or_invalid_meta",
				queueDepth: pendingQueue.length,
			});
			if (cleanText !== event.text) {
				return { action: "transform" as const, text: cleanText };
			}
			return { action: "continue" as const };
		}

		const entry: QueueEntry = {
			bridgeSeq: ++queueSeq,
			clientId: meta.clientId,
			intent: meta.intent,
			enqueuedAt: Date.now(),
			promptPreview: cleanText.slice(0, 120),
		};
		pendingQueue.push(entry);

		emitQueueEvent(ctx, "enqueued", {
			bridgeSeq: entry.bridgeSeq,
			clientId: entry.clientId,
			intent: entry.intent,
			queueDepth: pendingQueue.length,
		});

		return { action: "transform" as const, text: cleanText };
	});

	pi.on("agent_start", (_event, ctx) => {
		agentRunning = true;
		setPhase(ctx, "generating");
	});

	pi.on("agent_end", (event, ctx) => {
		agentRunning = false;
		clearPhase(ctx);

		if (!rpcMode) return;

		const userMessageCount = event.messages.filter((msg) => msg.role.toLowerCase() === "user").length;

		for (let i = 0; i < userMessageCount; i++) {
			const next = pendingQueue.shift();
			if (!next) {
				emitQueueEvent(ctx, "invariant_violation", {
					reason: "more_user_messages_than_pending_entries",
					dequeuedIndex: i,
					queueDepth: pendingQueue.length,
				});
				break;
			}

			emitQueueEvent(ctx, "dequeued", {
				bridgeSeq: next.bridgeSeq,
				clientId: next.clientId,
				intent: next.intent,
				queueDepth: pendingQueue.length,
			});

			emitQueueEvent(ctx, "turn_bound", {
				bridgeSeq: next.bridgeSeq,
				clientId: next.clientId,
				intent: next.intent,
				boundUserOrdinal: i,
				queueDepth: pendingQueue.length,
			});
		}
	});

	pi.on("tool_call", (event, ctx) => {
		if (!agentRunning) return;
		setPhase(ctx, "tool_running", event.toolName);
	});

	pi.on("tool_result", (_event, ctx) => {
		if (!agentRunning) return;
		setPhase(ctx, "generating");
	});

	pi.on("turn_start", (_event, ctx) => {
		if (!agentRunning) return;
		setPhase(ctx, "generating");
	});

	pi.on("session_before_compact", (_event, ctx) => {
		setPhase(ctx, "compacting");
	});

	pi.on("session_compact", (_event, ctx) => {
		if (agentRunning) {
			setPhase(ctx, "generating");
		} else {
			clearPhase(ctx);
		}
	});

	pi.on("session_tree", (_event, ctx) => {
		agentRunning = false;
		clearPhase(ctx);
		pendingQueue.length = 0;
		emitQueueEvent(ctx, "queue_reset", { reason: "session_tree", queueDepth: 0 });
	});

	pi.on("session_start", (_event, ctx) => {
		agentRunning = false;
		clearPhase(ctx);
		pendingQueue.length = 0;
		exportSessionEnv(ctx);
		emitQueueEvent(ctx, "queue_reset", { reason: "session_start", queueDepth: 0 });
	});

	pi.on("turn_end", (_event, ctx) => {
		exportSessionEnv(ctx);
	});
}
