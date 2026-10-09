import type { ChatMessage, ChatMessagePart } from "../platform/contracts";

export type ContextTokenCount = {
	inputTokens: number;
	outputTokens: number;
};

const CHARS_PER_TOKEN = 4;

function normalize(value: number): number {
	return Number.isFinite(value) && value > 0 ? Math.floor(value) : 0;
}

function isCompaction(part: ChatMessagePart): boolean {
	// Not (yet) a canonical part type; arrive defensively via the wire shape.
	return (part as { type: string }).type === "compaction";
}

function partText(part: ChatMessagePart): string {
	switch (part.type) {
		case "text":
			return part.text;
		case "thinking":
			return part.text;
		case "tool_result":
			if (typeof part.output === "string") return part.output;
			if (part.output !== undefined) return JSON.stringify(part.output);
			return "";
		default:
			return "";
	}
}

/**
 * Estimate the tokens in the active context window from the durable timeline.
 *
 * The canonical store does not carry per-message usage, so this is the same
 * fallback the legacy chat uses after its higher-priority sources: text mass
 * since the last compaction, at roughly four characters per token. When real
 * usage arrives over the transport, prefer it and delete the estimate.
 */
export function estimateContextTokens(
	messages: ChatMessage[],
): ContextTokenCount {
	let startIndex = 0;
	for (let i = messages.length - 1; i >= 0; i -= 1) {
		const parts = messages[i]?.parts ?? [];
		if (parts.some(isCompaction)) {
			startIndex = i + 1;
			break;
		}
	}

	let inputTokens = 0;
	let outputTokens = 0;
	for (const message of messages.slice(startIndex)) {
		const parts = message.parts ?? [];
		const text = parts.map(partText).join("\n") || message.content;
		const tokens = Math.ceil(text.length / CHARS_PER_TOKEN);
		if (message.author === "user") {
			inputTokens += tokens;
		} else {
			outputTokens += tokens;
		}
	}
	return {
		inputTokens: normalize(inputTokens),
		outputTokens: normalize(outputTokens),
	};
}
