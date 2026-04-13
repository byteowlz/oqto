import type { CommandResponse } from "@/lib/canonical-types";
import type { AgentWsEvent } from "@/lib/ws-mux-types";

import type { PromptQueueItem } from "./types";

type QueueStatusPayload = {
	type?: string;
	clientId?: string;
	intent?: "default" | "steer" | "followUp";
	bridgeSeq?: number;
	ts?: number;
};

export const parseQueueStatusPayload = (
	event: AgentWsEvent,
): QueueStatusPayload | null => {
	if (event.key !== "oqto_queue_event") return null;
	const text = typeof event.text === "string" ? event.text : null;
	if (!text) return null;
	try {
		return JSON.parse(text) as QueueStatusPayload;
	} catch {
		return null;
	}
};

export const applyQueueStatusPayload = (
	previous: PromptQueueItem[],
	payload: QueueStatusPayload,
): PromptQueueItem[] => {
	const eventType = payload.type;
	if (!eventType) return previous;

	if (eventType === "queue_reset") {
		return [];
	}

	if (eventType === "enqueued") {
		const clientId = payload.clientId;
		if (!clientId) return previous;
		const exists = previous.some((item) =>
			payload.bridgeSeq != null
				? item.bridgeSeq === payload.bridgeSeq
				: item.clientId === clientId,
		);
		if (exists) return previous;
		return [
			...previous,
			{
				bridgeSeq:
					typeof payload.bridgeSeq === "number" ? payload.bridgeSeq : undefined,
				clientId,
				intent: payload.intent ?? "default",
				enqueuedAt: typeof payload.ts === "number" ? payload.ts : Date.now(),
			},
		];
	}

	if (eventType === "turn_bound") {
		return previous.filter((item) => {
			if (typeof payload.bridgeSeq === "number" && item.bridgeSeq != null) {
				return item.bridgeSeq !== payload.bridgeSeq;
			}
			if (payload.clientId) {
				return item.clientId !== payload.clientId;
			}
			return true;
		});
	}

	return previous;
};

export const parseCommandResponse = (
	event: AgentWsEvent,
): CommandResponse | undefined => {
	if (typeof event.cmd !== "string") return undefined;
	return {
		id: event.id as string,
		cmd: event.cmd as string,
		success: event.success as boolean,
		data: event.data as unknown,
		error: event.error as string | undefined,
	};
};

export const shouldAttemptSessionRecovery = ({
	errMsg,
	sessionId,
	wasInFlight,
}: {
	errMsg: string;
	sessionId: string | null | undefined;
	wasInFlight?: boolean;
}): boolean => {
	const isSessionNotFound =
		errMsg.includes("PiSessionNotFound") ||
		errMsg.includes("SessionNotFound") ||
		errMsg.includes("Response channel closed");
	if (!sessionId) return false;
	if (wasInFlight === false) return false;
	return isSessionNotFound;
};
