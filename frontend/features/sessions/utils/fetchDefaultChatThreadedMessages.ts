import type { ThreadedMessage } from "@/features/sessions/types";
import {
	convertChatMessagesToCanonical,
	getChatMessages,
	listDefaultChatPiSessions,
} from "@/lib/api";
import { formatSessionDate } from "@/lib/session-utils";

export async function fetchDefaultChatThreadedMessages(
	defaultChatAssistantName: string,
): Promise<ThreadedMessage[]> {
	const sessions = await listDefaultChatPiSessions();
	if (sessions.length === 0) return [];

	const sortedSessions = [...sessions].sort(
		(a, b) =>
			new Date(a.started_at).getTime() - new Date(b.started_at).getTime(),
	);

	const allMessages: ThreadedMessage[] = [];

	for (const session of sortedSessions) {
		try {
			const historyMessages = await getChatMessages(session.id);
			if (historyMessages.length === 0) continue;
			const converted = convertChatMessagesToCanonical(historyMessages);
			converted.forEach((msg, idx) => {
				const threadedMsg: ThreadedMessage = {
					...msg,
					_sessionId: session.id,
					_sessionTitle:
						session.title ||
						formatSessionDate(new Date(session.started_at).getTime()),
					_isSessionStart: idx === 0,
				};
				allMessages.push(threadedMsg);
			});
		} catch {
			// Ignore failures for individual sessions.
		}
	}

	return allMessages;
}
