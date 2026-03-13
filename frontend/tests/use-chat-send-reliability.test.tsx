import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const managerMock = {
	isSessionReady: vi.fn(),
	subscribeAgentSession: vi.fn(),
	ensureConnected: vi.fn(),
	waitForSessionReady: vi.fn(),
	agentGetStateWait: vi.fn(),
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
	beforeEach(() => {
		vi.clearAllMocks();
		managerMock.isSessionReady.mockReturnValue(false);
		managerMock.subscribeAgentSession.mockReturnValue(() => {});
		managerMock.ensureConnected.mockRejectedValue(new Error("ws down"));
		managerMock.waitForSessionReady.mockRejectedValue(
			new Error("session not ready"),
		);
		managerMock.agentGetStateWait.mockRejectedValue(new Error("not found"));
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
});
