import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const managerMock = {
	isSessionReady: vi.fn(() => false),
	subscribeAgentSession: vi.fn(() => () => {}),
	ensureConnected: vi.fn(() => Promise.resolve()),
	waitForSessionReady: vi.fn(() => Promise.reject(new Error("not ready"))),
	agentGetStateWait: vi.fn(() => Promise.reject(new Error("not found"))),
	agentListSessions: vi.fn(() => Promise.resolve([])),
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

vi.mock("@/lib/ws-manager", () => ({ getWsManager: () => managerMock }));
vi.mock("@/components/contexts", () => ({
	useBusySessions: () => ({ setSessionBusy: vi.fn() }),
}));

const getChatMessages = vi.fn();
vi.mock("@/lib/api/chat", async (importOriginal) => ({
	...(await importOriginal<typeof import("@/lib/api/chat")>()),
	getChatMessages: (...args: unknown[]) => getChatMessages(...args),
}));

import { useChat } from "@/features/chat/hooks/useChat";

describe("useChat history load failure", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("reports a failed history load instead of an empty chat, and recovers on retry", async () => {
		getChatMessages.mockRejectedValueOnce(new Error("500"));
		const { result } = renderHook(() =>
			useChat({
				autoConnect: false,
				selectedSessionId: "oqto-large",
				workspacePath: "/tmp/ws",
			}),
		);

		await act(async () => {
			await result.current.refresh();
		});
		await waitFor(() => expect(result.current.historyLoadFailed).toBe(true));
		expect(result.current.messages).toHaveLength(0);

		getChatMessages.mockResolvedValueOnce([
			{
				id: "m1",
				role: "user",
				parts: [{ type: "text", id: "p1", text: "kept" }],
				created_at: 1,
			},
		]);
		await act(async () => {
			await result.current.refresh();
		});
		await waitFor(() => expect(result.current.historyLoadFailed).toBe(false));
	});
});
