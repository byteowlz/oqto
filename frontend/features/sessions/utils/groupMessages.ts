import type { OpenCodeMessageWithParts } from "@/features/sessions/api";
import type { MessageGroup } from "@/features/sessions/types";

// Group consecutive messages from the same role (and session for main chat threads).
export function groupMessages(
	messages: OpenCodeMessageWithParts[],
): MessageGroup[] {
	const groups: MessageGroup[] = [];
	let currentGroup: MessageGroup | null = null;

	messages.forEach((msg, index) => {
		const role = msg.info.role as "user" | "assistant";

		const sessionId = (msg as { _sessionId?: string })._sessionId;
		const sessionTitle = (msg as { _sessionTitle?: string })._sessionTitle;
		const isNewSession = (msg as { _isSessionStart?: boolean })._isSessionStart;

		if (
			!currentGroup ||
			currentGroup.role !== role ||
			(currentGroup.sessionId && sessionId && currentGroup.sessionId !== sessionId)
		) {
			currentGroup = {
				role,
				messages: [msg],
				startIndex: index,
				sessionId,
				isNewSession,
				sessionTitle,
			};
			groups.push(currentGroup);
		} else {
			currentGroup.messages.push(msg);
		}
	});

	return groups;
}
