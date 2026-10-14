import { mergeServerMessages } from "@/features/chat/hooks/message-utils";
import type { DisplayMessage } from "@/features/chat/hooks/types";
import { describe, expect, it } from "vitest";

const user = (id: string, text: string, timestamp: number): DisplayMessage => ({
	id,
	role: "user",
	parts: [{ type: "text", id: `p-${id}`, text }],
	timestamp,
});

describe("authoritative merge and cached user prompts", () => {
	it("drops cached persisted prompts the server re-identified instead of duplicating them", () => {
		// Browser cache from earlier loads: durable ids from a previous projection.
		const cached = [
			user("msg:old-hi", "Hi", 1_000),
			user("msg:old-q", "Welches modell bist du ?", 12_000),
		];
		// The server re-projected the same history with new durable ids.
		const server = [
			user("msg:new-hi", "Hi", 1_000),
			user("msg:new-q", "Welches modell bist du ?", 12_000),
		];
		let shown = mergeServerMessages(cached, server, "authoritative");
		// Reloading again must not accumulate copies either.
		shown = mergeServerMessages(shown, server, "authoritative");
		expect(shown.map((m) => m.id)).toEqual(["msg:new-hi", "msg:new-q"]);
	});

	it("still keeps an unsent optimistic prompt the server does not know yet", () => {
		const cached = [
			user("msg:a", "Hi", 1_000),
			user("tmp:pending", "draft sent offline", 90_000),
		];
		const server = [user("msg:a", "Hi", 1_000)];
		const shown = mergeServerMessages(cached, server, "authoritative");
		expect(shown.map((m) => m.id)).toEqual(["msg:a", "tmp:pending"]);
	});
});
