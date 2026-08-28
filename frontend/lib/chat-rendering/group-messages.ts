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
