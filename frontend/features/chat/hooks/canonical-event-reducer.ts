import type { DisplayMessage, DisplayPart } from "./types";

export const appendDeltaPart = ({
	message,
	delta,
	partType,
	contentIndex,
	nextPartId,
}: {
	message: DisplayMessage;
	delta: string;
	partType: "text" | "thinking";
	/** The harness's own block number for this delta, when it sent one. */
	contentIndex?: number;
	nextPartId: () => string;
}): void => {
	// Reasoning and content are two sequences whose deltas interleave, so the
	// part at the end of the array is frequently the other one. The harness
	// numbers the block each delta belongs to, and the durable assembler in
	// oqto-history keys its drafts on exactly that number — so the browser
	// addresses the block the same way rather than inferring it from arrival
	// order.
	if (typeof contentIndex === "number") {
		const target = message.parts.find(
			(part) => part.type === partType && part.contentIndex === contentIndex,
		);
		if (target) {
			(target as { text: string }).text += delta;
			return;
		}
		message.parts.push({
			type: partType,
			id: nextPartId(),
			text: delta,
			contentIndex,
		});
		return;
	}

	// No number to go on — an older host, or a synthesised message. Scan back
	// to the most recent part of this type, which reassembles an interleaved
	// pair correctly even though it cannot tell two same-type blocks apart. A
	// tool call stops the scan: what the agent says after running something
	// starts a new block.
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
