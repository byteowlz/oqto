import { groupMessages } from "@/features/chat/rendering/group-messages";
import type { DisplayMessage } from "@/lib/chat-render-types";
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
		expect(groups[0]?.messages.map(({ id }) => id)).toEqual(["a1", "r1", "a2"]);
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
