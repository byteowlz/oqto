import type { DisplayMessage } from "@/lib/chat-render-types";
import {
	coalesceToolResults,
	groupMessages,
} from "@/lib/chat-rendering/group-messages";
import { describe, expect, it } from "vitest";

function message(
	id: string,
	role: DisplayMessage["role"],
	parts: DisplayMessage["parts"],
): DisplayMessage {
	return { id, role, parts, timestamp: 0 };
}

describe("canonical Chat message grouping", () => {
	it("renders thinking, tool call, result, and final text as one assistant turn", () => {
		const groups = groupMessages([
			message("a1", "assistant", [
				{ type: "thinking", id: "p1", text: "Checking" },
				{
					type: "tool_call",
					id: "p2",
					toolCallId: "call-1",
					name: "TodoWrite",
					input: { todos: [] },
					status: "success",
				},
			]),
			message("r1", "tool", [
				{
					type: "tool_result",
					id: "p3",
					toolCallId: "call-1",
					name: "TodoWrite",
					output: { todos: [] },
					isError: false,
				},
			]),
			message("a2", "assistant", [{ type: "text", id: "p4", text: "Done." }]),
		]);

		expect(groups).toHaveLength(1);
		expect(groups[0]?.role).toBe("assistant");
		expect(groups[0]?.messages.map(({ id }) => id)).toEqual(["a1", "a2"]);
		expect(groups[0]?.messages[0]?.parts.map(({ type }) => type)).toEqual([
			"thinking",
			"tool_call",
			"tool_result",
		]);
	});

	it("never exposes a standalone tool-result row to the renderer", () => {
		const messages = coalesceToolResults([
			message("a1", "assistant", [
				{
					type: "tool_call",
					id: "call-part",
					toolCallId: "call-1",
					name: "bash",
					input: { command: "printf ok" },
					status: "success",
				},
			]),
			message("r1", "tool", [
				{
					type: "tool_result",
					id: "result-part",
					toolCallId: "call-1",
					name: "bash",
					output: { stdout: "ok" },
					isError: false,
				},
			]),
		]);

		expect(messages).toHaveLength(1);
		expect(messages[0]?.id).toBe("a1");
		expect(messages[0]?.parts).toHaveLength(2);
		expect(messages[0]?.parts[1]).toMatchObject({
			type: "tool_result",
			toolCallId: "call-1",
		});
	});

	it("pairs historical result-before-call rows without leaving a running call", () => {
		const messages = coalesceToolResults([
			message("r1", "tool", [
				{
					type: "tool_result",
					id: "result-part",
					toolCallId: "historical-call",
					name: "read",
					output: "file content",
					isError: false,
				},
			]),
			message("a1", "assistant", [
				{
					type: "tool_call",
					id: "call-part",
					toolCallId: "historical-call",
					name: "read",
					input: { filePath: "README.md" },
					status: "success",
				},
			]),
		]);

		expect(messages).toHaveLength(1);
		expect(messages[0]?.id).toBe("a1");
		expect(messages[0]?.parts.map(({ type }) => type)).toEqual([
			"tool_call",
			"tool_result",
		]);
	});

	it("suppresses an unmatched result at a pagination boundary", () => {
		const messages = coalesceToolResults([
			message("r1", "tool", [
				{
					type: "tool_result",
					id: "result-part",
					toolCallId: "call-on-older-page",
					name: "read",
					output: "content",
					isError: false,
				},
			]),
		]);

		expect(messages).toEqual([]);
		expect(groupMessages(messages)).toEqual([]);
	});

	it("keeps distinct prose-only assistant turns separate", () => {
		const groups = groupMessages([
			message("a1", "assistant", [{ type: "text", id: "p1", text: "First" }]),
			message("a2", "assistant", [{ type: "text", id: "p2", text: "Second" }]),
		]);

		expect(groups).toHaveLength(2);
	});

	it("drops duplicate assistant rows produced by reconnect races", () => {
		const part = { type: "text", id: "p1", text: "Same" } as const;
		const groups = groupMessages([
			message("a1", "assistant", [part]),
			message("a2", "assistant", [part]),
		]);

		expect(groups).toHaveLength(1);
		expect(groups[0]?.messages).toHaveLength(1);
	});
});
