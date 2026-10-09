/**
 * Adapt a machine's stored chat rows to the canonical render contract.
 *
 * A conversation must look the same wherever it is read, so a machine's history
 * is mapped onto `DisplayMessage` and rendered by the same components as a live
 * chat rather than by a parallel preview with its own appearance.
 */

import type { DisplayMessage, DisplayPart } from "@/lib/chat-render-types";

export type MachineHistoryPart = {
	id: string;
	part_type: string;
	text?: string;
	tool_name?: string;
	tool_call_id?: string;
	tool_input?: unknown;
	tool_output?: unknown;
	is_error?: boolean;
	created_at?: string;
};

export type MachineHistoryMessage = {
	id: string;
	role: string;
	parts: MachineHistoryPart[];
	created_at?: string;
};

function toTimestamp(value: string | undefined): number {
	if (!value) return 0;
	// Stored timestamps are UTC but not always ISO-suffixed.
	const normalized = value.includes("T") ? value : value.replace(" ", "T");
	const parsed = Date.parse(
		/[Z+]/.test(normalized) ? normalized : `${normalized}Z`,
	);
	return Number.isNaN(parsed) ? 0 : parsed;
}

function toRole(role: string): DisplayMessage["role"] {
	switch (role) {
		case "user":
		case "assistant":
		case "system":
			return role;
		default:
			return "assistant";
	}
}

function toPart(part: MachineHistoryPart): DisplayPart | null {
	switch (part.part_type) {
		case "text":
			return part.text ? { type: "text", id: part.id, text: part.text } : null;
		case "thinking":
			return part.text
				? { type: "thinking", id: part.id, text: part.text }
				: null;
		case "tool_call":
			return {
				type: "tool_call",
				id: part.id,
				toolCallId: part.tool_call_id ?? part.id,
				name: part.tool_name ?? "tool",
				input: part.tool_input ?? undefined,
				// Stored history is settled: nothing is still running.
				status: part.is_error ? "error" : "success",
			};
		case "tool_result":
			return {
				type: "tool_result",
				id: part.id,
				toolCallId: part.tool_call_id ?? part.id,
				name: part.tool_name,
				output: part.tool_output ?? undefined,
				isError: part.is_error ?? false,
			};
		case "error":
			return part.text ? { type: "error", id: part.id, text: part.text } : null;
		case "compaction":
			return part.text
				? { type: "compaction", id: part.id, text: part.text }
				: null;
		default:
			// An unknown part is dropped rather than rendered as a guess; the
			// message still shows whatever else it carries.
			return null;
	}
}

export function toDisplayMessages(
	messages: MachineHistoryMessage[],
): DisplayMessage[] {
	return messages
		.map((message) => {
			const parts = message.parts
				.map(toPart)
				.filter((part): part is DisplayPart => part !== null);
			return {
				id: message.id,
				role: toRole(message.role),
				parts,
				timestamp: toTimestamp(message.created_at),
			};
		})
		.filter((message) => message.parts.length > 0);
}
