/**
 * Message normalization and conversion utilities for chat hooks.
 *
 * All output uses canonical Part types (from canonical-types.ts) extended
 * with display-only variants (CompactionPart, ErrorPart). The normalizers
 * accept messages from any source (hstry, Pi JSONL, canonical protocol)
 * and produce a uniform DisplayMessage[] for rendering.
 */

import type { PiAgentMessage, PiSessionMessage } from "@/lib/api/default-chat";
import type { Part, ToolStatus } from "@/lib/canonical-types";
import type { DisplayMessage, DisplayPart, RawMessage } from "./types";

const MESSAGE_ID_PATTERN = /^pi-msg-(\d+)$/;


// ============================================================================
// Internal helpers
// ============================================================================

let partIdCounter = 0;
/** Generate a unique part ID for display parts. */
export function nextPartId(): string {
	return `dp-${++partIdCounter}`;
}

function parseJsonMaybe(value: string): unknown | null {
	const trimmed = value.trim();
	if (!trimmed) return null;
	if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return null;
	try {
		return JSON.parse(trimmed) as unknown;
	} catch {
		return null;
	}
}

/** Safely stringify a value for fingerprinting. */
function safeStringify(value: unknown): string {
	if (value === null || value === undefined) return "";
	if (typeof value === "string") return value;
	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}

// ============================================================================
// Coercion: untyped objects → canonical Part
// ============================================================================

/** Try to interpret a raw value as a tool_result Part. */
function coerceToolResult(
	value: unknown,
): (Part & { type: "tool_result" }) | null {
	if (!value || typeof value !== "object") return null;
	const obj = value as Record<string, unknown>;
	const type = typeof obj.type === "string" ? obj.type : "";
	const looksLikeToolResult =
		type === "tool_result" ||
		type === "toolResult" ||
		"toolCallId" in obj ||
		"tool_use_id" in obj ||
		"toolName" in obj;
	if (!looksLikeToolResult) return null;

	return {
		type: "tool_result",
		id: (typeof obj.id === "string" && obj.id) || nextPartId(),
		toolCallId:
			(typeof obj.tool_use_id === "string" && obj.tool_use_id) ||
			(typeof obj.toolCallId === "string" && obj.toolCallId) ||
			(typeof obj.tool_call_id === "string" && obj.tool_call_id) ||
			"",
		name:
			(typeof obj.name === "string" && obj.name) ||
			(typeof obj.toolName === "string" && obj.toolName) ||
			undefined,
		output:
			"output" in obj
				? obj.output
				: "content" in obj
					? obj.content
					: typeof obj.text === "string"
						? obj.text
						: obj,
		isError: Boolean(obj.is_error ?? obj.isError),
	};
}

/**
 * Normalize a raw block (from parts_json, Pi content array, or canonical Part)
 * into a canonical DisplayPart. Returns null if unrecognized.
 */
