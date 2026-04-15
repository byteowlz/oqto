import { describe, expect, it, vi } from "vitest";

import { dispatchResponseCommand } from "@/features/chat/hooks/response-command-dispatch";

describe("dispatchResponseCommand", () => {
	it("surfaces prompt command failures as recoverable turn errors", () => {
		const applyTurnState = vi.fn();
		const setBusyForEvent = vi.fn();
		const setError = vi.fn();
		const onError = vi.fn();
		const recoverSessionOnError = vi.fn();

		dispatchResponseCommand({
			resp: {
				id: "resp-1",
				cmd: "prompt",
				success: false,
				error:
					"Error: 400 dynamic scenario requires a user message trigger like ##success",
			},
			eventSessionId: "sess-1",
			activeSessionId: "sess-1",
			isDebug: false,
			setState: vi.fn(),
			bindSessionIdentity: vi.fn(),
			applyTurnState,
			setBusyForEvent,
			setError,
			onError,
			fetchHistoryMessages: vi.fn(async () => {}),
			applyServerMessages: vi.fn(),
			persistedMessageVersionRef: { current: null },
			streamingMessageRef: { current: null },
			isStreamingRef: { current: true },
			sendInFlightRef: { current: true },
			setSharedWorkspaceSessionId: vi.fn(),
			clearSharedWorkspaceSessionId: vi.fn(),
			recoverSessionOnError,
		});

		expect(applyTurnState).toHaveBeenCalledWith({
			kind: "error",
			recoverable: true,
			message:
				"Error: 400 dynamic scenario requires a user message trigger like ##success",
		});
		expect(setBusyForEvent).toHaveBeenCalledWith("sess-1", false);
		expect(setError).toHaveBeenCalledTimes(1);
		expect(onError).toHaveBeenCalledTimes(1);
		expect(recoverSessionOnError).toHaveBeenCalledWith(
			"Error: 400 dynamic scenario requires a user message trigger like ##success",
		);
	});
});
