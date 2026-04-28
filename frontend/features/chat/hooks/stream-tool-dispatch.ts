import type { AgentWsEvent } from "@/lib/ws-mux-types";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";

import {
	appendDeltaPart,
	replaceCompactionPlaceholder,
	upsertToolCallPart,
	upsertToolResultPart,
} from "./canonical-event-reducer";
import type { DisplayMessage, DisplayPart } from "./types";

type TurnState =
	| { kind: "idle" }
	| { kind: "streaming" }
	| { kind: "syncing" }
	| { kind: "sending" }
	| { kind: "error"; recoverable: boolean; message: string };

export type StreamToolDispatchContext = {
	event: AgentWsEvent;
	activeSessionId: string | null;
	isDebug: boolean;
	ensureAssistantMessage: (preferStreaming: boolean) => DisplayMessage;
	nextPartId: () => string;
	throttledStreamingUpdate: (snapshot: DisplayMessage) => void;
	scheduleStreamingUpdate: () => void;
	appendPartToMessage: (messageId: string, part: DisplayPart) => void;
	applyTurnState: (state: TurnState) => void;
	setMessages: Dispatch<SetStateAction<DisplayMessage[]>>;
	streamingMessageRef: MutableRefObject<DisplayMessage | null>;
	setBusyForEvent: (
		sessionId: string | null | undefined,
		busy: boolean,
	) => void;
	isStreamingRef: MutableRefObject<boolean>;
};

function upsertRetryErrorPart(
	message: DisplayMessage,
	nextPartId: () => string,
	text: string,
	retryAttempt?: number,
	retryMax?: number,
): void {
	const existing = message.parts.find(
		(part): part is Extract<DisplayPart, { type: "error" }> =>
			part.type === "error" && part.retrying === true,
	);
	if (existing) {
		existing.text = text;
		existing.retrying = true;
		existing.retryAttempt = retryAttempt;
		existing.retryMax = retryMax;
		return;
	}
	message.parts.push({
		type: "error",
		id: nextPartId(),
		text,
		retrying: true,
		retryAttempt,
		retryMax,
	});
}

function removeRetryErrorPart(message: DisplayMessage): void {
	message.parts = message.parts.filter(
		(part) => !(part.type === "error" && part.retrying === true),
	);
}