function coerceBlockToPart(b: Record<string, unknown>): DisplayPart | null {
	const blockType = typeof b.type === "string" ? b.type : "";

	// --- text ---
	if (blockType === "text") {
		const text =
			typeof b.text === "string"
				? b.text
				: typeof b.content === "string"
					? b.content
					: "";
		if (text) {
			return {
				type: "text",
				id: (typeof b.id === "string" && b.id) || nextPartId(),
				text,
			};
		}
		return null;
	}

	// --- thinking ---
	if (blockType === "thinking") {
		const text =
			typeof b.thinking === "string"
				? b.thinking
				: typeof b.text === "string"
					? b.text
					: typeof b.content === "string"
						? b.content
						: "";
		if (text.trim()) {
			return {
				type: "thinking",
				id: (typeof b.id === "string" && b.id) || nextPartId(),
				text,
			};
		}
		return null;
	}

	// --- tool_call / tool_use / toolCall ---
	if (
		blockType === "tool_call" ||
		blockType === "tool_use" ||
		blockType === "toolCall"
	) {
		return {
			type: "tool_call",
			id: (typeof b.id === "string" && b.id) || nextPartId(),
			toolCallId:
				(typeof b.toolCallId === "string" && b.toolCallId) ||
				(typeof b.tool_call_id === "string" && b.tool_call_id) ||
				(typeof b.id === "string" && b.id) ||
				"",
			name: (typeof b.name === "string" && b.name) || "unknown",
			input:
				typeof b.arguments === "object" && b.arguments !== null
					? b.arguments
					: b.input,
			status: (typeof b.status === "string"
				? b.status
				: "success") as ToolStatus,
		};
	}

	// --- tool_result / toolResult ---
	if (blockType === "tool_result" || blockType === "toolResult") {
		return {
			type: "tool_result",
			id: (typeof b.id === "string" && b.id) || nextPartId(),
			toolCallId:
				(typeof b.tool_use_id === "string" && b.tool_use_id) ||
				(typeof b.toolCallId === "string" && b.toolCallId) ||
				(typeof b.tool_call_id === "string" && b.tool_call_id) ||
				"",
			name:
				(typeof b.name === "string" && b.name) ||
				(typeof b.toolName === "string" && b.toolName) ||
				undefined,
			output:
				"output" in b
					? b.output
					: "content" in b
						? b.content
						: typeof b.text === "string"
							? b.text
							: b,
			isError: Boolean(b.is_error ?? b.isError),
		};
	}

	// --- image ---
	if (blockType === "image") {
		const id = (typeof b.id === "string" && b.id) || nextPartId();
		// Canonical images use a MediaSource tagged union; legacy images have
		// source/data/url/mimeType fields.
		const source = b.source as string | Record<string, unknown> | undefined;
		if (typeof source === "string") {
			// Already canonical-ish (source is a discriminator like "base64", "url")
			return {
				type: "image",
				id,
				source: source as "base64",
				data: typeof b.data === "string" ? b.data : "",
				mimeType: typeof b.mimeType === "string" ? b.mimeType : "image/png",
			};
		}
		if (source && typeof source === "object") {
			const srcType = typeof source.type === "string" ? source.type : "base64";
			if (srcType === "url" && typeof source.url === "string") {
				return {
					type: "image",
					id,
					source: "url",
					url: source.url,
					mimeType:
						(typeof source.media_type === "string" && source.media_type) ||
						(typeof source.mediaType === "string" && source.mediaType) ||
						undefined,
				};
			}
			return {
				type: "image",
				id,
				source: "base64",
				data: typeof source.data === "string" ? source.data : "",
				mimeType:
					(typeof source.media_type === "string" && source.media_type) ||
					(typeof source.mediaType === "string" && source.mediaType) ||
					"image/png",
			};
		}
		return null;
	}

	// --- file_ref ---
	if (blockType === "file_ref" && typeof b.uri === "string") {
		return {
			type: "file_ref",
			id: (typeof b.id === "string" && b.id) || nextPartId(),
			uri: b.uri,
			label: typeof b.label === "string" ? b.label : undefined,
		};
	}

	return null;
}

// ============================================================================
// Content normalization (any source → DisplayPart[])
// ============================================================================

/** Normalize mixed content (string, array of blocks, single object) into DisplayPart[]. */
export function normalizeContentToParts(content: unknown): DisplayPart[] {
	const parts: DisplayPart[] = [];

	if (typeof content === "string") {
		const parsed = parseJsonMaybe(content);
		// If the JSON parsed to an array, normalize it recursively — some sources
		// (e.g. Pi MCP bridge) encode content as a stringified JSON array of blocks.
		// Without this, the raw JSON string would be rendered as a text part.
		if (Array.isArray(parsed)) {
			const arrayParts = normalizeContentToParts(parsed);
			if (arrayParts.length > 0) {
				parts.push(...arrayParts);
				return parts;
			}
		}
		const toolResult = parsed ? coerceToolResult(parsed) : null;
		if (toolResult) {
			parts.push(toolResult);
			return parts;
		}
		parts.push({ type: "text", id: nextPartId(), text: content });
		return parts;
	}

	if (Array.isArray(content)) {
		for (const block of content) {
			if (typeof block === "string") {
				parts.push({ type: "text", id: nextPartId(), text: block });
				continue;
			}
			if (!block || typeof block !== "object") continue;
			const part = coerceBlockToPart(block as Record<string, unknown>);
			if (part) parts.push(part);
		}
		return parts;
	}

	if (content && typeof content === "object") {
		const b = content as Record<string, unknown>;
		const part = coerceBlockToPart(b);
		if (part) {
			parts.push(part);
		} else {
			const toolResult = coerceToolResult(b);
			if (toolResult) parts.push(toolResult);
		}
	}

	return parts;
}

