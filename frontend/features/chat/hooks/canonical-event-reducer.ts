import type { DisplayMessage, DisplayPart } from "./types";

export const appendDeltaPart = ({
	message,
	delta,
	partType,
	nextPartId,
}: {
	message: DisplayMessage;
	delta: string;
	partType: "text" | "thinking";
	nextPartId: () => string;
}): void => {
	// Reasoning and content stream as two sequences whose deltas interleave,
	// so the last part is often the *other* one. Merging only into the last
	// part starts a new part on every switch between them, which chops a
	// sentence wherever the two happened to alternate — a word can end up
	// split across two parts with a thinking block wedged between its halves.
	//
	// Scan back to the most recent part of this type instead. A tool call
	// stops the scan: whatever the agent says after running something is a
	// new block, not a continuation of what it was saying before.
	for (let index = message.parts.length - 1; index >= 0; index--) {
		const part = message.parts[index];
		if (part.type === partType) {
			(part as { text: string }).text += delta;
			return;
		}
		if (part.type === "tool_call" || part.type === "tool_result") break;
	}
	message.parts.push({
		type: partType,
		id: nextPartId(),
		text: delta,
	});
};

export const upsertToolCallPart = ({
	message,
	toolCallId,
	name,
	input,
	status,
	nextPartId,
}: {
	message: DisplayMessage;
	toolCallId: string;
	name?: string;
	input?: unknown;
	status: "running" | "success" | "error";
	nextPartId: () => string;
}): DisplayPart => {
	const existing = message.parts.find(
		(part) => part.type === "tool_call" && part.toolCallId === toolCallId,
	);
	if (existing && existing.type === "tool_call") {
		existing.status = status;
		existing.name = name || existing.name;
		if (input !== undefined) {
			existing.input = input;
		}
		return existing;
	}

	const created: DisplayPart = {
		type: "tool_call",
		id: nextPartId(),
		toolCallId,
		name,
		input,
		status,
	};
	message.parts.push(created);
	return created;
};

export const upsertToolResultPart = ({
	message,
	toolCallId,
	name,
	output,
	isError,
	nextPartId,
}: {
	message: DisplayMessage;
	toolCallId: string;
	name?: string;
	output: unknown;
	isError: boolean;
	nextPartId: () => string;
}): DisplayPart => {
	const existing = message.parts.find(
		(part) => part.type === "tool_result" && part.toolCallId === toolCallId,
	);
	if (existing && existing.type === "tool_result") {
		existing.output = output;
		existing.isError = isError;
		existing.name = name || existing.name;
		return existing;
	}

	const created: DisplayPart = {
		type: "tool_result",
		id: nextPartId(),
		toolCallId,
		name,
		output,
		isError,
	};
	message.parts.push(created);
	return created;
};

export const replaceCompactionPlaceholder = ({
	message,
	replacement,
}: {
	message: DisplayMessage;
	replacement: DisplayPart;
}): boolean => {
	const compactIdx = message.parts.findIndex(
		(part) =>
			part.type === "compaction" && part.text === "Compacting context...",
	);
	if (compactIdx < 0) return false;
	message.parts[compactIdx] = replacement;
	return true;
};
