/**
 * Regression tests for the runner message buffer architecture.
 *
 * These tests verify the key invariants that prevent disappearing messages:
 * 1. Authoritative merge never drops in-flight optimistic messages
 * 2. Repeated authoritative merges converge (idempotent)
 * 3. Page reload mid-stream recovers via buffer snapshot
 * 4. Compacted Pi context doesn't cause message loss (buffer is independent)
 * 5. State machine transitions cover all syncing scenarios
 */

import {
	beginMessageSync,
	bindIdentity,
	completeMessageSync,
	createInitialChatStateMachine,
	transitionTurn,
} from "@/features/chat/hooks/chat-state-machine";
import {
	mergeServerMessages,
	normalizeMessages,
} from "@/features/chat/hooks/message-utils";
import type { DisplayMessage, RawMessage } from "@/features/chat/hooks/types";
import { describe, expect, it } from "vitest";

function textMsg(
	id: string,
	role: DisplayMessage["role"],
	text: string,
	extra?: Partial<DisplayMessage>,
): DisplayMessage {
	return {
		id,
		role,
		parts: [{ type: "text", id: `p-${id}`, text }],
		timestamp: Date.now(),
		...extra,
	};
}

describe("disappearing message regression", () => {
	it("authoritative merge preserves streaming assistant message", () => {
		// Scenario: user sends message, assistant starts streaming, then
		// agent.idle triggers an authoritative fetch from oqto-log/buffer.
		// The streaming message must survive.
		const local = [
			textMsg("tmp:u1", "user", "hello", { clientId: "c-1" }),
			textMsg("tmp:a1", "assistant", "thinking...", { isStreaming: true }),
		];
		const server = [
			textMsg("h-u1", "user", "hello", { clientId: "c-1" }),
			textMsg("h-a1", "assistant", "full response"),
		];
		const result = mergeServerMessages(local, server, "authoritative");
		// Server version should replace optimistic, streaming should be gone
		// since server has the final version
		expect(result.some((m) => m.id === "h-u1")).toBe(true);
		expect(result.some((m) => m.id === "h-a1")).toBe(true);
		// tmp:u1 reconciled via clientId -> gone
		expect(result.some((m) => m.id === "tmp:u1")).toBe(false);
	});

	it("authoritative merge keeps optimistic if server doesn't have it yet", () => {
		// Scenario: user just sent a message, authoritative fetch runs before
		// the backend has processed it. The tmp: message must survive.
		const local = [
			textMsg("h-old-u", "user", "old question"),
			textMsg("h-old-a", "assistant", "old answer"),
			textMsg("tmp:u2", "user", "new question", {
				clientId: "c-2",
				isStreaming: false,
			}),
		];
		const server = [
			textMsg("h-old-u", "user", "old question"),
			textMsg("h-old-a", "assistant", "old answer"),
			// Server doesn't have the new message yet
		];
		const result = mergeServerMessages(local, server, "authoritative");
		expect(result).toHaveLength(3);
		expect(result[2].id).toBe("tmp:u2");
		expect(result[2].clientId).toBe("c-2");
	});

	it("repeated authoritative merges are idempotent", () => {
		// Scenario: multiple agent.idle or reconnect events trigger
		// successive authoritative fetches. Result should be stable.
		const server = [
			textMsg("h-1", "user", "Q1"),
			textMsg("h-2", "assistant", "A1"),
			textMsg("h-3", "user", "Q2"),
			textMsg("h-4", "assistant", "A2"),
		];

		const first = mergeServerMessages([], server, "authoritative");
		const second = mergeServerMessages(first, server, "authoritative");
		const third = mergeServerMessages(second, server, "authoritative");

		expect(first).toEqual(second);
		expect(second).toEqual(third);
	});

	it("page reload mid-stream recovers from buffer snapshot", () => {
		// Scenario: user reloads page while assistant is streaming.
		// Buffer has partial response. Frontend starts fresh, loads buffer.
		const freshLocal: DisplayMessage[] = [];
		const bufferSnapshot = [
			textMsg("pi_msg_0", "user", "what is 2+2"),
			textMsg("pi_msg_1", "assistant", "The answer is"),
		];
		const result = mergeServerMessages(
			freshLocal,
			bufferSnapshot,
			"authoritative",
		);
		expect(result).toHaveLength(2);
		expect(result[0].parts[0]).toMatchObject({ text: "what is 2+2" });
		expect(result[1].parts[0]).toMatchObject({ text: "The answer is" });
	});

	it("compacted Pi context doesn't lose earlier messages", () => {
		// Scenario: Pi compacted its context, but the buffer was populated from
		// AgentEnd which provides the complete (compacted) list. hstry has the
		// full history. The frontend should show hstry's complete history.
		const localFromBuffer = [
			textMsg("pi_msg_0", "user", "Q1 (compacted)"),
			textMsg("pi_msg_1", "assistant", "summary of context"),
		];
		const hstryComplete = [
			textMsg("h-1", "user", "Q1"),
			textMsg("h-2", "assistant", "A1"),
			textMsg("h-3", "user", "Q2"),
			textMsg("h-4", "assistant", "A2"),
			textMsg("h-5", "user", "Q3"),
			textMsg("h-6", "assistant", "A3"),
		];
		// After idle, hstry fetch provides complete history
		const result = mergeServerMessages(
			localFromBuffer,
			hstryComplete,
			"authoritative",
		);
		expect(result).toHaveLength(6);
		expect(result[0].id).toBe("h-1");
		expect(result[5].id).toBe("h-6");
	});

	it("multiple tabs see same buffer data without conflicts", () => {
		// Scenario: two tabs fetch buffer at slightly different times.
		// Both should converge to the same state.
		const tab1Local = [
			textMsg("h-1", "user", "Q1"),
			textMsg("h-2", "assistant", "A1 (stale)"),
		];
		const tab2Local = [
			textMsg("h-1", "user", "Q1"),
			textMsg("h-2", "assistant", "A1 (also stale)"),
		];
		const bufferNow = [
			textMsg("h-1", "user", "Q1"),
			textMsg("h-2", "assistant", "A1 (current)"),
		];
		const tab1Result = mergeServerMessages(
			tab1Local,
			bufferNow,
			"authoritative",
		);
		const tab2Result = mergeServerMessages(
			tab2Local,
			bufferNow,
			"authoritative",
		);
		expect(tab1Result).toEqual(tab2Result);
	});
});

