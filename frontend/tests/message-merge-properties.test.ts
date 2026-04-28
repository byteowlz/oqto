import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
	mergeServerMessages,
	normalizeMessages,
	shouldPreserveLocalMessage,
} from "@/features/chat/hooks/message-utils";
import type { DisplayMessage, RawMessage } from "@/features/chat/hooks/types";
import { describe, expect, it } from "vitest";

/**
 * Property-style invariants for merge/normalize behavior.
 *
 * Invariants (checked repeatedly across deterministic seeded timelines):
 * 1) No duplicate message IDs in merged output.
 * 2) At most one message per logical turn key (clientId).
 * 3) If server provides a persisted message for a clientId, merged output keeps
 *    one logical message for that clientId (tmp/local duplicates are superseded).
 * 4) Partial snapshots must not drop unsuperseded local tmp/legacy messages.
 * 5) Authoritative merge is idempotent for the same server snapshot.
 */

type RegressionSeedFixture = {
	name: string;
	description: string;
	previous: DisplayMessage[];
	partialServer: DisplayMessage[];
	authoritativeServer: DisplayMessage[];
	expectedPartialIds: string[];
	expectedAuthoritativeIds: string[];
};

function loadFixture(name: string): RegressionSeedFixture {
	const path = join(
		process.cwd(),
		"tests",
		"fixtures",
		"message-property-seeds",
		name,
	);
	return JSON.parse(readFileSync(path, "utf8")) as RegressionSeedFixture;
}

function assertNoDuplicateIds(messages: DisplayMessage[]): void {
	const ids = messages.map((m) => m.id);
	expect(new Set(ids).size).toBe(ids.length);
}

function assertSingleMessagePerClientId(messages: DisplayMessage[]): void {
	const counts = new Map<string, number>();
	for (const msg of messages) {
		if (!msg.clientId) continue;
		counts.set(msg.clientId, (counts.get(msg.clientId) ?? 0) + 1);
	}
	for (const count of counts.values()) {
		expect(count).toBeLessThanOrEqual(1);
	}
}

function assertPersistedSupersedesTmpByClientId(
	merged: DisplayMessage[],
	server: DisplayMessage[],
): void {
	for (const serverMessage of server) {
		if (!serverMessage.clientId) continue;
		const withClient = merged.filter(
			(m) => m.clientId === serverMessage.clientId,
		);
		expect(withClient.length).toBe(1);
		expect(withClient[0]?.id).toBe(serverMessage.id);
	}
}

function assertNoUnexpectedDropOnPartial(
	previous: DisplayMessage[],
	mergedPartial: DisplayMessage[],
	server: DisplayMessage[],
): void {
	const serverClientIds = new Set(
		server
			.map((m) => m.clientId)
			.filter((c): c is string => typeof c === "string" && c.length > 0),
	);

	for (const prev of previous) {
		if (!shouldPreserveLocalMessage(prev)) continue;
		const shouldSurvive =
			Boolean(prev.isStreaming) ||
			!prev.clientId ||
			!serverClientIds.has(prev.clientId);
		if (!shouldSurvive) continue;
		const survives =
			mergedPartial.some((m) => m.id === prev.id) ||
			(Boolean(prev.clientId) &&
				mergedPartial.some((m) => m.clientId === prev.clientId));
		expect(survives).toBe(true);
	}
}

function cloneMessages(messages: DisplayMessage[]): DisplayMessage[] {
	return messages.map((m) => ({ ...m, parts: m.parts.map((p) => ({ ...p })) }));
}

function text(
	id: string,
	role: DisplayMessage["role"],
	content: string,
	timestamp: number,
	extra?: Partial<DisplayMessage>,
): DisplayMessage {
	return {
		id,
		role,
		timestamp,
		parts: [{ type: "text", id: `p-${id}`, text: content }],
		...extra,
	};
}

class SeededRng {
	private state: number;

	constructor(seed: number) {
		this.state = seed >>> 0;
	}

