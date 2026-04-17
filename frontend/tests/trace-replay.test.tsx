import fs from "node:fs";
import path from "node:path";

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DisplayMessage, RawMessage } from "@/features/chat/hooks/types";

const managerMock = {
	isSessionReady: vi.fn(),
	subscribeAgentSession: vi.fn(),
	ensureConnected: vi.fn(),
	waitForSessionReady: vi.fn(),
	agentGetStateWait: vi.fn(),
	agentListSessions: vi.fn(),
	agentPrompt: vi.fn(),
	agentSteer: vi.fn(),
	agentFollowUp: vi.fn(),
	agentAbort: vi.fn(),
	agentCompact: vi.fn(),
	agentCloseSession: vi.fn(),
	agentCreateSession: vi.fn(),
	agentGetState: vi.fn(),
	agentGetMessages: vi.fn(),
	connect: vi.fn(),
	disconnect: vi.fn(),
	send: vi.fn(),
	onResync: vi.fn(() => () => {}),
	removeResync: vi.fn(),
	onConnectionState: vi.fn(() => () => {}),
	onChannel: vi.fn(() => () => {}),
	onGlobal: vi.fn(() => () => {}),
	state: "connected",
	isConnected: true,
};

const getChatMessagesMock =
	vi.fn<(sessionId: string, workspaceId?: string) => Promise<RawMessage[]>>();
const triggerChatHistoryBackfillMock = vi.fn(async () => ({}));

vi.mock("@/lib/ws-manager", () => ({
	getWsManager: () => managerMock,
}));

vi.mock("@/components/contexts", () => ({
	useBusySessions: () => ({ setSessionBusy: vi.fn() }),
}));

vi.mock("@/lib/api/chat", () => ({
	getChatMessages: (...args: [string, string?]) => getChatMessagesMock(...args),
	triggerChatHistoryBackfill: (...args: unknown[]) =>
		triggerChatHistoryBackfillMock(...args),
}));

import { useChat } from "@/features/chat/hooks/useChat";

type TraceStep =
	| { type: "event"; event: Record<string, unknown> }
	| { type: "advance"; ms: number }
	| { type: "switch_session"; sessionId: string };

type ExpectedMessage = {
	id: string;
	role: "user" | "assistant" | "system";
	text: string;
};

type TraceFixture = {
	id: string;
	description: string;
	initialSessionId: string;
	initialHistory?: RawMessage[];
	historyFetches?: RawMessage[][];
	steps: TraceStep[];
	expected: ExpectedMessage[];
};

function fixturePath(): string {
	return path.resolve(
		__dirname,
		"fixtures/traces/persistence-historical-traces.json",
	);
}

function loadFixtures(): TraceFixture[] {
	const raw = fs.readFileSync(fixturePath(), "utf8");
	return JSON.parse(raw) as TraceFixture[];
}

function summarizeMessages(messages: DisplayMessage[]): ExpectedMessage[] {
	return messages.map((m) => ({
		id: m.id,
		role: m.role,
		text: m.parts
			.filter((part) => part.type === "text")
			.map((part) => part.text)
			.join(""),
	}));
}

function assertNoDuplicateIds(messages: DisplayMessage[]): void {
	const ids = messages.map((m) => m.id);
	expect(new Set(ids).size).toBe(ids.length);
}

function assertNoDuplicateClientTurns(messages: DisplayMessage[]): void {
	const counts = new Map<string, number>();
	for (const msg of messages) {
		if (!msg.clientId) continue;
		counts.set(msg.clientId, (counts.get(msg.clientId) ?? 0) + 1);
	}
	for (const count of counts.values()) {
		expect(count).toBeLessThanOrEqual(1);
	}
}

function remapFixtureSessionIds(fixture: TraceFixture): TraceFixture {
	const mapping = new Map<string, string>();
	const mapSessionId = (sessionId: string): string => {
		if (!mapping.has(sessionId)) {
			mapping.set(sessionId, `${fixture.id}::${sessionId}`);
		}
		return mapping.get(sessionId) as string;
	};

	const remappedSteps = fixture.steps.map((step) => {
		if (step.type === "switch_session") {
			return { ...step, sessionId: mapSessionId(step.sessionId) };
		}
		if (step.type === "event") {
			const event = { ...step.event };
			if (typeof event.session_id === "string") {
				event.session_id = mapSessionId(event.session_id);
			}
			return { ...step, event };
		}
		return step;
	});

	return {
		...fixture,
		initialSessionId: mapSessionId(fixture.initialSessionId),
		steps: remappedSteps,
	};
}

describe("trace replay harness (canonical incidents)", () => {
	let sessionHandler: ((event: Record<string, unknown>) => void) | null = null;
	const fixtures = loadFixtures();

	beforeEach(() => {
		vi.clearAllMocks();
		sessionHandler = null;
		managerMock.isSessionReady.mockReturnValue(false);
		managerMock.ensureConnected.mockResolvedValue(undefined);
		managerMock.waitForSessionReady.mockResolvedValue(undefined);
		managerMock.agentGetStateWait.mockResolvedValue({
			sessionId: "sess-a",
			isStreaming: false,
		});
		managerMock.agentListSessions.mockResolvedValue([]);
		managerMock.subscribeAgentSession.mockImplementation((_, handler) => {
			sessionHandler = handler as (event: Record<string, unknown>) => void;
			return () => {
				sessionHandler = null;
			};
		});
		getChatMessagesMock.mockResolvedValue([]);
	});

	it("loads >=10 historical traces", () => {
		expect(fixtures.length).toBeGreaterThanOrEqual(10);
	});

	it.each(fixtures)("replays trace $id", async (fixtureInput) => {
		const fixture = remapFixtureSessionIds(fixtureInput);
		let activeSessionId = fixture.initialSessionId;
		let historyCall = 0;
		const seeded = fixture.initialHistory ?? [];
		const historyFetches = fixture.historyFetches ?? [];

		getChatMessagesMock.mockImplementation(async () => {
			const callIndex = historyCall;
			historyCall += 1;
			if (callIndex === 0) return seeded;
			return historyFetches[callIndex - 1] ?? historyFetches.at(-1) ?? [];
		});

		const { result, rerender } = renderHook(
			(props: { selectedSessionId: string }) =>
				useChat({
					autoConnect: false,
					selectedSessionId: props.selectedSessionId,
					workspacePath: "/tmp/ws",
				}),
			{
				initialProps: { selectedSessionId: activeSessionId },
			},
		);

		await waitFor(() => expect(sessionHandler).not.toBeNull());

		for (const step of fixture.steps) {
			if (step.type === "event") {
				act(() => {
					sessionHandler?.(step.event);
				});
				continue;
			}

			if (step.type === "switch_session") {
				activeSessionId = step.sessionId;
				rerender({ selectedSessionId: activeSessionId });
				await waitFor(() => expect(sessionHandler).not.toBeNull());
				continue;
			}

			await act(async () => {
				await new Promise((resolve) => setTimeout(resolve, step.ms));
			});
		}

		const finalMessages = result.current.messages;
		assertNoDuplicateIds(finalMessages);
		assertNoDuplicateClientTurns(finalMessages);

		for (let i = 1; i < finalMessages.length; i++) {
			expect(finalMessages[i].timestamp).toBeGreaterThanOrEqual(
				finalMessages[i - 1].timestamp,
			);
		}

		if (fixture.id.includes("session-switch")) {
			expect(finalMessages.some((m) => m.id.startsWith("a-"))).toBe(false);
		}

		expect(
			summarizeMessages(finalMessages),
			`${fixture.id}: ${fixture.description}`,
		).toEqual(fixture.expected);
	});
});