// ============================================================================
// Message-level normalization
// ============================================================================

/** Convert a canonical Message (oqto-protocol) into a DisplayMessage. */
export function convertCanonicalMessageToDisplay(
	message: unknown,
	fallbackId: string,
): DisplayMessage | null {
	if (!message || typeof message !== "object") return null;
	const msg = message as Record<string, unknown>;
	const roleValue = typeof msg.role === "string" ? msg.role : "assistant";
	const role =
		roleValue === "user" || roleValue === "assistant" || roleValue === "system"
			? roleValue
			: "assistant";
	const parts = normalizeContentToParts(
		Array.isArray(msg.parts) && msg.parts.length > 0 ? msg.parts : msg.content,
	);
	const timestamp =
		typeof msg.created_at === "number" && msg.created_at > 0
			? msg.created_at
			: Date.now();
	const usage = msg.usage as DisplayMessage["usage"] | undefined;

	return {
		id: typeof msg.id === "string" && msg.id ? msg.id : fallbackId,
		role,
		parts,
		timestamp,
		usage,
	};
}

/**
 * Normalize raw messages (from any source) into DisplayMessage[].
 *
 * Handles hstry SerializableMessage (with parts_json), Pi JSONL messages,
 * and canonical Messages. Merges tool_result parts back into the assistant
 * message that contains the matching tool_call.
 */
