import type { CommandResponse } from "@/lib/canonical-types";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";

import {
	parseGetMessagesPayload,
	parseSessionCreateTarget,
	shouldFetchHistoryAfterSessionCreate,
} from "./response-command-handlers";
import type { AgentState, DisplayMessage, RawMessage } from "./types";

type TurnState =
	| { kind: "idle" }
	| { kind: "streaming" }
	| { kind: "syncing" }
	| { kind: "sending" }
	| { kind: "error"; recoverable: boolean; message: string };

export type ResponseDispatchContext = {
	resp: CommandResponse;
	eventSessionId?: string;
	activeSessionId: string | null;
	isDebug: boolean;
	/** Current turn kind from the chat state machine (idle, streaming, syncing, etc.). */
	turnKind: string;
	setState: Dispatch<SetStateAction<AgentState | null>>;
	bindSessionIdentity: (ids: { runnerId: string; piId?: string }) => void;
	applyTurnState: (state: TurnState) => void;
	setBusyForEvent: (
		sessionId: string | null | undefined,
		busy: boolean,
	) => void;
	setError: (error: Error | null) => void;
	onError?: (error: Error) => void;
	fetchHistoryMessages: (
		sessionId: string,
		expectedVersion?: number,
		opts?: { forceAuthoritative?: boolean },
	) => Promise<void>;
	applyServerMessages: (
		rawMessages: RawMessage[] | unknown[],
		sessionId: string,
		serverVersion?: number,
		mode?: "authoritative" | "partial",
	) => void;
	persistedMessageVersionRef: MutableRefObject<number | null>;
	streamingMessageRef: MutableRefObject<DisplayMessage | null>;
	isStreamingRef: MutableRefObject<boolean>;
	sendInFlightRef: MutableRefObject<boolean>;
	setSharedWorkspaceSessionId: (
		sessionId: string | undefined,
		workspaceId: string,
	) => void;
	clearSharedWorkspaceSessionId: (sessionId: string | undefined) => void;
	recoverSessionOnError: (errMsg: string) => void;
};

export const dispatchResponseCommand = (ctx: ResponseDispatchContext): void => {
	const {
		resp,
		eventSessionId,
		activeSessionId,
		isDebug,
		turnKind,
		setState,
		bindSessionIdentity,
		applyTurnState,
		setBusyForEvent,
		setError,
		onError,
		fetchHistoryMessages,
		applyServerMessages,
		persistedMessageVersionRef,
		streamingMessageRef,
		isStreamingRef,
		sendInFlightRef,
		setSharedWorkspaceSessionId,
		clearSharedWorkspaceSessionId,
		recoverSessionOnError,
	} = ctx;

	switch (resp.cmd) {
		case "prompt":
		case "steer":
		case "follow_up": {
			if (!resp.success) {
				const errMsg = resp.error || `Failed to execute ${resp.cmd}`;
				applyTurnState({
					kind: "error",
					recoverable: true,
					message: errMsg,
				});
				setBusyForEvent(eventSessionId ?? activeSessionId, false);
				const err = new Error(errMsg);
				setError(err);
				onError?.(err);
				recoverSessionOnError(errMsg);
			}
			return;
		}

		case "session.create": {
			if (resp.success) {
				const { targetScope, targetWorkspaceId } = parseSessionCreateTarget(
					resp.data,
				);
				if (targetScope === "shared_workspace" && targetWorkspaceId) {
					setSharedWorkspaceSessionId(eventSessionId, targetWorkspaceId);
				} else if (targetScope === "personal") {
					clearSharedWorkspaceSessionId(eventSessionId);
				}

				if (
					shouldFetchHistoryAfterSessionCreate({
						hasStreamingMessage: Boolean(streamingMessageRef.current),
						isStreaming: isStreamingRef.current,
						sendInFlight: sendInFlightRef.current,
					}) &&
					eventSessionId
				) {
					void fetchHistoryMessages(eventSessionId);
					if (isDebug) {
						console.debug(
							"[useChat] Session created, fetching history:",
							eventSessionId,
						);
					}
				}
			} else {
				const err = new Error(resp.error || "Failed to create session");
				setError(err);
				onError?.(err);
			}
			return;
		}

		case "get_state": {
			if (resp.success && resp.data) {
				const nextState = resp.data as AgentState & { sessionId?: string };
				setState(nextState);
				bindSessionIdentity({
					runnerId: eventSessionId ?? activeSessionId ?? "",
					piId:
						typeof nextState.sessionId === "string"
							? nextState.sessionId
							: undefined,
				});

				if (nextState?.isStreaming === true) {
					if (!isStreamingRef.current) {
						applyTurnState({ kind: "streaming" });
						setBusyForEvent(eventSessionId ?? activeSessionId, true);
					}
				} else if (nextState?.isStreaming === false) {
					// get_state can transiently report non-streaming while a live
					// assistant bubble is still active. Let explicit stream.message_end /
					// agent.idle finalize the container; do not clear it here.
					if (!sendInFlightRef.current && !streamingMessageRef.current) {
						applyTurnState({ kind: "idle" });
					}
				}
			}
			return;
		}

		case "get_messages": {
			if (resp.success && resp.data) {
				const { messages, serverVersion } = parseGetMessagesPayload(resp.data);
				// Include the turn state: "syncing" means we're between
				// stream.message_end and agent.idle — refs are cleared but the
				// turn is not truly idle. Stale snapshots must not be applied.
				const liveTurnActive =
					isStreamingRef.current ||
					Boolean(streamingMessageRef.current) ||
					sendInFlightRef.current ||
					turnKind === "streaming" ||
					turnKind === "syncing" ||
					turnKind === "sending";
				if (Array.isArray(messages)) {
					// Structural authority rule: during an active live turn, streaming
					// events own the timeline. Snapshot get_messages payloads are
					// non-authoritative and can race/reorder; ignore them until idle.
					if (!liveTurnActive) {
						applyServerMessages(
							messages,
							eventSessionId ?? activeSessionId ?? "unknown",
							serverVersion,
							"partial",
						);
					}
					if (isDebug) {
						console.debug(
							"[useChat] Loaded messages:",
							eventSessionId,
							messages.length,
							serverVersion,
							"liveTurnActive=",
							liveTurnActive,
						);
					}
				} else if (typeof serverVersion === "number") {
					persistedMessageVersionRef.current = Math.max(
						persistedMessageVersionRef.current ?? 0,
						serverVersion,
					);
				}
			}
			return;
		}

		case "get_stats": {
			return;
		}

		default: {
			if (!resp.success && resp.error) {
				const err = new Error(resp.error);
				setError(err);
				onError?.(err);
				recoverSessionOnError(resp.error);
			}
		}
	}
};
