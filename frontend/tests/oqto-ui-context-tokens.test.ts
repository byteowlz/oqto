import { estimateContextTokens } from "@/src/oqto-ui/chat/context-tokens";
import type { ChatMessage } from "@/src/oqto-ui/platform/contracts";
import { describe, expect, it } from "vitest";

function message(
	id: string,
	author: ChatMessage["author"],
	parts: ChatMessage["parts"],
	content = "",
): ChatMessage {
	return { id, author, content, parts, time: "12:00" };
}

describe("estimateContextTokens", () => {
	it("estimates text mass at four characters per token", () => {
		const messages = [
			message("u1", "user", [{ type: "text", id: "t1", text: "a".repeat(40) }]),
			message("a1", "agent", [
				{ type: "text", id: "t2", text: "b".repeat(80) },
			]),
		];
		expect(estimateContextTokens(messages)).toEqual({
			inputTokens: 10,
			outputTokens: 20,
		});
	});

	it("counts tool output and thinking as agent tokens", () => {
		const messages = [
			message("a1", "agent", [
				{ type: "thinking", id: "th", text: "c".repeat(36) },
				{
					type: "tool_result",
					id: "tr",
					toolCallId: "call",
					output: "d".repeat(36),
					isError: false,
				},
			]),
		];
		expect(estimateContextTokens(messages)).toEqual({
			inputTokens: 0,
			outputTokens: 19,
		});
	});

	it("restarts counting after the last compaction marker", () => {
		const messages = [
			message("u0", "user", [
				{ type: "text", id: "old", text: "x".repeat(400) },
			]),
			message("c1", "agent", [
				{ type: "text", id: "compaction", text: "summary" },
			]),
			message("u1", "user", [
				{ type: "text", id: "new", text: "y".repeat(40) },
			]),
		];
		// The compaction part type arrives via the wire shape; mark it here.
		(
			messages[1].parts as Array<{ type: string; id: string; text: string }>
		)[0].type = "compaction";
		expect(estimateContextTokens(messages)).toEqual({
			inputTokens: 10,
			outputTokens: 0,
		});
	});

	it("is empty for an empty timeline", () => {
		expect(estimateContextTokens([])).toEqual({
			inputTokens: 0,
			outputTokens: 0,
		});
	});
});