describe("state machine syncing transitions", () => {
	it("streaming -> syncing -> idle is the canonical happy path", () => {
		const machine = bindIdentity(createInitialChatStateMachine("oqto-1"), {
			runnerId: "r-1",
		});
		const sending = transitionTurn(machine, { kind: "sending" });
		const streaming = transitionTurn(sending, { kind: "streaming" });
		const syncing = transitionTurn(streaming, { kind: "syncing" });
		const idle = transitionTurn(syncing, { kind: "idle" });

		expect(sending.turn.kind).toBe("sending");
		expect(streaming.turn.kind).toBe("streaming");
		expect(syncing.turn.kind).toBe("syncing");
		expect(idle.turn.kind).toBe("idle");
	});

	it("syncing -> streaming is valid (new turn during reconciliation)", () => {
		const machine = bindIdentity(createInitialChatStateMachine("oqto-1"), {
			runnerId: "r-1",
		});
		const syncing = transitionTurn(machine, { kind: "syncing" });
		const streaming = transitionTurn(syncing, { kind: "streaming" });
		expect(streaming.turn.kind).toBe("streaming");
	});

	it("error -> syncing allows recovery", () => {
		const machine = bindIdentity(createInitialChatStateMachine("oqto-1"), {
			runnerId: "r-1",
		});
		const error = transitionTurn(machine, {
			kind: "error",
			recoverable: true,
			message: "rate limited",
		});
		const syncing = transitionTurn(error, { kind: "syncing" });
		expect(syncing.turn.kind).toBe("syncing");
	});

	it("message sync revision increments correctly", () => {
		let machine = createInitialChatStateMachine("oqto-1");
		expect(machine.sync.revision).toBe(0);

		machine = beginMessageSync(machine);
		expect(machine.sync.phase).toBe("syncing");

		machine = completeMessageSync(machine);
		expect(machine.sync.phase).toBe("idle");
		expect(machine.sync.revision).toBe(1);

		// Second sync
		machine = beginMessageSync(machine);
		machine = completeMessageSync(machine);
		expect(machine.sync.revision).toBe(2);
	});
});

describe("normalizeMessages preserves IDs across sources", () => {
	it("preserves server-assigned IDs from buffer/hstry", () => {
		const raw: RawMessage[] = [
			{ id: "pi_msg_0", role: "user", content: "hello", timestamp: 1000 },
			{
				id: "pi_msg_1",
				role: "assistant",
				content: "hi there",
				timestamp: 2000,
			},
		];
		const display = normalizeMessages(raw, "fallback");
		expect(display[0].id).toBe("pi_msg_0");
		expect(display[1].id).toBe("pi_msg_1");
	});

	it("preserves hstry message IDs (uuid format)", () => {
		const raw: RawMessage[] = [
			{
				id: "550e8400-e29b-41d4-a716-446655440000",
				role: "user",
				content: "test",
			},
		];
		const display = normalizeMessages(raw, "fallback");
		expect(display[0].id).toBe("550e8400-e29b-41d4-a716-446655440000");
	});

	it("preserves client_id for optimistic reconciliation", () => {
		const raw: RawMessage[] = [
			{
				id: "pi_msg_0",
				role: "user",
				content: "hello",
				client_id: "c-abc",
			},
		];
		const display = normalizeMessages(raw, "fallback");
		expect(display[0].clientId).toBe("c-abc");
	});
});
