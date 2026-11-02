import type { DisplayMessage } from "@/lib/chat-render-types";
import { groupMessages } from "@/lib/chat-rendering/group-messages";
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

	it("preserves distinct durable rows even when their text is identical", () => {
		const part = { type: "text", id: "p1", text: "Same" } as const;
		const groups = groupMessages([
			message("a1", "assistant", [part]),
			message("a2", "assistant", [part]),
		]);

		expect(groups).toHaveLength(2);
		expect(
			groups.flatMap((group) => group.messages.map((message) => message.id)),
		).toEqual(["a1", "a2"]);
	});

	it("does not collapse distinct assistant image rows into the image fingerprint", () => {
		const groups = groupMessages([
			message("image-a", "assistant", [
				{ type: "image", id: "part-a", source: "url", url: "/image-a.png" },
			]),
			message("image-b", "assistant", [
				{ type: "image", id: "part-b", source: "url", url: "/image-b.png" },
			]),
		]);
		expect(
			groups.flatMap((group) => group.messages.map((message) => message.id)),
		).toEqual(["image-a", "image-b"]);
	});
});
