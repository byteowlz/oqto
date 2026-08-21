import type { MessagePage } from "../platform/contracts";
import type { OqtoUiPlatform, OqtoUiSnapshot } from "../platform/contracts";
import { scriptedFixture, scriptedTimeline } from "./fixture";

const SCRIPTED_PAGE_SIZE = 40;

export const scriptedOqtoUiPlatform: OqtoUiPlatform = {
	id: "scripted",
	async load(sessionId): Promise<OqtoUiSnapshot> {
		const known = scriptedFixture.workDirectories.some((directory) =>
			directory.sessions.some((session) => session.id === sessionId),
		);
		const activeSessionId = known
			? sessionId
			: (scriptedFixture.workDirectories[0]?.sessions[0]?.id ?? null);
		return { ...scriptedFixture, activeSessionId };
	},

	async loadMessages(sessionId, before, limit): Promise<MessagePage> {
		const size = Math.min(Math.max(limit ?? SCRIPTED_PAGE_SIZE, 1), 200);
		// The scripted session's timeline ends at the newest message; other
		// sessions have no durable history yet.
		const known = scriptedFixture.workDirectories.some((directory) =>
			directory.sessions.some((session) => session.id === sessionId),
		);
		if (!known) {
			return { sessionId, messages: [], hasMore: false, nextBefore: null };
		}
		// `before` is the id of the oldest message of the previous page; the
		// next page contains only messages strictly older than it.
		const endExclusive = before
			? scriptedTimeline.findIndex((message) => message.id === before)
			: scriptedTimeline.length;
		if (endExclusive <= 0) {
			return { sessionId, messages: [], hasMore: false, nextBefore: null };
		}
		const start = Math.max(0, endExclusive - size);
		const messages = scriptedTimeline.slice(start, endExclusive);
		return {
			sessionId,
			messages,
			hasMore: start > 0,
			nextBefore: start > 0 ? (messages[0]?.id ?? null) : null,
		};
	},
};
