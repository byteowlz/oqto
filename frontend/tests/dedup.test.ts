import { describe, expect, it } from "vitest";
import {
	messageFingerprint,
	mergeServerMessages,
	shouldPreserveLocalMessage,
} from "@/features/chat/hooks/message-utils";
import type { DisplayMessage } from "@/features/chat/hooks/types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeMessage(
	overrides: Partial<DisplayMessage> & { id: string; role: DisplayMessage["role"] },
): DisplayMessage {
	return {
		parts: [],
		timestamp: Date.now(),
		...overrides,
	};
}

function textMsg(
	id: string,
	role: DisplayMessage["role"],
	text: string,
	extra?: Partial<DisplayMessage>,
): DisplayMessage {
	return makeMessage({
		id,
		role,
		parts: [{ type: "text", id: `p-${id}`, text }],
		...extra,
	});
}

function toolCallMsg(
	id: string,
	toolName: string,
	input: unknown,
	extra?: Partial<DisplayMessage>,
): DisplayMessage {
	return makeMessage({
		id,
		role: "assistant",
		parts: [
			{
				type: "tool_call",
				id: `tc-${id}`,
				name: toolName,
				input,
				status: "complete" as const,
			},
		],
		...extra,
	});
}

// ---------------------------------------------------------------------------
// shouldPreserveLocalMessage
// ---------------------------------------------------------------------------

describe("shouldPreserveLocalMessage", () => {
	it("preserves messages with pi-msg-N pattern", () => {
		const msg = textMsg("pi-msg-42", "user", "hello");
		expect(shouldPreserveLocalMessage(msg)).toBe(true);
	});

	it("preserves compaction messages", () => {
		const msg = makeMessage({ id: "compaction-abc", role: "system" });
		expect(shouldPreserveLocalMessage(msg)).toBe(true);
	});

	it("does not preserve random IDs", () => {
		const msg = textMsg("random-id", "user", "hello");
		expect(shouldPreserveLocalMessage(msg)).toBe(false);
	});

	it("does not preserve UUIDs", () => {
		const msg = textMsg("550e8400-e29b-41d4-a716-446655440000", "user", "hello");
		expect(shouldPreserveLocalMessage(msg)).toBe(false);
	});

	it("requires exact pi-msg-N format (digits only)", () => {
		expect(shouldPreserveLocalMessage(textMsg("pi-msg-", "user", "x"))).toBe(false);
		expect(shouldPreserveLocalMessage(textMsg("pi-msg-abc", "user", "x"))).toBe(false);
		expect(shouldPreserveLocalMessage(textMsg("pi-msg-1", "user", "x"))).toBe(true);
		expect(shouldPreserveLocalMessage(textMsg("pi-msg-999", "user", "x"))).toBe(true);
	});
});

// ---------------------------------------------------------------------------
// messageFingerprint
// ---------------------------------------------------------------------------

describe("messageFingerprint", () => {
	it("includes role in fingerprint", () => {
		const userMsg = textMsg("1", "user", "hello");
		const assistantMsg = textMsg("2", "assistant", "hello");
		expect(messageFingerprint(userMsg)).not.toBe(messageFingerprint(assistantMsg));
	});

	it("same content produces same fingerprint regardless of id", () => {
		const a = textMsg("id-a", "user", "hello");
		const b = textMsg("id-b", "user", "hello");
		expect(messageFingerprint(a)).toBe(messageFingerprint(b));
	});

	it("different text produces different fingerprint", () => {
		const a = textMsg("1", "user", "hello");
		const b = textMsg("2", "user", "goodbye");
		expect(messageFingerprint(a)).not.toBe(messageFingerprint(b));
	});

	it("handles tool_call parts", () => {
		const a = toolCallMsg("1", "read", { path: "/foo" });
		const b = toolCallMsg("2", "read", { path: "/foo" });
		const c = toolCallMsg("3", "read", { path: "/bar" });
		expect(messageFingerprint(a)).toBe(messageFingerprint(b));
		expect(messageFingerprint(a)).not.toBe(messageFingerprint(c));
	});

	it("handles tool_result parts", () => {
		const msg = makeMessage({
			id: "1",
			role: "assistant",
			parts: [
				{
					type: "tool_result",
					id: "tr-1",
					toolCallId: "tc-1",
					name: "read",
					output: "file contents",
					isError: false,
				},
			],
		});
		expect(messageFingerprint(msg)).toContain("tool_result:read:");
	});

	it("handles thinking parts", () => {
		const msg = makeMessage({
			id: "1",
			role: "assistant",
			parts: [{ type: "thinking", id: "th-1", text: "let me think..." }],
		});
		expect(messageFingerprint(msg)).toContain("thinking:let me think...");
	});

	it("handles compaction parts", () => {
		const msg = makeMessage({
			id: "1",
			role: "system",
			parts: [{ type: "compaction", id: "c-1", text: "compacted" }],
		});
		expect(messageFingerprint(msg)).toContain("compaction");
	});

	it("handles empty parts array", () => {
		const msg = makeMessage({ id: "1", role: "user", parts: [] });
		expect(messageFingerprint(msg)).toBe("user|");
	});

	it("handles multi-part messages", () => {
		const msg = makeMessage({
			id: "1",
			role: "assistant",
			parts: [
				{ type: "text", id: "p1", text: "First" },
				{ type: "text", id: "p2", text: "Second" },
			],
		});
		expect(messageFingerprint(msg)).toBe("assistant|text:First|text:Second");
	});
});

