import { appendDeltaPart } from "@/features/chat/hooks/canonical-event-reducer";
import type { DisplayMessage } from "@/features/chat/hooks/types";
import { describe, expect, it } from "vitest";

/**
 * Reasoning and content arrive as two sequences whose deltas interleave. The
 * reducer has to reassemble each one, not the order they happened to land in.
 */

function message(): DisplayMessage {
	return {
		id: "assistant-1",
		role: "assistant",
		parts: [],
		timestamp: 0,
	} as unknown as DisplayMessage;
}

function feed(
	target: DisplayMessage,
	stream: ReadonlyArray<["text" | "thinking", string]>,
): void {
	let counter = 0;
	for (const [partType, delta] of stream) {
		appendDeltaPart({
			message: target,
			delta,
			partType,
			nextPartId: () => `part-${++counter}`,
		});
	}
}

/** The same, but carrying the block number the harness sends. */
function feedIndexed(
	target: DisplayMessage,
	stream: ReadonlyArray<["text" | "thinking", number, string]>,
): void {
	let counter = 0;
	for (const [partType, contentIndex, delta] of stream) {
		appendDeltaPart({
			message: target,
			delta,
			partType,
			contentIndex,
			nextPartId: () => `part-${++counter}`,
		});
	}
}

function textOf(target: DisplayMessage, type: "text" | "thinking"): string[] {
	return target.parts
		.filter((part) => part.type === type)
		.map((part) => (part as { text: string }).text);
}

describe("streamed deltas", () => {
	it("keeps a word whole when the two streams alternate mid-sentence", () => {
		const target = message();
		feed(target, [
			["thinking", "Let me write a python"],
			["text", "Sc"],
			["thinking", " script."],
			["text", "affold is in place."],
		]);

		expect(textOf(target, "text")).toEqual(["Scaffold is in place."]);
		expect(textOf(target, "thinking")).toEqual([
			"Let me write a python script.",
		]);
	});

	it("keeps the order the streams started in", () => {
		const target = message();
		feed(target, [
			["thinking", "first"],
			["text", "second"],
		]);
		expect(target.parts.map((part) => part.type)).toEqual(["thinking", "text"]);
	});

	it("starts a new block after a tool call", () => {
		const target = message();
		feed(target, [["text", "Before."]]);
		target.parts.push({
			type: "tool_call",
			id: "call-1",
			toolCallId: "t1",
			name: "read",
			input: {},
			status: "success",
		} as unknown as DisplayMessage["parts"][number]);
		feed(target, [["text", "After."]]);

		// Whatever the agent says after running something is a new block, not a
		// continuation of the sentence it was in.
		expect(textOf(target, "text")).toEqual(["Before.", "After."]);
	});

	it("still merges consecutive deltas of one stream", () => {
		const target = message();
		feed(target, [
			["text", "one "],
			["text", "two "],
			["text", "three"],
		]);
		expect(textOf(target, "text")).toEqual(["one two three"]);
	});
});

describe("the harness's own block numbers", () => {
	it("routes interleaved deltas by index rather than by arrival", () => {
		const target = message();
		feedIndexed(target, [
			["thinking", 0, "Let me write a python"],
			["text", 1, "Sc"],
			["thinking", 0, " script."],
			["text", 1, "affold is in place."],
		]);

		expect(textOf(target, "text")).toEqual(["Scaffold is in place."]);
		expect(textOf(target, "thinking")).toEqual([
			"Let me write a python script.",
		]);
	});

	it("keeps two blocks of one kind apart", () => {
		// The fallback scan cannot do this: with nothing between them to mark a
		// boundary, it would fold the second block into the first.
		const target = message();
		feedIndexed(target, [
			["text", 0, "First block."],
			["text", 2, "Second block."],
			["text", 0, " Still first."],
		]);

		expect(textOf(target, "text")).toEqual([
			"First block. Still first.",
			"Second block.",
		]);
	});

	it("falls back to the scan when no number is sent", () => {
		const target = message();
		feed(target, [
			["thinking", "thinking "],
			["text", "answer "],
			["thinking", "continues"],
			["text", "continues"],
		]);

		expect(textOf(target, "text")).toEqual(["answer continues"]);
		expect(textOf(target, "thinking")).toEqual(["thinking continues"]);
	});
});