export const dispatchStreamToolEvent = ({
	event,
	activeSessionId,
	isDebug,
	ensureAssistantMessage,
	nextPartId,
	throttledStreamingUpdate,
	scheduleStreamingUpdate,
	appendPartToMessage,
	applyTurnState,
	setMessages,
	streamingMessageRef,
	setBusyForEvent,
	isStreamingRef,
}: StreamToolDispatchContext): boolean => {
	switch (event.event) {
		case "stream.text_delta": {
			const delta = event.delta as string | undefined;
			if (!delta) return true;
			if (isDebug) {
				console.debug(
					"[useChat] text_delta:",
					JSON.stringify(delta).slice(0, 60),
					"session:",
					event.session_id,
				);
			}
			const currentMsg = ensureAssistantMessage(true);
			appendDeltaPart({
				message: currentMsg,
				delta,
				partType: "text",
				nextPartId,
			});
			throttledStreamingUpdate(currentMsg);
			applyTurnState({ kind: "streaming" });
			return true;
		}

		case "stream.thinking_delta": {
			const delta = event.delta as string | undefined;
			if (!delta) return true;
			const currentMsg = ensureAssistantMessage(true);
			appendDeltaPart({
				message: currentMsg,
				delta,
				partType: "thinking",
				nextPartId,
			});
			throttledStreamingUpdate(currentMsg);
			applyTurnState({ kind: "streaming" });
			return true;
		}

		case "stream.tool_call_start": {
			const toolCallId =
				typeof event.tool_call_id === "string" ? event.tool_call_id : "";
			if (!toolCallId) return true;
			const targetMessage = ensureAssistantMessage(true);
			upsertToolCallPart({
				message: targetMessage,
				toolCallId,
				name: event.name as string,
				status: "running",
				nextPartId,
			});
			scheduleStreamingUpdate();
			applyTurnState({ kind: "streaming" });
			return true;
		}

		case "stream.tool_call_end": {
			const toolCall = event.tool_call as
				| { id: string; name: string; input: unknown }
				| undefined;
			if (!toolCall?.id) return true;
			const targetMessage = ensureAssistantMessage(true);
			upsertToolCallPart({
				message: targetMessage,
				toolCallId: toolCall.id,
				name: toolCall.name,
				input: toolCall.input,
				status: "running",
				nextPartId,
			});
			scheduleStreamingUpdate();
			return true;
		}

		case "tool.start": {
			const toolCallId =
				typeof event.tool_call_id === "string" ? event.tool_call_id : "";
			if (!toolCallId) return true;
			const targetMessage = ensureAssistantMessage(true);
			upsertToolCallPart({
				message: targetMessage,
				toolCallId,
				name: event.name as string,
				input: event.input,
				status: "running",
				nextPartId,
			});
			scheduleStreamingUpdate();
			applyTurnState({ kind: "streaming" });
			return true;
		}

		case "tool.end": {
			const toolCallId =
				typeof event.tool_call_id === "string" ? event.tool_call_id : "";
			if (!toolCallId) return true;
			const name = event.name as string;
			const output = event.output;
			const isError = event.is_error as boolean;
			const targetMessage = ensureAssistantMessage(false);
			const matchingToolCall = targetMessage.parts.find(
				(part) => part.type === "tool_call" && part.toolCallId === toolCallId,
			);
			if (matchingToolCall && matchingToolCall.type === "tool_call") {
				matchingToolCall.status = isError ? "error" : "success";
				matchingToolCall.name = name || matchingToolCall.name;
			}
			upsertToolResultPart({
				message: targetMessage,
				toolCallId,
				name:
					name ||
					(matchingToolCall?.type === "tool_call"
						? matchingToolCall.name
						: undefined),
				output,
				isError,
				nextPartId,
			});
			scheduleStreamingUpdate();
			applyTurnState({ kind: "streaming" });
			return true;
		}

		case "retry.start": {
			const currentMsg = ensureAssistantMessage(true);
			if (streamingMessageRef.current?.id !== currentMsg.id) {
				streamingMessageRef.current = {
					...currentMsg,
					isStreaming: true,
				};
			}

			const retryAttempt =
				typeof event.attempt === "number" ? event.attempt : undefined;
			const retryMax =
				typeof event.max_attempts === "number" ? event.max_attempts : undefined;
			const retryError =
				typeof event.error === "string" && event.error.trim().length > 0
					? event.error.trim()
					: "The model request failed";
			const retryText =
				retryAttempt && retryMax
					? `${retryError} — retrying (${retryAttempt}/${retryMax})…`
					: `${retryError} — retrying…`;
			upsertRetryErrorPart(
				currentMsg,
				nextPartId,
				retryText,
				retryAttempt,
				retryMax,
			);
			scheduleStreamingUpdate();

			setBusyForEvent(event.session_id ?? activeSessionId, true);
			isStreamingRef.current = true;
			applyTurnState({ kind: "streaming" });
			return true;
		}

		case "retry.end": {
			const retrySuccess = event.success as boolean;
			const currentMsg = ensureAssistantMessage(true);
			removeRetryErrorPart(currentMsg);
			scheduleStreamingUpdate();
			if (retrySuccess) {
				setBusyForEvent(event.session_id ?? activeSessionId, true);
				isStreamingRef.current = true;
				applyTurnState({ kind: "streaming" });
			}
			return true;
		}

		case "compact.start": {
			const currentMsg = ensureAssistantMessage(false);
			const part: DisplayPart = {
				type: "compaction",
				id: nextPartId(),
				text: "Compacting context...",
			};
			if (streamingMessageRef.current?.id === currentMsg.id) {
				currentMsg.parts.push(part);
				scheduleStreamingUpdate();
			} else {
				appendPartToMessage(currentMsg.id, part);
			}
			return true;
		}

		case "compact.end": {
			const success = event.success as boolean;
			const tokensBefore = event.tokens_before as number | undefined;
			const resultText = success
				? (() => {
						const parts: string[] = ["Context compacted"];
						if (tokensBefore) {
							const fmt = (n: number) =>
								n >= 1000 ? `${(n / 1000).toFixed(1)}K` : n.toString();
							parts[0] = `Context compacted (${fmt(tokensBefore)} tokens summarized)`;
						}
						return parts[0];
					})()
				: (event.error as string) || "Compaction failed";

			const currentMsg = ensureAssistantMessage(false);
			const part: DisplayPart = success
				? { type: "compaction", id: nextPartId(), text: resultText }
				: { type: "error", id: nextPartId(), text: resultText };

			if (streamingMessageRef.current?.id === currentMsg.id) {
				const replaced = replaceCompactionPlaceholder({
					message: currentMsg,
					replacement: part,
				});
				if (!replaced) {
					currentMsg.parts.push(part);
				}
				scheduleStreamingUpdate();
			} else {
				setMessages((prev) => {
					const msgIdx = prev.findIndex((m) => m.id === currentMsg.id);
					if (msgIdx < 0) return prev;
					const msg = { ...prev[msgIdx], parts: [...prev[msgIdx].parts] };
					const replaced = replaceCompactionPlaceholder({
						message: msg,
						replacement: part,
					});
					if (!replaced) return prev;
					const next = [...prev];
					next[msgIdx] = msg;
					return next;
				});
			}
			return true;
		}

		default:
			return false;
	}
};
