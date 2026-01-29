/**
 * Message normalization and conversion utilities for Pi chat hooks.
 */

import type {
	PiAgentMessage,
	PiSessionMessage,
} from "@/lib/control-plane-client";
import type { PiDisplayMessage, PiMessagePart, RawPiMessage } from "./types";

const PI_MESSAGE_ID_PATTERN = /^pi-msg-(\d+)$/;
const MESSAGE_MATCH_WINDOW_MS = 120_000;

/** Normalize Pi content blocks to display parts */
export function normalizePiContentToParts(content: unknown): PiMessagePart[] {
	const parts: PiMessagePart[] = [];

	if (typeof content === "string") {
		parts.push({ type: "text", content });
		return parts;
	}

	if (Array.isArray(content)) {
		for (const block of content) {
			if (typeof block === "string") {
				parts.push({ type: "text", content: block });
				continue;
			}
			if (!block || typeof block !== "object") continue;
			const b = block as Record<string, unknown>;
			const blockType = typeof b.type === "string" ? b.type : "";

			if (blockType === "text" && typeof b.text === "string") {
				parts.push({ type: "text", content: b.text });
				continue;
			}
			if (blockType === "thinking") {
				const thinkingText =
					typeof b.thinking === "string"
						? b.thinking
						: typeof b.content === "string"
							? b.content
							: "";
				if (thinkingText.trim()) {
					parts.push({ type: "thinking", content: thinkingText });
				}
				continue;
			}
			if (blockType === "toolCall" || blockType === "tool_use") {
				parts.push({
					type: "tool_use",
					id: typeof b.id === "string" ? b.id : "",
					name: typeof b.name === "string" ? b.name : "unknown",
					input:
						typeof b.arguments === "object" && b.arguments !== null
							? b.arguments
							: b.input,
				});
				continue;
			}
			if (blockType === "tool_result" || blockType === "toolResult") {
				parts.push({
					type: "tool_result",
					id:
						(typeof b.tool_use_id === "string" && b.tool_use_id) ||
						(typeof b.toolCallId === "string" && b.toolCallId) ||
						(typeof b.id === "string" && b.id) ||
						"",
					name:
						(typeof b.name === "string" && b.name) ||
						(typeof b.toolName === "string" && b.toolName) ||
						undefined,
					content:
						"content" in b
							? b.content
							: typeof b.text === "string"
								? b.text
								: b,
					isError: Boolean(b.is_error ?? b.isError),
				});
			}
		}
		return parts;
	}

	if (content && typeof content === "object") {
		const b = content as Record<string, unknown>;
		if (b.type === "text" && typeof b.text === "string") {
			parts.push({ type: "text", content: b.text });
		} else if (b.type === "thinking" && typeof b.thinking === "string") {
			parts.push({ type: "thinking", content: b.thinking });
		}
	}

	return parts;
}

/** Normalize raw Pi messages to display messages */
export function normalizePiMessages(
	messages: RawPiMessage[],
	idPrefix: string,
): PiDisplayMessage[] {
	const display: PiDisplayMessage[] = [];
	const toolUseIndexById = new Map<string, number>();
	const pendingToolUseByName = new Map<string, number[]>();

	const addPendingByName = (name: string, index: number) => {
		const list = pendingToolUseByName.get(name) ?? [];
		list.push(index);
		pendingToolUseByName.set(name, list);
	};

	const resolvePendingByName = (
		name: string | undefined,
	): number | undefined => {
		if (!name) return undefined;
		const list = pendingToolUseByName.get(name);
		if (!list || list.length === 0) return undefined;
		return list[list.length - 1];
	};

	for (const [idx, message] of messages.entries()) {
		const role = message.role;
		const timestamp = message.timestamp ?? Date.now();

		if (role === "toolResult") {
			const toolCallId =
				message.toolCallId || message.id || `tool-result-${idx}`;
			const toolResultPart: PiMessagePart = {
				type: "tool_result",
				id: toolCallId,
				name: message.toolName,
				content: message.content,
				isError: message.isError,
			};

			const targetIndex = message.toolCallId
				? toolUseIndexById.get(message.toolCallId)
				: resolvePendingByName(message.toolName);

			if (targetIndex !== undefined) {
				display[targetIndex].parts.push(toolResultPart);
			} else {
				display.push({
					id: `${idPrefix}-${idx}-${message.id ?? "tool-result"}`,
					role: "assistant",
					parts: [toolResultPart],
					timestamp,
				});
			}
			continue;
		}

		const normalizedRole =
			role === "user" || role === "assistant" || role === "system"
				? role
				: "assistant";
		const parts = normalizePiContentToParts(message.content);
		const displayMessage: PiDisplayMessage = {
			id: `${idPrefix}-${idx}-${message.id ?? ""}`,
			role: normalizedRole,
			parts,
			timestamp,
			usage: message.usage,
		};

		display.push(displayMessage);

		if (normalizedRole === "assistant") {
			for (const part of parts) {
				if (part.type === "tool_use" && part.id) {
					toolUseIndexById.set(part.id, display.length - 1);
					addPendingByName(part.name, display.length - 1);
				}
				if (part.type === "tool_result") {
					const id = part.id;
					const indexById = id ? toolUseIndexById.get(id) : undefined;
					const indexByName = resolvePendingByName(part.name);
					const targetIndex = indexById ?? indexByName;
					if (targetIndex !== undefined && targetIndex !== display.length - 1) {
						display[targetIndex].parts.push(part);
					}
				}
			}
		}
	}

	return display;
}

