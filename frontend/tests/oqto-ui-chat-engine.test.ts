import { describe, expect, it } from "vitest";
import {
	type EngineEvent,
	type TurnDraft,
	projectEvent,
} from "../src/oqto-ui/chat/engine/projection";
import {
	bindIdentity,
	createInitialChatStateMachine,
	transitionTurn,
} from "../src/oqto-ui/chat/engine/turn-machine";

function makePartIds(): () => string {
	let n = 0;
	return () => `p${++n}`;
}

function run(events: EngineEvent[]): {
	draft: TurnDraft | null;
	outcomes: string[];
} {
	const nextPartId = makePartIds();
	let draft: TurnDraft | null = null;
	const outcomes: string[] = [];
	for (const event of events) {
		const outcome = projectEvent(draft, event, nextPartId, () => 1000);
		outcomes.push(outcome.kind);
		if (outcome.kind === "draft") draft = outcome.draft;
		if (outcome.kind === "turn-ended" || outcome.kind === "resync")
			draft = null;
	}
	return { draft, outcomes };
}

describe("chat engine projection", () => {
	it("assembles partial text deltas into one coalesced part", () => {
		const { draft } = run([
			{ event: "stream.message_start", role: "assistant", session_id: "s1" },
			{ event: "stream.text_delta", delta: "Hel" },
			{ event: "stream.text_delta", delta: "lo " },
			{ event: "stream.text_delta", delta: "world" },
		]);
		expect(draft?.parts).toEqual([
			{ type: "text", id: "p1", text: "Hello world" },
		]);
	});

	it("keeps thinking and text as separate parts in stream order", () => {
		const { draft } = run([
			{ event: "stream.message_start", role: "assistant" },
			{ event: "stream.thinking_delta", delta: "hmm" },
			{ event: "stream.text_delta", delta: "answer" },
			{ event: "stream.thinking_delta", delta: " more" },
		]);
		expect(draft?.parts.map((part) => part.type)).toEqual([
			"thinking",
			"text",
			"thinking",
		]);
	});

	it("ignores user-echo and tool-role message starts", () => {
		const { outcomes } = run([
			{ event: "stream.message_start", role: "user" },
			{ event: "stream.message_start", role: "tool" },
		]);
		expect(outcomes).toEqual(["ignored", "ignored"]);
	});

	it("handles out-of-order tool results (end before start)", () => {
		const { draft } = run([
			{ event: "stream.message_start", role: "assistant" },
			{
				event: "tool.end",
				tool_call_id: "t1",
				name: "read",
				output: "ok",
				is_error: false,
			},
			{ event: "tool.start", tool_call_id: "t1", name: "read" },
		]);
		const tool = draft?.parts.find((part) => part.type === "tool_call");
		expect(tool).toMatchObject({
			toolCallId: "t1",
			name: "read",
			output: "ok",
		});
		expect(draft?.parts).toHaveLength(1);
	});

	it("merges duplicate tool events into a single upserted part", () => {
		const { draft } = run([
			{ event: "stream.message_start", role: "assistant" },
			{ event: "stream.tool_call_start", tool_call_id: "t1", name: "edit" },
			{ event: "tool.start", tool_call_id: "t1", name: "edit" },
			{
				event: "tool.end",
				tool_call_id: "t1",
				name: "edit",
				output: "done",
				is_error: false,
			},
			{
				event: "tool.end",
				tool_call_id: "t1",
				name: "edit",
				output: "done",
				is_error: false,
			},
		]);
		const tools = draft?.parts.filter((part) => part.type === "tool_call");
		expect(tools).toHaveLength(1);
		expect(tools?.[0]).toMatchObject({ status: "success", output: "done" });
	});

	it("marks failed tools as error", () => {
		const { draft } = run([
			{ event: "stream.message_start", role: "assistant" },
			{ event: "tool.start", tool_call_id: "t1", name: "bash" },
			{ event: "tool.end", tool_call_id: "t1", is_error: true, output: "boom" },
		]);
		expect(draft?.parts[0]).toMatchObject({
			type: "tool_call",
			status: "error",
		});
	});

	it("replaces retry banner in place and clears it on retry.end", () => {
		const first = run([
			{ event: "stream.message_start", role: "assistant" },
			{
				event: "retry.start",
				error: "overloaded",
				attempt: 1,
				max_attempts: 3,
			},
			{
				event: "retry.start",
				error: "overloaded",
				attempt: 2,
				max_attempts: 3,
			},
		]);
		const banners = first.draft?.parts.filter(
			(part) => part.type === "error" && part.retrying,
		);
		expect(banners).toHaveLength(1);
		expect(banners?.[0]).toMatchObject({ retryAttempt: 2 });

		const cleared = run([
			{ event: "stream.message_start", role: "assistant" },
			{ event: "retry.start", error: "overloaded" },
			{ event: "retry.end", success: true },
			{ event: "stream.text_delta", delta: "recovered" },
		]);
		expect(cleared.draft?.parts).toEqual([
			{ type: "text", id: "p2", text: "recovered" },
		]);
	});

	it("replaces the compaction placeholder with the result", () => {
		const { draft } = run([
			{ event: "stream.message_start", role: "assistant" },
			{ event: "compact.start" },
			{ event: "compact.end", success: true, tokens_before: 61400 },
		]);
		expect(draft?.parts).toHaveLength(1);
		expect(draft?.parts[0]).toMatchObject({
			type: "compaction",
			text: "Context compacted (61.4K tokens summarized)",
		});
	});

	it("ends the turn and drops the draft on done/idle/error", () => {
		for (const event of ["stream.done", "agent.idle", "agent.error"]) {
			const { draft, outcomes } = run([
				{ event: "stream.message_start", role: "assistant" },
				{ event: "stream.text_delta", delta: "partial" },
				{ event },
			]);
			expect(outcomes.at(-1)).toBe("turn-ended");
			expect(draft).toBeNull();
		}
	});

	it("signals resync (mid-turn disconnect) so pages get refetched", () => {
		const { draft, outcomes } = run([
			{ event: "stream.message_start", role: "assistant" },
			{ event: "stream.text_delta", delta: "half a mess" },
			{ event: "stream.resync_required" },
		]);
		expect(outcomes.at(-1)).toBe("resync");
		expect(draft).toBeNull();
	});

	it("starts a draft from a mid-turn delta when message_start was missed", () => {
		const { draft } = run([
			{ event: "stream.text_delta", delta: "late join", session_id: "s1" },
		]);
		expect(draft?.parts).toEqual([
			{ type: "text", id: "p1", text: "late join" },
		]);
		expect(draft?.sessionId).toBe("s1");
	});
});

describe("chat engine turn machine", () => {
	it("refuses non-idle turns before identity binds", () => {
		const machine = createInitialChatStateMachine("client-1");
		const next = transitionTurn(machine, { kind: "streaming" });
		expect(next.turn.kind).toBe("idle");
	});

	it("refuses rebinding to a different session mid-turn", () => {
		let machine = createInitialChatStateMachine("client-1");
		machine = bindIdentity(machine, { runnerId: "oqto-a" });
		machine = transitionTurn(machine, { kind: "streaming" });
		machine = bindIdentity(machine, { runnerId: "oqto-b" });
		expect(machine.identity).toMatchObject({ runnerId: "oqto-a" });
	});

	it("allows runner-initiated turns from idle", () => {
		let machine = createInitialChatStateMachine(null);
		machine = bindIdentity(machine, { runnerId: "oqto-a" });
		machine = transitionTurn(machine, { kind: "streaming" });
		expect(machine.turn.kind).toBe("streaming");
	});
});