	nextU32(): number {
		// LCG constants from Numerical Recipes (deterministic across platforms)
		this.state = (1664525 * this.state + 1013904223) >>> 0;
		return this.state;
	}

	nextInt(maxExclusive: number): number {
		if (maxExclusive <= 0) return 0;
		return this.nextU32() % maxExclusive;
	}

	chance(numerator: number, denominator: number): boolean {
		return this.nextInt(denominator) < numerator;
	}
}

function generatePreviousTimeline(seed: number, count = 8): DisplayMessage[] {
	const rng = new SeededRng(seed);
	const messages: DisplayMessage[] = [];
	let ts = 1000;
	for (let i = 0; i < count; i++) {
		const role: DisplayMessage["role"] = rng.chance(1, 2)
			? "user"
			: "assistant";
		const domain = rng.nextInt(4);
		const id =
			domain === 0
				? `tmp:${seed}-${i}`
				: domain === 1
					? `pi-msg-${seed * 100 + i}`
					: `history-${seed}-${i}`;
		const clientId =
			role === "user" && rng.chance(2, 3) ? `c-${seed}-${i}` : undefined;
		messages.push(
			text(id, role, `${role}-${seed}-${i}`, ts, {
				clientId,
				isStreaming: id.startsWith("tmp:") && rng.chance(1, 3),
			}),
		);
		ts += 1000 + rng.nextInt(250);
	}
	return messages;
}

function generateServerSnapshot(
	previous: DisplayMessage[],
	seed: number,
): DisplayMessage[] {
	const rng = new SeededRng(seed ^ 0x9e3779b9);
	const out: DisplayMessage[] = [];
	let ts = 2000;

	for (const prev of previous) {
		if (rng.chance(1, 4)) continue;
		const persistedId =
			prev.id.startsWith("tmp:") || prev.id.startsWith("pi-msg-")
				? `history-${prev.id.replace(/[^a-zA-Z0-9]+/g, "-")}`
				: prev.id;
		out.push({
			...prev,
			id: persistedId,
			timestamp: ts,
			isStreaming: false,
			parts: prev.parts.map((p) => ({ ...p })),
		});
		ts += 1000 + rng.nextInt(200);
	}

	if (rng.chance(1, 2)) {
		out.push(
			text(`history-new-${seed}`, "assistant", `new-${seed}`, ts, {
				clientId: undefined,
			}),
		);
	}

	return out;
}