// ---------------------------------------------------------------------------
// mergeServerMessages
// ---------------------------------------------------------------------------

describe("mergeServerMessages", () => {
	const now = Date.now();

	describe("server is authoritative", () => {
		it("returns server messages when previous is empty", () => {
			const server = [textMsg("s1", "user", "hello", { timestamp: now })];
			const result = mergeServerMessages([], server);
			expect(result).toEqual(server);
		});

		it("replaces all local messages with server messages", () => {
			const prev = [
				textMsg("pi-msg-1", "user", "hello", { timestamp: now }),
				textMsg("pi-msg-2", "assistant", "world", { timestamp: now }),
			];
			const server = [
				textMsg("history-s1-0", "user", "hello", { timestamp: now }),
				textMsg("history-s1-1", "assistant", "world", { timestamp: now }),
			];
			const result = mergeServerMessages(prev, server);
			expect(result).toHaveLength(2);
			expect(result[0].id).toBe("history-s1-0");
			expect(result[1].id).toBe("history-s1-1");
		});

		it("returns previous when server is empty", () => {
			const prev = [textMsg("pi-msg-1", "user", "hello", { timestamp: now })];
			const result = mergeServerMessages(prev, []);
			expect(result).toEqual(prev);
		});
	});

	describe("preserves in-flight streaming message", () => {
		it("keeps streaming assistant message at tail", () => {
			const prev = [
				textMsg("pi-msg-1", "user", "hello", { timestamp: now }),
				{ ...textMsg("pi-msg-2", "assistant", "partial response", { timestamp: now }), isStreaming: true },
			];
			const server = [
				textMsg("history-s1-0", "user", "hello", { timestamp: now }),
			];
			const result = mergeServerMessages(prev, server);
			expect(result).toHaveLength(2);
			expect(result[0].id).toBe("history-s1-0");
			expect(result[1].id).toBe("pi-msg-2");
			expect(result[1].isStreaming).toBe(true);
		});

		it("keeps optimistic user message + streaming response", () => {
			const prev = [
				textMsg("pi-msg-1", "user", "old msg", { timestamp: now }),
				textMsg("pi-msg-2", "user", "new question", { timestamp: now, clientId: "c-new" }),
				{ ...textMsg("pi-msg-3", "assistant", "thinking...", { timestamp: now }), isStreaming: true },
			];
			const server = [
				textMsg("history-s1-0", "user", "old msg", { timestamp: now }),
				textMsg("history-s1-1", "assistant", "old answer", { timestamp: now }),
			];
			const result = mergeServerMessages(prev, server);
			expect(result).toHaveLength(4);
			expect(result[0].id).toBe("history-s1-0");
			expect(result[1].id).toBe("history-s1-1");
			expect(result[2].id).toBe("pi-msg-2"); // optimistic user msg
			expect(result[3].id).toBe("pi-msg-3"); // streaming
		});
	});

	describe("preserves optimistic user messages", () => {
		it("keeps user message with clientId not in server", () => {
			const prev = [
				textMsg("pi-msg-1", "user", "sent just now", { timestamp: now, clientId: "c-123" }),
			];
			const server = [
				textMsg("history-s1-0", "user", "earlier", { timestamp: now }),
			];
			const result = mergeServerMessages(prev, server);
			expect(result).toHaveLength(2);
			expect(result[1].id).toBe("pi-msg-1");
		});

		it("drops user message when server has matching clientId", () => {
			const prev = [
				textMsg("pi-msg-1", "user", "my msg", { timestamp: now, clientId: "c-123" }),
			];
			const server = [
				textMsg("history-s1-0", "user", "my msg", { timestamp: now, clientId: "c-123" }),
			];
			const result = mergeServerMessages(prev, server);
			expect(result).toHaveLength(1);
			expect(result[0].id).toBe("history-s1-0");
		});
	});

	describe("no duplication of completed messages", () => {
		it("does not duplicate when local pi-msg and server have same text", () => {
			// This was the original bug: pi-msg-* text matched history-*
			// text but different IDs caused both to survive.
			const prev = [
				textMsg("pi-msg-1", "user", "hello world", { timestamp: now }),
				textMsg("pi-msg-2", "assistant", "I can help with that", { timestamp: now }),
			];
			const server = [
				textMsg("history-s1-0", "user", "hello world", { timestamp: now }),
				textMsg("history-s1-1", "assistant", "I can help with that", { timestamp: now }),
			];
			const result = mergeServerMessages(prev, server);
			expect(result).toHaveLength(2);
			// Server versions win
			expect(result[0].id).toBe("history-s1-0");
			expect(result[1].id).toBe("history-s1-1");
		});

		it("does not duplicate thinking+text messages from different sources", () => {
			// Kimi-K2.5 style: streaming has separate thinking+text parts,
			// hstry also has them. IDs differ but content is the same.
			const prev = [
				makeMessage({
					id: "pi-msg-1",
					role: "assistant",
					parts: [
						{ type: "thinking", id: "th-1", text: "Let me think about this..." },
						{ type: "text", id: "t-1", text: "Here is my answer." },
					],
					timestamp: now,
				}),
			];
			const server = [
				makeMessage({
					id: "history-s1-0",
					role: "assistant",
					parts: [
						{ type: "thinking", id: "p-55", text: "Let me think about this..." },
						{ type: "text", id: "p-56", text: "Here is my answer." },
					],
					timestamp: now,
				}),
			];
			const result = mergeServerMessages(prev, server);
			expect(result).toHaveLength(1);
			expect(result[0].id).toBe("history-s1-0");
		});
	});

	describe("in-flight scanning stops at non-in-flight message", () => {
		it("only preserves trailing in-flight messages, not earlier ones", () => {
			const prev = [
				// This user msg with clientId is NOT at the tail (there's a completed
				// assistant msg after it), so it should NOT be preserved.
				textMsg("pi-msg-1", "user", "old question", { timestamp: now, clientId: "c-old" }),
				textMsg("pi-msg-2", "assistant", "old answer", { timestamp: now }),
				// This IS at the tail and is in-flight
				textMsg("pi-msg-3", "user", "new question", { timestamp: now, clientId: "c-new" }),
			];
			const server = [
				textMsg("history-s1-0", "assistant", "different context", { timestamp: now }),
			];
			const result = mergeServerMessages(prev, server);
			// Server + only pi-msg-3 (tail in-flight)
			expect(result).toHaveLength(2);
			expect(result[0].id).toBe("history-s1-0");
			expect(result[1].id).toBe("pi-msg-3");
		});
	});

	describe("edge cases", () => {
		it("handles empty server and empty previous", () => {
			const result = mergeServerMessages([], []);
			expect(result).toEqual([]);
		});

		it("handles both empty", () => {
			expect(mergeServerMessages([], [])).toHaveLength(0);
		});
	});
});

