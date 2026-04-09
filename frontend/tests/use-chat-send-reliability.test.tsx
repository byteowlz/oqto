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

	it("dedupes repeated tool lifecycle events for the same tool_call_id", async () => {
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
			tool_call_id: "call-1",
			name: "bash",
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.start",
			tool_call_id: "call-1",
			name: "bash",
			input: { command: "ls" },
		});
		// Duplicate start replay for same tool_call_id should be idempotent.
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.start",
			tool_call_id: "call-1",
			name: "bash",
			input: { command: "ls" },
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.end",
			tool_call_id: "call-1",
			name: "bash",
			output: "ok",
			is_error: false,
		});
		// Duplicate end replay for same tool_call_id should update, not append.
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.end",
			tool_call_id: "call-1",
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
		expect(toolResults[0]).toMatchObject({ output: "ok-again" });
	});

	it("preserves existing tool input when replayed tool.start omits input", async () => {
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
			event: "tool.start",
			tool_call_id: "call-keep-input",
			name: "bash",
			input: { command: 'rg -n "cargo install --path" justfile' },
		});
		// Replay/out-of-order update without input should not erase known input.
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.start",
			tool_call_id: "call-keep-input",
			name: "bash",
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.end",
			tool_call_id: "call-keep-input",
			name: "bash",
			output: "ok",
			is_error: false,
		});

		const toolCall = result.current.messages
			.flatMap((m) => m.parts)
			.find(
				(part) =>
					part.type === "tool_call" && part.toolCallId === "call-keep-input",
			);
		expect(toolCall).toBeDefined();
		if (toolCall?.type === "tool_call") {
			expect(toolCall.input).toEqual({
				command: 'rg -n "cargo install --path" justfile',
			});
		}
	});

	it("preserves existing tool input when stream.tool_call_end omits input", async () => {
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
			event: "tool.start",
			tool_call_id: "call-keep-input-2",
			name: "bash",
			input: { command: "cargo check -p oqto" },
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "stream.tool_call_end",
			tool_call: { id: "call-keep-input-2", name: "bash" },
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.end",
			tool_call_id: "call-keep-input-2",
			name: "bash",
			output: "ok",
			is_error: false,
		});

		const toolCall = result.current.messages
			.flatMap((m) => m.parts)
			.find(
				(part) =>
					part.type === "tool_call" && part.toolCallId === "call-keep-input-2",
			);
		expect(toolCall).toBeDefined();
		if (toolCall?.type === "tool_call") {
			expect(toolCall.input).toEqual({ command: "cargo check -p oqto" });
		}
	});

	it("keeps recoverable retry errors in working state without durable-looking error rows", async () => {
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
			event: "retry.start",
			attempt: 1,
			max_attempts: 3,
			error: "429 Rate limit exceeded",
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "agent.error",
			error: "429 Rate limit exceeded",
			recoverable: true,
		});

		expect(result.current.error).toBeNull();
		const assistantMessages = result.current.messages.filter(
			(m) => m.role === "assistant",
		);
		const assistant = assistantMessages[assistantMessages.length - 1];
		expect(assistant).toBeDefined();
		expect(assistant?.isStreaming).toBe(true);
		const errorParts = assistant?.parts.filter((part) => part.type === "error") ?? [];
		expect(errorParts).toHaveLength(0);
	});

	it("replaces working bubble with inline terminal error part on non-recoverable agent.error", async () => {
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
			event: "agent.error",
			error: "Internal server error during generation",
			recoverable: false,
		});

		expect(result.current.isStreaming).toBe(false);
		const assistantMessages = result.current.messages.filter(
			(m) => m.role === "assistant",
		);
		const assistant = assistantMessages[assistantMessages.length - 1];
		expect(assistant).toBeDefined();
		const errorParts = assistant?.parts.filter((part) => part.type === "error") ?? [];
		expect(errorParts.length).toBeGreaterThan(0);
		expect(errorParts[0]).toMatchObject({
			type: "error",
			text: "Internal server error during generation",
		});
	});

	it("keeps distinct tool calls separate when ids differ but names match", async () => {
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
			event: "tool.start",
			tool_call_id: "call-a",
			name: "bash",
			input: { command: "ls" },
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.end",
			tool_call_id: "call-a",
			name: "bash",
			output: "ok-a",
			is_error: false,
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.start",
			tool_call_id: "call-b",
			name: "bash",
			input: { command: "pwd" },
		});
		emit({
			channel: "agent",
			session_id: "sess-1",
			event: "tool.end",
			tool_call_id: "call-b",
			name: "bash",
			output: "ok-b",
			is_error: false,
		});

		const assistantMessages = result.current.messages.filter(
			(m) => m.role === "assistant",
		);
		expect(assistantMessages.length).toBeGreaterThan(0);
		const toolCalls = assistantMessages.flatMap((message) =>
			message.parts.filter((part) => part.type === "tool_call"),
		);
		const toolResults = assistantMessages.flatMap((message) =>
			message.parts.filter((part) => part.type === "tool_result"),
		);
		const toolCallIds = new Set(
			toolCalls.map((part) =>
				part.type === "tool_call" ? part.toolCallId : undefined,
			),
		);
		const toolResultIds = new Set(
			toolResults.map((part) =>
				part.type === "tool_result" ? part.toolCallId : undefined,
			),
		);
		expect(toolCallIds.has("call-a")).toBe(true);
		expect(toolCallIds.has("call-b")).toBe(true);
		expect(toolResultIds.has("call-a")).toBe(true);
		expect(toolResultIds.has("call-b")).toBe(true);
	});
});
