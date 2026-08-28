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
	let turnRows: DisplayMessage[] = [];

	const flushTurn = () => {
		if (turnRows.length === 0) return;
		const rows = turnRows.map(
			(source): DisplayMessage => ({ ...source, parts: [...source.parts] }),
		);
		const calls = new Map<string, DisplayMessage>();

		// Historical adapters do not all preserve call/result row order. Discover
		// every call in the visual turn before moving any result onto its owner.
		for (const row of rows) {
			for (const part of row.parts) {
				if (part.type === "tool_call") calls.set(part.toolCallId, row);
			}
		}
		const results: Extract<DisplayPart, { type: "tool_result" }>[] = [];
		for (const row of rows) {
			row.parts = row.parts.filter((part) => {
				if (part.type !== "tool_result") return true;
				results.push(part);
				return false;
			});
		}
		for (const result of results) {
			calls.get(result.toolCallId)?.parts.push(result);
		}

		// Rows used only to transport results disappear after reduction. This also
		// covers historical assistant-role result rows, not only role=tool rows.
		visualMessages.push(...rows.filter((row) => row.parts.length > 0));
		turnRows = [];
	};

	for (const message of messages) {
		if (message.role === "user") {
			flushTurn();
			visualMessages.push({ ...message, parts: [...message.parts] });
			continue;
		}
		turnRows.push(message);
	}
	flushTurn();

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
