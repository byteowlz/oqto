import { getMessageText } from "@/lib/message-text";
import { describe, expect, it } from "vitest";

describe("getMessageText", () => {
	it("joins only text parts with paragraph breaks", () => {
		const parts = [
			{ type: "text", text: "Hello" },
			{ type: "tool", tool: "read", state: { status: "completed" } },
			{ type: "text", text: "World" },
		];

		expect(getMessageText(parts as never)).toBe("Hello\n\nWorld");
	});

	it("returns empty string when no text parts exist", () => {
		const parts = [
			{ type: "tool", tool: "read", state: { status: "running" } },
		];
		expect(getMessageText(parts as never)).toBe("");
	});
});