// ---------------------------------------------------------------------------
// Session deduplication (useSessionData logic, tested as pure function)
// ---------------------------------------------------------------------------

describe("session deduplication", () => {
	// Reproduce the dedup logic from useSessionData as a pure function
	function dedupSessions(
		chatHistory: Array<{ id: string; updated_at: number }>,
	) {
		const byId = new Map<string, (typeof chatHistory)[number]>();
		for (const session of chatHistory) {
			const existing = byId.get(session.id);
			if (!existing || session.updated_at >= existing.updated_at) {
				byId.set(session.id, session);
			}
		}
		return Array.from(byId.values());
	}

	it("removes exact duplicates keeping latest", () => {
		const sessions = [
			{ id: "s1", updated_at: 1000 },
			{ id: "s1", updated_at: 2000 },
		];
		const result = dedupSessions(sessions);
		expect(result).toHaveLength(1);
		expect(result[0].updated_at).toBe(2000);
	});

	it("keeps unique sessions untouched", () => {
		const sessions = [
			{ id: "s1", updated_at: 1000 },
			{ id: "s2", updated_at: 2000 },
			{ id: "s3", updated_at: 3000 },
		];
		const result = dedupSessions(sessions);
		expect(result).toHaveLength(3);
	});

	it("handles single session", () => {
		const result = dedupSessions([{ id: "s1", updated_at: 1000 }]);
		expect(result).toHaveLength(1);
	});

	it("handles empty array", () => {
		expect(dedupSessions([])).toHaveLength(0);
	});

	it("keeps the later entry when same updated_at", () => {
		const sessions = [
			{ id: "s1", updated_at: 1000 },
			{ id: "s1", updated_at: 1000 },
		];
		const result = dedupSessions(sessions);
		expect(result).toHaveLength(1);
	});

	it("handles multiple duplicates of different sessions", () => {
		const sessions = [
			{ id: "s1", updated_at: 1000 },
			{ id: "s2", updated_at: 1500 },
			{ id: "s1", updated_at: 3000 },
			{ id: "s2", updated_at: 2000 },
			{ id: "s3", updated_at: 500 },
		];
		const result = dedupSessions(sessions);
		expect(result).toHaveLength(3);
		const s1 = result.find((s) => s.id === "s1");
		const s2 = result.find((s) => s.id === "s2");
		expect(s1?.updated_at).toBe(3000);
		expect(s2?.updated_at).toBe(2000);
	});

	it("does not lose the first occurrence when it is newer", () => {
		const sessions = [
			{ id: "s1", updated_at: 5000 },
			{ id: "s1", updated_at: 1000 },
		];
		const result = dedupSessions(sessions);
		expect(result).toHaveLength(1);
		expect(result[0].updated_at).toBe(5000);
	});
});
