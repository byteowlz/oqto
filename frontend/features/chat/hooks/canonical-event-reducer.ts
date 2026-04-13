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
	const lastPart = message.parts[message.parts.length - 1];
	if (lastPart?.type === partType) {
		(lastPart as { text: string }).text += delta;
		return;
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
