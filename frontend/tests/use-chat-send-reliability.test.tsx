import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

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

vi.mock("@/lib/ws-manager", () => ({
	getWsManager: () => managerMock,
}));

vi.mock("@/components/contexts", () => ({
	useBusySessions: () => ({ setSessionBusy: vi.fn() }),
}));

import { useChat } from "@/features/chat/hooks/useChat";

describe("useChat send reliability", () => {
	let sessionHandler: ((event: Record<string, unknown>) => void) | null = null;

	beforeEach(() => {
		vi.clearAllMocks();
		sessionHandler = null;
		managerMock.isSessionReady.mockReturnValue(false);
		managerMock.subscribeAgentSession.mockImplementation((_, handler) => {
			sessionHandler = handler as (event: Record<string, unknown>) => void;
			return () => {
				sessionHandler = null;
			};
		});
		managerMock.ensureConnected.mockRejectedValue(new Error("ws down"));
		managerMock.waitForSessionReady.mockRejectedValue(
			new Error("session not ready"),
		);
		managerMock.agentGetStateWait.mockRejectedValue(new Error("not found"));
		managerMock.agentListSessions.mockResolvedValue([]);
	});

	it("does not reject send when connection/readiness checks fail transiently", async () => {
		const { result } = renderHook(() =>
			useChat({
				autoConnect: false,
				selectedSessionId: "sess-1",
				workspacePath: "/tmp/ws",
			}),
		);

		await expect(
			act(async () => {
				await result.current.send("hello", {
					mode: "steer",
					sessionId: "sess-1",
				});
			}),
		).resolves.toBeUndefined();

		// Core guarantee: transient readiness/connectivity failures must not reject
		// send(), otherwise ChatView restores the draft and the user perceives a
		// dropped message.
		expect(managerMock.ensureConnected).toHaveBeenCalled();
	});

	it("dedupes duplicate tool lifecycle events while streaming", async () => {
		const { result } = renderHook(() =>
			useChat({
				autoConnect: false,
				selectedSessionId: "sess-1",
				workspacePath: "/tmp/ws",
			}),
		);

		await waitFor(() => expect(sessionHandler).not.toBeNull());
		const emit = (event: Record<string, unknown>) => {
			act(() => {
				sessionHandler?.(event);
			});
		};

		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "stream.message_start",
			role: "assistant",
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "stream.tool_call_start",
			tool_call_id: "call-a",
			name: "bash",
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "stream.tool_call_start",
			tool_call_id: "call-b",
			name: "bash",
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.start",
			tool_call_id: "call-c",
			name: "bash",
			input: { command: "ls" },
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.end",
			tool_call_id: "call-d",
			name: "bash",
			output: "ok",
			is_error: false,
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.end",
			tool_call_id: "call-e",
			name: "bash",
			output: "ok-again",
			is_error: false,
		});

		const assistant = result.current.messages.find(
			(m) => m.role === "assistant",
		);
		expect(assistant).toBeDefined();
		const toolCalls =
			assistant?.parts.filter((part) => part.type === "tool_call") ?? [];
		const toolResults =
			assistant?.parts.filter((part) => part.type === "tool_result") ?? [];
		expect(toolCalls).toHaveLength(1);
		expect(toolResults).toHaveLength(1);
	});
});
