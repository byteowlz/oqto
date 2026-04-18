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
			turnKind: "streaming",
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

	it("applies authoritative merge mode for authoritative get_messages payloads", () => {
		const applyServerMessages = vi.fn();

		dispatchResponseCommand({
			resp: {
				id: "resp-2",
				cmd: "get_messages",
				success: true,
				data: {
					messages: [
						{
							id: "m-1",
							session_id: "sess-1",
							role: "assistant",
							parts: [],
							created_at: Date.now(),
						},
					],
					messages_source: "authoritative",
				},
			},
			eventSessionId: "sess-1",
			activeSessionId: "sess-1",
			isDebug: false,
			turnKind: "idle",
			setState: vi.fn(),
			bindSessionIdentity: vi.fn(),
			applyTurnState: vi.fn(),
			setBusyForEvent: vi.fn(),
			setError: vi.fn(),
			onError: vi.fn(),
			fetchHistoryMessages: vi.fn(async () => {}),
			applyServerMessages,
			persistedMessageVersionRef: { current: null },
			streamingMessageRef: { current: null },
			isStreamingRef: { current: false },
			sendInFlightRef: { current: false },
			setSharedWorkspaceSessionId: vi.fn(),
			clearSharedWorkspaceSessionId: vi.fn(),
			recoverSessionOnError: vi.fn(),
		});

		expect(applyServerMessages).toHaveBeenCalledWith(
			expect.any(Array),
			"sess-1",
			undefined,
			"authoritative",
		);
	});

	it("keeps live get_messages payloads in partial mode", () => {
		const applyServerMessages = vi.fn();

		dispatchResponseCommand({
			resp: {
				id: "resp-3",
				cmd: "get_messages",
				success: true,
				data: {
					messages: [
						{
							id: "m-1",
							session_id: "sess-1",
							role: "assistant",
							parts: [],
							created_at: Date.now(),
						},
					],
					messages_source: "live",
				},
			},
			eventSessionId: "sess-1",
			activeSessionId: "sess-1",
			isDebug: false,
			turnKind: "idle",
			setState: vi.fn(),
			bindSessionIdentity: vi.fn(),
			applyTurnState: vi.fn(),
			setBusyForEvent: vi.fn(),
			setError: vi.fn(),
			onError: vi.fn(),
			fetchHistoryMessages: vi.fn(async () => {}),
			applyServerMessages,
			persistedMessageVersionRef: { current: null },
			streamingMessageRef: { current: null },
			isStreamingRef: { current: false },
			sendInFlightRef: { current: false },
			setSharedWorkspaceSessionId: vi.fn(),
			clearSharedWorkspaceSessionId: vi.fn(),
			recoverSessionOnError: vi.fn(),
		});

		expect(applyServerMessages).toHaveBeenCalledWith(
			expect.any(Array),
			"sess-1",
			undefined,
			"partial",
		);
	});

	it("ignores live get_messages snapshots while a live turn is active", () => {
		const applyServerMessages = vi.fn();

		dispatchResponseCommand({
			resp: {
				id: "resp-4",
				cmd: "get_messages",
				success: true,
				data: {
					messages: [
						{
							id: "m-1",
							session_id: "sess-1",
							role: "assistant",
							parts: [],
							created_at: Date.now(),
						},
					],
					messages_source: "live",
				},
			},
			eventSessionId: "sess-1",
			activeSessionId: "sess-1",
			isDebug: false,
			turnKind: "streaming",
			setState: vi.fn(),
			bindSessionIdentity: vi.fn(),
			applyTurnState: vi.fn(),
			setBusyForEvent: vi.fn(),
			setError: vi.fn(),
			onError: vi.fn(),
			fetchHistoryMessages: vi.fn(async () => {}),
			applyServerMessages,
			persistedMessageVersionRef: { current: null },
			streamingMessageRef: { current: null },
			isStreamingRef: { current: true },
			sendInFlightRef: { current: false },
			setSharedWorkspaceSessionId: vi.fn(),
			clearSharedWorkspaceSessionId: vi.fn(),
			recoverSessionOnError: vi.fn(),
		});

		expect(applyServerMessages).not.toHaveBeenCalled();
	});
});
