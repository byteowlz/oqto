import {
	mergeServerMessages,
	normalizeMessages,
	shouldPreserveLocalMessage,
} from "@/features/chat/hooks/message-utils";
import type { DisplayMessage, RawMessage } from "@/features/chat/hooks/types";
import { describe, expect, it } from "vitest";

function makeMessage(
	overrides: Partial<DisplayMessage> & {
		id: string;
		role: DisplayMessage["role"];
	},
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

describe("shouldPreserveLocalMessage", () => {
	it("preserves tmp:* optimistic IDs", () => {
		expect(shouldPreserveLocalMessage(textMsg("tmp:abc", "user", "x"))).toBe(
			true,
		);
	});

	it("keeps narrow legacy fallback + compaction", () => {
		expect(
			shouldPreserveLocalMessage(textMsg("pi-msg-7", "assistant", "x")),
		).toBe(true);
		expect(
			shouldPreserveLocalMessage(
				makeMessage({ id: "compaction-1", role: "system" }),
			),
		).toBe(true);
	});

	it("does not preserve canonical persisted IDs", () => {
		expect(
			shouldPreserveLocalMessage(textMsg("history-s1-1", "user", "x")),
		).toBe(false);
	});
});

describe("normalizeMessages", () => {
	it("keeps canonical message IDs from backend/hstry", () => {
		const raw: RawMessage[] = [
			{
				id: "history-1",
				role: "user",
				content: "hello",
				timestamp: Date.now(),
			},
		];
		const out = normalizeMessages(raw, "fallback");
		expect(out).toHaveLength(1);
		expect(out[0].id).toBe("history-1");
	});

	it("uses fallback only for malformed/legacy payloads with no ID", () => {
		const raw: RawMessage[] = [{ role: "assistant", content: "hello" }];
		const out = normalizeMessages(raw, "legacy");
		expect(out[0].id).toMatch(/^legacy-fallback-/);
	});
});

describe("mergeServerMessages", () => {
	it("partial mode upserts by ID", () => {
		const prev = [
			textMsg("m1", "user", "hello"),
			textMsg("m2", "assistant", "old"),
		];
		const server = [textMsg("m2", "assistant", "new")];
		const result = mergeServerMessages(prev, server, "partial");
		expect(result).toHaveLength(2);
		expect(result.find((m) => m.id === "m2")?.parts[0]).toMatchObject({
			text: "new",
		});
	});

	it("partial mode reconciles optimistic -> persisted via clientId", () => {
		const prev = [
			textMsg("tmp:u1", "user", "question", { clientId: "c-1" }),
			textMsg("tmp:a1", "assistant", "thinking", { isStreaming: true }),
		];
		const server = [
			textMsg("history-u1", "user", "question", { clientId: "c-1" }),
		];
		const result = mergeServerMessages(prev, server, "partial");
		expect(result.some((m) => m.id === "history-u1")).toBe(true);
		expect(result.some((m) => m.id === "tmp:u1")).toBe(false);
		expect(result.some((m) => m.id === "tmp:a1")).toBe(true);
	});

	it("partial mode does not fingerprint-dedupe unrelated IDs", () => {
		const prev = [textMsg("local-1", "user", "same text")];
		const server = [textMsg("history-1", "user", "same text")];
		const result = mergeServerMessages(prev, server, "partial");
		expect(result).toHaveLength(2);
		expect(result.map((m) => m.id)).toEqual(["local-1", "history-1"]);
	});

	it("authoritative mode keeps only in-flight tmp/legacy messages", () => {
		const prev = [
			textMsg("history-old", "user", "old"),
			textMsg("tmp:user", "user", "new", { clientId: "c-new" }),
			textMsg("tmp:assistant", "assistant", "stream", { isStreaming: true }),
		];
		const server = [textMsg("history-old", "user", "old")];
		const result = mergeServerMessages(prev, server, "authoritative");
		expect(result.map((m) => m.id)).toEqual([
			"history-old",
			"tmp:user",
			"tmp:assistant",
		]);
	});

	it("authoritative mode drops optimistic local once persisted clientId arrives", () => {
		const prev = [textMsg("tmp:user", "user", "new", { clientId: "c-new" })];
		const server = [
			textMsg("history-user", "user", "new", { clientId: "c-new" }),
		];
		const result = mergeServerMessages(prev, server, "authoritative");
		expect(result).toHaveLength(1);
		expect(result[0].id).toBe("history-user");
	});

	it("authoritative mode removes stale tmp assistant once persisted assistant exists", () => {
		const prev = [
			textMsg("tmp:assistant-1", "assistant", "in-flight", {
				isStreaming: false,
			}),
		];
		const server = [textMsg("history-assistant-1", "assistant", "in-flight")];
		const result = mergeServerMessages(prev, server, "authoritative");
		expect(result).toHaveLength(1);
		expect(result[0].id).toBe("history-assistant-1");
	});

	it("authoritative mode preserves unmatched local tail messages append-only", () => {
		const prev = [
			textMsg("history-1", "user", "first", { timestamp: 1000 }),
			textMsg("history-2", "assistant", "reply", { timestamp: 2000 }),
			textMsg("local-3", "user", "latest local", {
				timestamp: 3000,
				clientId: "c-local-3",
			}),
		];
		const server = [
			textMsg("history-1", "user", "first", { timestamp: 1000 }),
			textMsg("history-2", "assistant", "reply", { timestamp: 2000 }),
		];
		const result = mergeServerMessages(prev, server, "authoritative");
		expect(result.map((m) => m.id)).toEqual([
			"history-1",
			"history-2",
			"local-3",
		]);
	});

	it("partial mode is treated as authoritative after oqto-log cutover", () => {
		const prev = [
			textMsg("history-old", "user", "old"),
			textMsg("tmp:assistant", "assistant", "stream", { isStreaming: false }),
		];
		const server = [textMsg("history-old", "user", "old")];
		const result = mergeServerMessages(prev, server, "partial");
		expect(result.map((m) => m.id)).toEqual(["history-old"]);
	});

	it("interleavings converge to the same timeline", () => {
		const base: DisplayMessage[] = [
			textMsg("h1", "user", "Q", { clientId: "c-q" }),
			textMsg("h2", "assistant", "A"),
		];
		const partialA = [textMsg("h2", "assistant", "A+")];
		const partialB = [textMsg("h1", "user", "Q", { clientId: "c-q" })];

		const path1 = mergeServerMessages(
			mergeServerMessages(base, partialA, "partial"),
			partialB,
			"partial",
		);
		const path2 = mergeServerMessages(
			mergeServerMessages(base, partialB, "partial"),
			partialA,
			"partial",
		);
		expect(path1).toEqual(path2);
	});
});

describe("session deduplication", () => {
	function dedupSessions(
		chatHistory: Array<{ id: string; updated_at: number }>,
	): Array<{ id: string; updated_at: number }> {
		const byId = new Map<string, (typeof chatHistory)[number]>();
		for (const session of chatHistory) {
			const existing = byId.get(session.id);
			if (!existing || session.updated_at >= existing.updated_at) {
				byId.set(session.id, session);
			}
		}
		return Array.from(byId.values());
	}

	it("removes duplicates and keeps latest", () => {
		const sessions = [
			{ id: "s1", updated_at: 1000 },
			{ id: "s1", updated_at: 2000 },
			{ id: "s2", updated_at: 1500 },
		];
		const result = dedupSessions(sessions);
		expect(result).toHaveLength(2);
		expect(result.find((s) => s.id === "s1")?.updated_at).toBe(2000);
	});
});
