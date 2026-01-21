import {
	convertChatMessagesToOpenCode,
	getChatMessages,
	listMainChatSessions,
} from "@/features/sessions/api";
import type { ThreadedMessage } from "@/features/sessions/types";
import { formatSessionDate } from "@/lib/session-utils";

export async function fetchMainChatThreadedMessages(
	mainChatAssistantName: string,
): Promise<ThreadedMessage[]> {
	const sessions = await listMainChatSessions(mainChatAssistantName);
	if (sessions.length === 0) return [];

	const sortedSessions = [...sessions].sort(
		(a, b) =>
			new Date(a.started_at).getTime() - new Date(b.started_at).getTime(),
	);

	const allMessages: ThreadedMessage[] = [];

	for (const session of sortedSessions) {
		try {
			const historyMessages = await getChatMessages(session.session_id);
			if (historyMessages.length === 0) continue;
			const converted = convertChatMessagesToOpenCode(historyMessages);
			converted.forEach((msg, idx) => {
				const threadedMsg: ThreadedMessage = {
					...msg,
					_sessionId: session.session_id,
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
