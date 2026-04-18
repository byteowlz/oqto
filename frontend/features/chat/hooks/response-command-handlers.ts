import type { RawMessage } from "./types";

export const parseSessionCreateTarget = (
	data: unknown,
): { targetScope?: string; targetWorkspaceId?: string | null } => {
	const targetData = (data ?? null) as {
		target_scope?: string;
		target_workspace_id?: string | null;
	} | null;
	return {
		targetScope: targetData?.target_scope,
		targetWorkspaceId: targetData?.target_workspace_id ?? null,
	};
};

export const shouldFetchHistoryAfterSessionCreate = ({
	hasStreamingMessage,
	isStreaming,
	sendInFlight,
}: {
	hasStreamingMessage: boolean;
	isStreaming: boolean;
	sendInFlight: boolean;
}): boolean => !hasStreamingMessage && !isStreaming && !sendInFlight;

export const parseGetMessagesPayload = (
	data: unknown,
): {
	messages: RawMessage[] | null;
	serverVersion?: number;
	messagesSource?: "authoritative" | "live";
} => {
	if (!data) {
		return { messages: null };
	}
	const payload = data as
		| RawMessage[]
		| {
				messages?: RawMessage[];
				message_version?: { version?: number };
				messages_source?: "authoritative" | "live";
		  };
	const messages = Array.isArray(payload)
		? payload
		: (payload.messages ?? null);
	const serverVersion =
		!Array.isArray(payload) &&
		typeof payload.message_version?.version === "number"
			? payload.message_version.version
			: undefined;
	const messagesSource =
		!Array.isArray(payload) &&
		(payload.messages_source === "authoritative" ||
			payload.messages_source === "live")
			? payload.messages_source
			: undefined;
	return { messages, serverVersion, messagesSource };
};
