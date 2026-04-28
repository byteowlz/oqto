import {
	type MergeMode,
	mergeServerMessages,
	shouldPreserveLocalMessage,
} from "@/features/chat/hooks/message-utils";
import type { DisplayMessage } from "@/features/chat/hooks/types";
import { describe, expect, it } from "vitest";

type TurnKind = "idle" | "sending" | "streaming" | "syncing";

type HarnessState = {
	activeSession: string;
	transportEpoch: number;
	turn: TurnKind;
	sessions: Record<string, DisplayMessage[]>;
};

type FetchOp = {
	type: "fetch";
	sessionId: string;
	epoch: number;
	mode: MergeMode;
	server: DisplayMessage[];
};

type Op =
	| { type: "set_turn"; turn: TurnKind }
	| { type: "switch_session"; sessionId: string }
	| { type: "reconnect" }
	| FetchOp;

function msg(
	id: string,
	role: DisplayMessage["role"],
	text: string,
	timestamp: number,
	extra?: Partial<DisplayMessage>,
): DisplayMessage {
	return {
		id,
		role,
		parts: [{ type: "text", id: `p-${id}`, text }],
		timestamp,
		...extra,
	};
}

function permutations<T>(items: readonly T[]): T[][] {
	if (items.length <= 1) return [Array.from(items)];
	const out: T[][] = [];
	items.forEach((item, index) => {
		const rest = [...items.slice(0, index), ...items.slice(index + 1)];
		for (const perm of permutations(rest)) {
			out.push([item, ...perm]);
		}
	});
	return out;
}

function cloneMessages(messages: DisplayMessage[]): DisplayMessage[] {
	return messages.map((m) => ({ ...m, parts: m.parts.map((p) => ({ ...p })) }));
}

function applyOp(state: HarnessState, op: Op): HarnessState {
	switch (op.type) {
		case "set_turn":
			return { ...state, turn: op.turn };
		case "switch_session":
			return { ...state, activeSession: op.sessionId };
		case "reconnect":
			return { ...state, transportEpoch: state.transportEpoch + 1 };
		case "fetch": {
			if (op.sessionId !== state.activeSession) return state;
			if (op.epoch !== state.transportEpoch) return state;
			const current = state.sessions[op.sessionId] ?? [];
			const nextMessages = mergeServerMessages(current, op.server, op.mode);
			return {
				...state,
				sessions: {
					...state.sessions,
					[op.sessionId]: nextMessages,
				},
			};
		}
	}
}

function assertNoDuplicateIds(messages: DisplayMessage[]): void {
	const ids = messages.map((m) => m.id);
	expect(new Set(ids).size).toBe(ids.length);
}

function assertSingleMessagePerClientId(messages: DisplayMessage[]): void {
	const counts = new Map<string, number>();
	for (const m of messages) {
		if (!m.clientId) continue;
		counts.set(m.clientId, (counts.get(m.clientId) ?? 0) + 1);
	}
	for (const count of counts.values()) {
		expect(count).toBeLessThanOrEqual(1);
	}
}

function assertTimestampStable(messages: DisplayMessage[]): void {
	for (let i = 1; i < messages.length; i++) {
		expect(messages[i].timestamp).toBeGreaterThanOrEqual(
			messages[i - 1].timestamp,
		);
	}
}

function assertNoUnexpectedDropOnPartial(
	previous: DisplayMessage[],
	next: DisplayMessage[],
	server: DisplayMessage[],
): void {
	const serverClientIds = new Set(
		server
			.map((m) => m.clientId)
			.filter((c): c is string => typeof c === "string" && c.length > 0),
	);

	for (const prevMsg of previous) {
		if (!shouldPreserveLocalMessage(prevMsg)) continue;
		const shouldSurvive =
			Boolean(prevMsg.isStreaming) ||
			!prevMsg.clientId ||
			!serverClientIds.has(prevMsg.clientId);
		if (!shouldSurvive) continue;
		const exists =
			next.some((m) => m.id === prevMsg.id) ||
			(Boolean(prevMsg.clientId) &&
				next.some((m) => m.clientId === prevMsg.clientId));
		expect(exists).toBe(true);
	}
}