describe("message merge property tests", () => {
	it("runs deterministic seeded merge invariants", () => {
		for (let seed = 1; seed <= 40; seed++) {
			const previous = generatePreviousTimeline(seed, 10);
			const server = generateServerSnapshot(previous, seed + 17);

			const partial = mergeServerMessages(
				cloneMessages(previous),
				cloneMessages(server),
				"partial",
			);
			assertNoDuplicateIds(partial);
			assertSingleMessagePerClientId(partial);
			assertPersistedSupersedesTmpByClientId(partial, server);
			assertNoUnexpectedDropOnPartial(previous, partial, server);

			const authoritative = mergeServerMessages(
				cloneMessages(previous),
				cloneMessages(server),
				"authoritative",
			);
			assertNoDuplicateIds(authoritative);
			assertSingleMessagePerClientId(authoritative);
			assertPersistedSupersedesTmpByClientId(authoritative, server);

			const authoritativeAgain = mergeServerMessages(
				cloneMessages(authoritative),
				cloneMessages(server),
				"authoritative",
			);
			expect(authoritativeAgain).toEqual(authoritative);
		}
	});

	it("preserves tool_call/tool_result details when replay payload omits fields", () => {
		for (let seed = 100; seed < 115; seed++) {
			const toolCallId = `call-${seed}`;
			const local: DisplayMessage[] = [
				{
					id: `history-assistant-${seed}`,
					role: "assistant",
					timestamp: 1000,
					parts: [
						{
							type: "tool_call",
							id: `p-call-${seed}`,
							toolCallId,
							name: "bash",
							input: { command: `echo ${seed}` },
							status: "success",
						},
						{
							type: "tool_result",
							id: `p-result-${seed}`,
							toolCallId,
							name: "bash",
							output: `ok-${seed}`,
							isError: false,
						},
					],
				},
			];

			const replay: DisplayMessage[] = [
				{
					id: `history-assistant-${seed}`,
					role: "assistant",
					timestamp: 2000,
					parts: [
						{
							type: "tool_call",
							id: `p-call-replay-${seed}`,
							toolCallId,
							name: "bash",
							input: undefined,
							status: "success",
						},
						{
							type: "tool_result",
							id: `p-result-replay-${seed}`,
							toolCallId,
							name: "bash",
							output: undefined,
							isError: false,
						},
					],
				},
			];

			for (const mode of ["partial", "authoritative"] as const) {
				const merged = mergeServerMessages(
					cloneMessages(local),
					cloneMessages(replay),
					mode,
				);
				const assistant = merged.find(
					(m) => m.id === `history-assistant-${seed}`,
				);
				expect(assistant).toBeDefined();
				const mergedCall = assistant?.parts.find(
					(p) => p.type === "tool_call" && p.toolCallId === toolCallId,
				);
				const mergedResult = assistant?.parts.find(
					(p) => p.type === "tool_result" && p.toolCallId === toolCallId,
				);
				expect(mergedCall).toMatchObject({
					type: "tool_call",
					name: "bash",
					input: { command: `echo ${seed}` },
				});
				expect(mergedResult).toMatchObject({
					type: "tool_result",
					name: "bash",
					output: `ok-${seed}`,
				});
			}
		}
	});

	it("normalizes malformed/legacy payloads deterministically", () => {
		const raw: RawMessage[] = [
			{
				role: "assistant",
				content: JSON.stringify([
					{ type: "text", text: "hello" },
					{
						arguments: { command: "pwd" },
						id: "tool-1",
						name: "bash",
					},
				]),
				timestamp: 1000,
			},
			{
				role: "toolResult",
				parts_json: JSON.stringify([
					{
						type: "tool_result",
						toolCallId: "tool-1",
						name: "bash",
						output: "ok",
						isError: false,
					},
				]),
				timestamp: 1001,
			},
			{
				role: "assistant",
				content: "not json",
				timestamp: 1002,
			},
			{ role: "assistant", content: "fallback id", timestamp: 1003 },
			{ role: "assistant", content: "fallback id", timestamp: 1003 },
		];

		const first = normalizeMessages(raw, "legacy");
		const second = normalizeMessages(raw, "legacy");

		const stripPartIds = (messages: DisplayMessage[]) =>
			messages.map((m) => ({
				...m,
				parts: m.parts.map((p) => {
					if ("id" in p) {
						const { id: _ignored, ...rest } = p;
						return rest;
					}
					return p;
				}),
			}));

		expect(stripPartIds(first)).toEqual(stripPartIds(second));
		expect(first[0]?.parts.some((p) => p.type === "tool_call")).toBe(true);
		expect(first[0]?.parts.some((p) => p.type === "tool_result")).toBe(true);
		expect(first[1]?.parts[0]).toMatchObject({
			type: "text",
			text: "not json",
		});
		assertNoDuplicateIds(first);
	});

	it("replays historical regression seed from fixture", () => {
		const fixture = loadFixture("2026-04-14-finalized-tmp-assistant-drop.json");
		const partial = mergeServerMessages(
			cloneMessages(fixture.previous),
			cloneMessages(fixture.partialServer),
			"partial",
		);
		expect(partial.map((m) => m.id)).toEqual(fixture.expectedPartialIds);

		const authoritative = mergeServerMessages(
			cloneMessages(partial),
			cloneMessages(fixture.authoritativeServer),
			"authoritative",
		);
		expect(authoritative.map((m) => m.id)).toEqual(
			fixture.expectedAuthoritativeIds,
		);
	});
});
