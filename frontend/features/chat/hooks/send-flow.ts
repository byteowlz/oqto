import type { DisplayMessage, SendMode } from "./types";

export type PromptDispatcher = {
	agentPrompt: (
		sessionId: string,
		message: string,
		meta?: unknown,
		clientId?: string,
	) => void;
	agentSteer: (
		sessionId: string,
		message: string,
		meta?: unknown,
		clientId?: string,
	) => void;
	agentFollowUp: (
		sessionId: string,
		message: string,
		meta?: unknown,
		clientId?: string,
	) => void;
};

export const dispatchMessageByMode = ({
	dispatcher,
	mode,
	sessionId,
	message,
	clientId,
}: {
	dispatcher: PromptDispatcher;
	mode: SendMode;
	sessionId: string;
	message: string;
	clientId: string;
}): void => {
	switch (mode) {
		case "prompt":
			dispatcher.agentPrompt(sessionId, message, undefined, clientId);
			break;
		case "steer":
			dispatcher.agentSteer(sessionId, message, undefined, clientId);
			break;
		case "follow_up":
			dispatcher.agentFollowUp(sessionId, message, undefined, clientId);
			break;
	}
};

export const buildOptimisticUserMessage = ({
	id,
	partId,
	message,
	clientId,
	senderName,
	timestamp,
}: {
	id: string;
	partId: string;
	message: string;
	clientId: string;
	senderName?: string | null;
	timestamp: number;
}): DisplayMessage => ({
	id,
	role: "user",
	parts: [{ type: "text", id: partId, text: message }],
	timestamp,
	clientId,
	...(senderName
		? {
				sender: {
					type: "user" as const,
					id: senderName,
					name: senderName,
				},
			}
		: {}),
});
