import { normalizeMessages } from "@/features/chat/hooks/message-utils";
import type { RawMessage } from "@/features/chat/hooks/types";
import { describe, expect, it } from "vitest";

describe("messages without visible content", () => {
	it("drops Pi 1.1's empty system snapshot but keeps the conversation", () => {
		const raw = [
			// Shape Pi 1.1 writes at session start (session 01a1220c, line 6).
			{
				id: "sys-1",
				role: "system",
				content: "",
				sections: { prompt: "..." },
				toolsAdded: ["read"],
				timestamp: 1_000,
			},
			{ id: "u-1", role: "user", content: "Hi", timestamp: 1_001 },
			{
				id: "a-1",
				role: "assistant",
				content: "Hi Wismut! Wie kann ich helfen?",
				timestamp: 1_002,
			},
		] as unknown as RawMessage[];
		const shown = normalizeMessages(raw, "s");
		expect(shown.map((m) => m.id)).toEqual(["u-1", "a-1"]);
	});

	it("drops whitespace-only assistant text but keeps a user's empty prompt", () => {
		const raw = [
			{ id: "u-1", role: "user", content: "", timestamp: 1 },
			{ id: "a-1", role: "assistant", content: "  \n", timestamp: 2 },
			{ id: "s-1", role: "system", content: "Compacted context", timestamp: 3 },
		] as unknown as RawMessage[];
		expect(normalizeMessages(raw, "s").map((m) => m.id)).toEqual([
			"u-1",
			"s-1",
		]);
	});
});
