import type { DisplayMessage, DisplayPart } from "@/lib/chat-render-types";

export type MessageGroup = {
	role: DisplayMessage["role"];
	messages: DisplayMessage[];
};

function fingerprintPartForRender(part: DisplayPart): string {
	switch (part.type) {
		case "text":
			return `text:${part.text}`;
		case "thinking":
			return `thinking:${part.text}`;
		case "tool_call":
			return `tool_call:${part.toolCallId}:${part.name}:${JSON.stringify(part.input ?? null)}`;
		case "tool_result":
			return `tool_result:${part.toolCallId}:${part.name ?? ""}:${JSON.stringify(part.output ?? null)}:${part.isError ? "1" : "0"}`;
		case "compaction":
			return `compaction:${part.text}`;
		case "error":
			return `error:${part.text}`;
		case "image":
			return "image";
		case "file_ref":
			return `file_ref:${part.uri}`;
		default:
			return part.type;
	}
}

function messageRenderFingerprint(message: DisplayMessage): string {
	const parts = message.parts.map(fingerprintPartForRender);
	return `${message.role}|${parts.join("|")}`;
}

function hasRenderableAssistantPayload(message: DisplayMessage): boolean {
	if (message.role !== "assistant") return false;
	return message.parts.some((part) => {
		if (part.type === "text") return part.text.trim().length > 0;
		return part.type === "image" || part.type === "file_ref";
	});
}

function isAssistantAuxiliaryMessage(message: DisplayMessage): boolean {
	return (
		message.role === "assistant" && !hasRenderableAssistantPayload(message)
	);
}

/**
 * Reduce transport rows into visual messages before grouping.
 *
 * A tool result is state belonging to its call, never an independent visual
 * message. Results are moved onto the assistant message containing the matching
 * call. Orphaned results (for example at a pagination boundary before the call
 * page is loaded) remain invisible rather than becoming standalone JSON cards.
 */
export function coalesceToolResults(
	messages: DisplayMessage[],
): DisplayMessage[] {
	const visualMessages: DisplayMessage[] = [];
	let calls = new Map<string, DisplayMessage>();

	for (const source of messages) {
		if (source.role === "user") calls = new Map();

		const message: DisplayMessage = { ...source, parts: [] };
		for (const part of source.parts) {
			if (part.type === "tool_call") {
				message.parts.push(part);
				calls.set(part.toolCallId, message);
				continue;
			}
			if (part.type === "tool_result") {
				const owner = calls.get(part.toolCallId);
				if (owner) owner.parts.push(part);
				continue;
			}
			message.parts.push(part);
		}

		// Tool rows exist only to transport results. Once those results have been
		// attached to their calls, the row has no independent visual identity.
		if (message.role === "tool" && message.parts.length === 0) continue;
		visualMessages.push(message);
	}

	return visualMessages;
}

/**
 * Canonical visual grouping shared by every Chat host.
 *
 * Durable storage rows are not visual message cards: one assistant turn can be
 * split across thinking, tool calls/results, and final text rows. This function
 * preserves the proven legacy grouping semantics independently of transport.
 */
export function groupMessages(messages: DisplayMessage[]): MessageGroup[] {
	const groups: MessageGroup[] = [];
	let current: MessageGroup | null = null;

	for (const message of coalesceToolResults(messages)) {
		if (message.role === "user") {
			const hasRenderableContent = message.parts.some((part) => {
				if (part.type === "text") return part.text.trim().length > 0;
				return part.type === "image" || part.type === "file_ref";
			});
			if (!hasRenderableContent) continue;
		}

		// A durable tool-result row is visually part of the assistant turn that
		// issued the call, not a separate speaker message.
		if (message.role === "tool" && current?.role === "assistant") {
			current.messages.push(message);
			continue;
		}

		if (!current || current.role !== message.role) {
			current = { role: message.role, messages: [message] };
			groups.push(current);
			continue;
		}

		const previous = current.messages[current.messages.length - 1];
		if (message.role === "assistant") {
			if (
				previous &&
				messageRenderFingerprint(previous) === messageRenderFingerprint(message)
			) {
				continue;
			}
			const currentHasRenderableAssistant = current.messages.some(
				(candidate) =>
					candidate.role === "assistant" &&
					hasRenderableAssistantPayload(candidate),
			);
			const currentHasToolActivity = current.messages.some((candidate) =>
				candidate.parts.some(
					(part) => part.type === "tool_call" || part.type === "tool_result",
				),
			);
			if (
				(previous &&
					(isAssistantAuxiliaryMessage(previous) ||
						isAssistantAuxiliaryMessage(message))) ||
				currentHasToolActivity ||
				(!currentHasRenderableAssistant &&
					hasRenderableAssistantPayload(message))
			) {
				current.messages.push(message);
				continue;
			}
			current = { role: message.role, messages: [message] };
			groups.push(current);
			continue;
		}

		current.messages.push(message);
	}

	return groups;
}