export function normalizeMessages(
	messages: RawMessage[],
	idPrefix: string,
): DisplayMessage[] {
	const display: DisplayMessage[] = [];
	const toolCallIndexById = new Map<string, number>();
	const pendingToolCallByName = new Map<string, number[]>();
	// Track which tool results have been added to each message to prevent duplicates
	// Key is message index, value is Set of toolCallIds
	const toolResultsByMessageIndex = new Map<number, Set<string>>();

	const addPendingByName = (name: string, index: number) => {
		const list = pendingToolCallByName.get(name) ?? [];
		list.push(index);
		pendingToolCallByName.set(name, list);
	};

	const resolvePendingByName = (
		name: string | undefined,
	): number | undefined => {
		if (!name) return undefined;
		const list = pendingToolCallByName.get(name);
		if (!list || list.length === 0) return undefined;
		return list[list.length - 1];
	};

	for (const [idx, message] of messages.entries()) {
		const role = message.role;
		const timestamp =
			message.timestamp ??
			message.created_at ??
			message.created_at_ms ??
			message.createdAtMs ??
			Date.now();
		const partsJson =
			typeof message.parts_json === "string"
				? message.parts_json
				: typeof message.partsJson === "string"
					? message.partsJson
					: null;
		const parsedParts = partsJson
			? (parseJsonMaybe(partsJson) as unknown[] | null)
			: null;
		const canonicalParts =
			Array.isArray(message.parts) && message.parts.length > 0
				? message.parts
				: null;
		const content =
			parsedParts !== null
				? parsedParts
				: canonicalParts !== null
					? canonicalParts
					: message.content;

		// --- Tool result messages (role=tool/toolResult) ---
		if (role === "toolResult" || role === "tool") {
			// For hstry messages, toolCallId lives inside parts_json, not on the message.
			let resolvedToolCallId = message.toolCallId || message.tool_call_id || "";
			let resolvedToolName = message.toolName ?? message.tool_name;
			let resolvedOutput: unknown = content;
			let resolvedIsError = message.isError ?? message.is_error;

			if (
				!resolvedToolCallId &&
				Array.isArray(parsedParts) &&
				parsedParts.length > 0
			) {
				const firstResult = (parsedParts as Record<string, unknown>[]).find(
					(p) => p.type === "tool_result",
				);
				if (firstResult) {
					resolvedToolCallId =
						(firstResult.toolCallId as string) ||
						(firstResult.tool_call_id as string) ||
						(firstResult.id as string) ||
						"";
					resolvedToolName =
						resolvedToolName || (firstResult.name as string | undefined);
					resolvedOutput = firstResult.output ?? firstResult.content ?? content;
					resolvedIsError =
						resolvedIsError ??
						(firstResult.is_error as boolean | undefined) ??
						(firstResult.isError as boolean | undefined);
				}
			}

			const toolCallId =
				resolvedToolCallId || message.id || `tool-result-${idx}`;
			const toolResultPart: DisplayPart = {
				type: "tool_result",
				id: nextPartId(),
				toolCallId,
				name: resolvedToolName,
				output: resolvedOutput,
				isError: Boolean(resolvedIsError),
			};

			const targetIndex = resolvedToolCallId
				? toolCallIndexById.get(toolCallId)
				: resolvePendingByName(resolvedToolName);

			if (targetIndex !== undefined) {
				// Check if this tool result has already been added to this message
				const existingResults =
					toolResultsByMessageIndex.get(targetIndex) || new Set<string>();
				if (!existingResults.has(toolCallId)) {
					display[targetIndex].parts.push(toolResultPart);
					existingResults.add(toolCallId);
					toolResultsByMessageIndex.set(targetIndex, existingResults);
				}
			} else {
				display.push({
					id: `${idPrefix}-${idx}`,
					role: "assistant",
					parts: [toolResultPart],
					timestamp,
				});
			}
			continue;
		}

		// --- Regular messages (user, assistant, system) ---
		const normalizedRole =
			role === "user" || role === "assistant" || role === "system"
				? role
				: "assistant";
		const parts = normalizeContentToParts(content);
		const clientId = message.client_id ?? message.clientId;
		const msgModel = message.model_id ?? message.model ?? null;
		const msgProvider = message.provider_id ?? message.provider ?? null;
		const displayMessage: DisplayMessage = {
			id: `${idPrefix}-${idx}`,
			role: normalizedRole,
			parts,
			timestamp,
			usage: message.usage,
			clientId,
			model: msgModel,
			provider: msgProvider,
		};

		display.push(displayMessage);

		if (normalizedRole === "assistant") {
			for (const part of parts) {
				if (part.type === "tool_call" && part.toolCallId) {
					toolCallIndexById.set(part.toolCallId, display.length - 1);
					addPendingByName(part.name, display.length - 1);
				}
				if (part.type === "tool_result") {
					const tcId = part.toolCallId;
					const indexById = tcId ? toolCallIndexById.get(tcId) : undefined;
					const indexByName = resolvePendingByName(part.name);
					const targetIndex = indexById ?? indexByName;
					if (targetIndex !== undefined && targetIndex !== display.length - 1) {
						// Check if this tool result has already been added to target message
						const existingResults =
							toolResultsByMessageIndex.get(targetIndex) || new Set<string>();
						if (!existingResults.has(tcId)) {
							display[targetIndex].parts.push(part);
							existingResults.add(tcId);
							toolResultsByMessageIndex.set(targetIndex, existingResults);
						}
					}
				}
			}
		}
	}

	return display;
}

// ============================================================================
// Backward-compatible aliases (used by existing callers)
// ============================================================================

/** @deprecated Use normalizeMessages */
export const normalizePiMessages = normalizeMessages;

/** @deprecated Use normalizeContentToParts */
export const normalizePiContentToParts = normalizeContentToParts;

// ============================================================================
// Converter functions for specific message sources
// ============================================================================

/** Convert Pi agent messages to display messages. */
export function convertToDisplayMessages(
	agentMessages: PiAgentMessage[],
): DisplayMessage[] {
	const rawMessages: RawMessage[] = agentMessages.map((msg) => ({
		role: msg.role,
		content: msg.content,
		timestamp: msg.timestamp,
		usage: msg.usage,
	}));
	return normalizeMessages(rawMessages, "hist");
}

/** Convert session messages to display messages. */
export function convertSessionMessagesToDisplay(
	sessionMessages: PiSessionMessage[],
): DisplayMessage[] {
	const rawMessages: RawMessage[] = sessionMessages.map((msg) => ({
		id: msg.id,
		role: msg.role,
		content: msg.content,
		timestamp: msg.timestamp || Date.now(),
		usage: msg.usage as DisplayMessage["usage"],
		toolCallId: msg.toolCallId,
		toolName: msg.toolName,
		isError: msg.isError,
	}));
	return normalizeMessages(rawMessages, "session");
}

// ============================================================================
// Message ID utilities
// ============================================================================