function runScenario(initial: HarnessState, ops: Op[]): HarnessState {
	let state = {
		...initial,
		sessions: Object.fromEntries(
			Object.entries(initial.sessions).map(([id, messages]) => [
				id,
				cloneMessages(messages),
			]),
		),
	};

	for (const op of ops) {
		const prevActiveMessages = state.sessions[state.activeSession] ?? [];
		const next = applyOp(state, op);
		const nextActiveMessages = next.sessions[next.activeSession] ?? [];

		assertNoDuplicateIds(nextActiveMessages);
		assertSingleMessagePerClientId(nextActiveMessages);
		assertTimestampStable(nextActiveMessages);

		if (op.type === "fetch" && op.mode === "partial") {
			const wasApplied =
				op.sessionId === state.activeSession &&
				op.epoch === state.transportEpoch;
			if (wasApplied) {
				assertNoUnexpectedDropOnPartial(
					prevActiveMessages,
					nextActiveMessages,
					op.server,
				);
			}
		}

		state = next;
	}

	return state;
}

describe("message interleaving regression harness", () => {
	it("covers >= 25 deterministic interleaving cases", () => {
		const window1Base: HarnessState = {
			activeSession: "A",
			transportEpoch: 1,
			turn: "syncing",
			sessions: {
				A: [
					msg("h-u1", "user", "question", 1000, { clientId: "c-1" }),
					msg("tmp:a1", "assistant", "draft answer", 2000, {
						isStreaming: false,
					}),
				],
			},
		};
		const w1StalePartial: FetchOp = {
			type: "fetch",
			sessionId: "A",
			epoch: 1,
			mode: "partial",
			server: [msg("h-u1", "user", "question", 1000, { clientId: "c-1" })],
		};
		const w1PersistedPartial: FetchOp = {
			type: "fetch",
			sessionId: "A",
			epoch: 1,
			mode: "partial",
			server: [
				msg("h-u1", "user", "question", 1000, { clientId: "c-1" }),
				msg("h-a1", "assistant", "final answer", 3000),
			],
		};
		const w1IdleAuth: FetchOp = {
			type: "fetch",
			sessionId: "A",
			epoch: 1,
			mode: "authoritative",
			server: [
				msg("h-u1", "user", "question", 1000, { clientId: "c-1" }),
				msg("h-a1", "assistant", "final answer", 3000),
			],
		};

		const window1Permutations = permutations([
			w1StalePartial,
			w1PersistedPartial,
			w1IdleAuth,
		]);

		for (const [idx, ops] of window1Permutations.entries()) {
			const final = runScenario(window1Base, ops);
			expect(
				final.sessions.A.map((m) => m.id),
				`window1 case ${idx + 1}`,
			).toEqual(["h-u1", "h-a1"]);
		}

		const turns: TurnKind[] = ["sending", "streaming", "syncing"];
		const snapshotVariants: Array<{
			name: string;
			server: DisplayMessage[];
		}> = [
			{
				name: "old-history-only",
				server: [
					msg("h-u0", "user", "old q", 1000),
					msg("h-a0", "assistant", "old a", 2000),
				],
			},
			{
				name: "assistant-only-partial",
				server: [msg("h-a0", "assistant", "old a", 2000)],
			},
			{
				name: "includes-persisted-user",
				server: [
					msg("h-u0", "user", "old q", 1000),
					msg("h-a0", "assistant", "old a", 2000),
					msg("h-u1", "user", "new q", 3000, { clientId: "c-new" }),
				],
			},
		];

		let window2Count = 0;
		for (const turn of turns) {
			for (const snapshot of snapshotVariants) {
				window2Count += 1;
				const initial: HarnessState = {
					activeSession: "A",
					transportEpoch: 1,
					turn,
					sessions: {
						A: [
							msg("h-u0", "user", "old q", 1000),
							msg("h-a0", "assistant", "old a", 2000),
							msg("tmp:u1", "user", "new q", 3000, { clientId: "c-new" }),
						],
					},
				};
				const final = runScenario(initial, [
					{ type: "set_turn", turn },
					{
						type: "fetch",
						sessionId: "A",
						epoch: 1,
						mode: "partial",
						server: snapshot.server,
					},
					{
						type: "fetch",
						sessionId: "A",
						epoch: 1,
						mode: "authoritative",
						server: [
							msg("h-u0", "user", "old q", 1000),
							msg("h-a0", "assistant", "old a", 2000),
							msg("h-u1", "user", "new q", 3000, { clientId: "c-new" }),
							msg("h-a1", "assistant", "new a", 4000),
						],
					},
				]);
				expect(
					final.sessions.A.map((m) => m.id),
					`window2 ${turn} ${snapshot.name}`,
				).toEqual(["h-u0", "h-a0", "h-u1", "h-a1"]);
			}
		}
		expect(window2Count).toBe(9);

		const staleFetchA1: FetchOp = {
			type: "fetch",
			sessionId: "A",
			epoch: 1,
			mode: "partial",
			server: [msg("h-u1", "user", "base q", 1000)],
		};
		const freshFetchA2: FetchOp = {
			type: "fetch",
			sessionId: "A",
			epoch: 2,
			mode: "authoritative",
			server: [
				msg("h-u1", "user", "base q", 1000),
				msg("h-a1", "assistant", "base a", 2000),
				msg("h-u2", "user", "new q", 3000),
			],
		};
		const staleFetchA1b: FetchOp = {
			type: "fetch",
			sessionId: "A",
			epoch: 1,
			mode: "authoritative",
			server: [msg("h-u1", "user", "base q", 1000)],
		};

		for (const [idx, perm] of permutations([
			staleFetchA1,
			freshFetchA2,
			staleFetchA1b,
		]).entries()) {
			const initial: HarnessState = {
				activeSession: "A",
				transportEpoch: 1,
				turn: "syncing",
				sessions: {
					A: [
						msg("h-u1", "user", "base q", 1000),
						msg("h-a1", "assistant", "base a", 2000),
					],
				},
			};
			const final = runScenario(initial, [{ type: "reconnect" }, ...perm]);
			expect(final.transportEpoch).toBe(2);
			expect(
				final.sessions.A.map((m) => m.id),
				`window3 case ${idx + 1}`,
			).toEqual(["h-u1", "h-a1", "h-u2"]);
		}

		const switchAndInflightPerms = permutations<FetchOp>([
			{
				type: "fetch",
				sessionId: "A",
				epoch: 1,
				mode: "authoritative",
				server: [msg("a1", "user", "a-old", 1000)],
			},
			{
				type: "fetch",
				sessionId: "B",
				epoch: 1,
				mode: "authoritative",
				server: [
					msg("b1", "user", "b-old", 1000),
					msg("b2", "assistant", "b-new", 2000),
				],
			},
			{
				type: "fetch",
				sessionId: "A",
				epoch: 1,
				mode: "partial",
				server: [msg("a2", "assistant", "a-new", 2000)],
			},
		]);

		for (const [idx, perm] of switchAndInflightPerms.entries()) {
			const initial: HarnessState = {
				activeSession: "A",
				transportEpoch: 1,
				turn: "idle",
				sessions: {
					A: [msg("a1", "user", "a-old", 1000)],
					B: [msg("b1", "user", "b-old", 1000)],
				},
			};
			const final = runScenario(initial, [
				{ type: "switch_session", sessionId: "B" },
				...perm,
			]);
			expect(final.activeSession).toBe("B");
			expect(
				final.sessions.B.map((m) => m.id),
				`window4 case ${idx + 1}`,
			).toEqual(["b1", "b2"]);
			// Session A must remain untouched after switch; stale in-flight responses for
			// non-active session are ignored.
			expect(final.sessions.A.map((m) => m.id)).toEqual(["a1"]);
		}

		const totalCases =
			window1Permutations.length +
			window2Count +
			permutations([staleFetchA1, freshFetchA2, staleFetchA1b]).length +
			switchAndInflightPerms.length;
		expect(totalCases).toBeGreaterThanOrEqual(25);
	});
});
