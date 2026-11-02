import type { DisplayMessage } from "@/lib/chat-render-types";

export type MessageGroup = {
	role: DisplayMessage["role"];
	messages: DisplayMessage[];
};

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
 * Canonical visual grouping shared by every Chat host.
 *
 * Durable storage rows are not visual message cards: one assistant turn can be
 * split across thinking, tool calls/results, and final text rows. This function
 * preserves the proven legacy grouping semantics independently of transport.
 */
export function groupMessages(messages: DisplayMessage[]): MessageGroup[] {
	const groups: MessageGroup[] = [];
	let current: MessageGroup | null = null;

	for (const message of messages) {
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
			// Grouping is presentation only. Distinct durable rows must survive
			// even when their text or image bytes happen to be identical.
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
