import { describe, expect, it } from "vitest";

import {
	createParagraphPlayer,
	splitIntoParagraphs,
} from "@/lib/voice/tts-queue";

describe("tts-queue", () => {
	it("splits text into non-empty paragraphs", () => {
		const paragraphs = splitIntoParagraphs("One\n\nTwo\n\n\nThree\n");
		expect(paragraphs).toEqual(["One", "Two", "Three"]);
	});

	it("plays paragraphs in order and stops when session invalidates", async () => {
		const calls: string[] = [];
		const indices: number[] = [];
		let activeSession = 1;

		const player = createParagraphPlayer(
			["First", "Second", "Third"],
			async (text) => {
				calls.push(text);
				if (text === "Second") {
					activeSession = 2;
				}
			},
			(index) => {
				indices.push(index);
			},
			(sessionId) => sessionId === activeSession,
		);

		await player.playFrom(0, 1);

		expect(calls).toEqual(["First", "Second"]);
		expect(indices).toEqual([0, 1]);
	});

	it("plays all paragraphs when session remains active", async () => {
		const calls: string[] = [];
		const indices: number[] = [];

		const player = createParagraphPlayer(
			["A", "B"],
			async (text) => {
				calls.push(text);
			},
			(index) => {
				indices.push(index);
			},
			() => true,
		);

		await player.playFrom(0, 1);

		expect(calls).toEqual(["A", "B"]);
		expect(indices).toEqual([0, 1]);
	});
});