/** Get the maximum message ID number from a list of messages. */
export function getMaxMessageId(messages: DisplayMessage[]): number {
	let maxId = 0;
	for (const message of messages) {
		const match = MESSAGE_ID_PATTERN.exec(message.id);
		if (!match) continue;
		const value = Number.parseInt(match[1] ?? "0", 10);
		if (!Number.isNaN(value) && value > maxId) {
			maxId = value;
		}
	}
	return maxId;
}

/** @deprecated Use getMaxMessageId */
export const getMaxPiMessageId = getMaxMessageId;

/** Check if a message should be preserved during server refresh. */
export function shouldPreserveLocalMessage(message: DisplayMessage): boolean {
	if (MESSAGE_ID_PATTERN.test(message.id)) return true;
	if (message.id.startsWith("compaction-")) return true;
	return false;
}

// ============================================================================
// Fingerprinting and merge
// ============================================================================

/** Create a fingerprint for a message (used for deduplication). */
export function messageFingerprint(message: DisplayMessage): string {
	const parts = message.parts.map((part) => {
		switch (part.type) {
			case "text":
				return `text:${part.text}`;
			case "thinking":
				return `thinking:${part.text}`;
			case "tool_call":
				return `tool_call:${part.name}:${safeStringify(part.input)}`;
			case "tool_result":
				return `tool_result:${part.name ?? ""}:${safeStringify(part.output)}:${
					part.isError ? "1" : "0"
				}`;
			case "compaction":
				return "compaction";
			default:
				return part.type;
		}
	});
	return `${message.role}|${parts.join("|")}`;
}

function messageTextSignature(message: DisplayMessage): string {
	return message.parts
		.flatMap((p) => (p.type === "text" ? [p.text] : []))
		.join("")
		.trim();
}

/**
 * Merge server messages with local messages, preserving in-flight optimistic updates.
 *
 * Matching strategy (in order of priority):
 * 1. client_id match: If a server message has a client_id that matches a local
 *    optimistic message, they represent the same message.
 * 2. id match: Direct ID match means the same message.
 * 3. content/fingerprint match: Same text content around the same time = same message.
 */
/**
 * Merge mode declares the caller's intent explicitly, rather than
 * guessing from message counts.
 *
 * - "authoritative": server has the complete history (e.g., hstry).
 *   Replaces local state entirely, preserving only trailing in-flight
 *   messages (streaming + optimistic sends).
 *
 * - "partial": server has a subset (e.g., Pi's context window).
 *   Keeps all local messages and updates/appends from the server set.
 */
export type MergeMode = "authoritative" | "partial";

export function mergeServerMessages(
	previous: DisplayMessage[],
	serverMessages: DisplayMessage[],
	mode: MergeMode = "partial",
): DisplayMessage[] {
	if (serverMessages.length === 0) return previous;
	if (previous.length === 0) return serverMessages;

	if (mode === "partial") {
		// Keep all previous messages. Update any that match by ID or
		// clientId; append the rest.
		const result = [...previous];
		for (const serverMsg of serverMessages) {
			let matched = false;
			for (let i = 0; i < result.length; i++) {
				const local = result[i];
				if (local.id === serverMsg.id) {
					result[i] = serverMsg;
					matched = true;
					break;
				}
				if (
					local.clientId &&
					serverMsg.clientId &&
					local.clientId === serverMsg.clientId
				) {
					result[i] = serverMsg;
					matched = true;
					break;
				}
			}
			if (!matched) {
				result.push(serverMsg);
			}
		}
		return result;
	}

	// mode === "authoritative"
	// Server is the complete source of truth. Only preserve trailing
	// in-flight local messages the server doesn't know about yet.
	const serverClientIds = new Set(
		serverMessages.filter((m) => m.clientId).map((m) => m.clientId),
	);

	const trailing: DisplayMessage[] = [];
	for (let i = previous.length - 1; i >= 0; i--) {
		const msg = previous[i];
		if (msg.isStreaming) {
			trailing.unshift(msg);
			continue;
		}
		if (
			msg.role === "user" &&
			msg.clientId &&
			!serverClientIds.has(msg.clientId)
		) {
			trailing.unshift(msg);
			continue;
		}
		break;
	}

	return trailing.length > 0
		? [...serverMessages, ...trailing]
		: serverMessages;
}
