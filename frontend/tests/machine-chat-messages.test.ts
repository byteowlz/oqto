import { groupMessages } from "@/lib/chat-rendering/group-messages";
import {
	type MachineHistoryMessage,
	toDisplayMessages,
} from "@/lib/machine-chat-messages";
import { describe, expect, it } from "vitest";

describe("machine chat messages", () => {
	it("renders a machine conversation through the same contract as a live chat", () => {
		const stored: MachineHistoryMessage[] = [
			{
				id: "m1",
				role: "user",
				created_at: "2026-09-09 11:45:12",
				parts: [{ id: "p1", part_type: "text", text: "check the refs" }],
			},
			{
				id: "m2",
				role: "assistant",
				created_at: "2026-09-09 11:45:20",
				parts: [
					{ id: "p2", part_type: "thinking", text: "looking" },
					{
						id: "p3",
						part_type: "tool_call",
						tool_name: "bash",
						tool_call_id: "call-1",
						tool_input: { cmd: "git status" },
					},
					{
						id: "p4",
						part_type: "tool_result",
						tool_name: "bash",
						tool_call_id: "call-1",
						tool_output: "clean",
					},
					{ id: "p5", part_type: "text", text: "all clean" },
				],
			},
		];

		const messages = toDisplayMessages(stored);
		expect(messages.map((m) => m.role)).toEqual(["user", "assistant"]);

		// Tool data survives as structured parts, not as a JSON blob in a <details>.
		const assistant = messages[1];
		expect(assistant.parts.map((p) => p.type)).toEqual([
			"thinking",
			"tool_call",
			"tool_result",
			"text",
		]);
		const call = assistant.parts[1];
		if (call.type !== "tool_call") throw new Error("expected a tool call");
		expect(call.toolCallId).toBe("call-1");
		expect(call.status).toBe("success");

		// Stored timestamps are UTC even without an explicit zone.
		expect(messages[0].timestamp).toBe(Date.parse("2026-09-09T11:45:12Z"));

		// The renderer groups these exactly as it groups a live chat.
		expect(groupMessages(messages).map((g) => g.role)).toEqual([
			"user",
			"assistant",
		]);
	});

	it("drops parts it cannot render rather than inventing a shape", () => {
		const messages = toDisplayMessages([
			{
				id: "m1",
				role: "assistant",
				parts: [
					{ id: "p1", part_type: "x-unknown-future" },
					{ id: "p2", part_type: "text", text: "kept" },
				],
			},
			{
				id: "m2",
				role: "assistant",
				parts: [{ id: "p3", part_type: "x-unknown-future" }],
			},
		]);

		expect(messages).toHaveLength(1);
		expect(messages[0].parts.map((p) => p.type)).toEqual(["text"]);
	});

	it("marks a failed tool result so it renders as an error", () => {
		const messages = toDisplayMessages([
			{
				id: "m1",
				role: "assistant",
				parts: [
					{
						id: "p1",
						part_type: "tool_result",
						tool_name: "bash",
						tool_call_id: "c1",
						tool_output: "boom",
						is_error: true,
					},
				],
			},
		]);
		const part = messages[0].parts[0];
		if (part.type !== "tool_result") throw new Error("expected a tool result");
		expect(part.isError).toBe(true);
	});
});