/** Convert Pi agent messages to display messages */
export function convertToDisplayMessages(
	agentMessages: PiAgentMessage[],
): PiDisplayMessage[] {
	const rawMessages: RawPiMessage[] = agentMessages.map((msg) => ({
		role: msg.role,
		content: msg.content,
		timestamp: msg.timestamp,
		usage: msg.usage,
	}));
	return normalizePiMessages(rawMessages, "pi-hist");
}

/** Convert session messages to display messages */
export function convertSessionMessagesToDisplay(
	sessionMessages: PiSessionMessage[],
): PiDisplayMessage[] {
	const rawMessages: RawPiMessage[] = sessionMessages.map((msg) => ({
		id: msg.id,
		role: msg.role,
		content: msg.content,
		timestamp: msg.timestamp || Date.now(),
		usage: msg.usage as PiAgentMessage["usage"],
		toolCallId: msg.toolCallId,
		toolName: msg.toolName,
		isError: msg.isError,
	}));
	return normalizePiMessages(rawMessages, "pi-session");
}

/** Get the maximum message ID number from a list of messages */
export function getMaxPiMessageId(messages: PiDisplayMessage[]): number {
	let maxId = 0;
	for (const message of messages) {
		const match = PI_MESSAGE_ID_PATTERN.exec(message.id);
		if (!match) continue;
		const value = Number.parseInt(match[1] ?? "0", 10);
		if (!Number.isNaN(value) && value > maxId) {
			maxId = value;
		}
	}
	return maxId;
}

/** Check if a message should be preserved during server refresh */
export function shouldPreserveLocalMessage(message: PiDisplayMessage): boolean {
	// Local optimistic messages (not yet persisted) use pi-msg-* IDs.
	// Keep them when server refreshes history to avoid clobbering in-flight streaming.
	if (PI_MESSAGE_ID_PATTERN.test(message.id)) return true;
	if (message.id.startsWith("compaction-")) return true;
	return false;
}

/** Safely stringify a value for fingerprinting */
function safeStringify(value: unknown): string {
	if (value === null || value === undefined) return "";
	if (typeof value === "string") return value;
	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}

/** Create a fingerprint for a message (used for deduplication) */
export function messageFingerprint(message: PiDisplayMessage): string {
	const parts = message.parts.map((part) => {
		switch (part.type) {
			case "text":
				return `text:${part.content}`;
			case "thinking":
				return `thinking:${part.content}`;
			case "tool_use":
				return `tool_use:${part.name}:${safeStringify(part.input)}`;
			case "tool_result":
				return `tool_result:${part.name ?? ""}:${safeStringify(part.content)}:${
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

/** Merge server messages with local messages, preserving in-flight optimistic updates */
export function mergeServerMessages(
	previous: PiDisplayMessage[],
	serverMessages: PiDisplayMessage[],
): PiDisplayMessage[] {
	const serverIds = new Set(serverMessages.map((m) => m.id));
	const serverEntries = serverMessages.map((message) => ({
		fingerprint: messageFingerprint(message),
		timestamp: message.timestamp ?? 0,
	}));
	const preserved = previous.filter((message) => {
		if (!shouldPreserveLocalMessage(message)) return false;
		if (serverIds.has(message.id)) return false;
		const localFingerprint = messageFingerprint(message);
		for (const server of serverEntries) {
			if (server.fingerprint !== localFingerprint) continue;
			if (!server.timestamp || !message.timestamp) {
				return false;
			}
			const diff = Math.abs(server.timestamp - message.timestamp);
			if (diff <= MESSAGE_MATCH_WINDOW_MS) {
				return false;
			}
		}
		return true;
	});
	return preserved.length > 0
		? [...serverMessages, ...preserved]
		: serverMessages;
}
