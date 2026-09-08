/**
 * Stable Content identities the compositor shell places: the sessions
 * catalog, one Chat per Session, one Files view per work directory, and
 * the interface settings. Identity is the contract; placement is not.
 */

import { type ContentRef, contentIdFrom } from "../compositor/index";
import type { OqtoUiSnapshot } from "../platform/contracts";

function staticContent(identity: string, kind: string): ContentRef {
	return { id: contentIdFrom(identity), kind };
}

export const SESSIONS_CONTENT = staticContent("sessions:catalog", "sessions");
export const STATUS_CONTENT = staticContent("status:session", "status");
export const SETTINGS_CONTENT = staticContent("settings:ui", "settings");
export const TODOS_CONTENT = staticContent("todos:session", "todos");
export const TERMINAL_CONTENT = staticContent("terminal:workdir", "terminal");
export const GIT_CONTENT = staticContent("git:workdir", "git");
export const ISSUES_CONTENT = staticContent("issues:workdir", "issues");
export const GALLERY_CONTENT = staticContent("gallery:workdir", "gallery");

export function chatContent(sessionId: string): ContentRef {
	return {
		id: contentIdFrom(`chat:${sessionId}`),
		kind: "chat",
		extensions: { sessionId },
	};
}

export function fileContent(path: string): ContentRef {
	return {
		id: contentIdFrom(`file:${path}`),
		kind: "file",
		extensions: { path },
	};
}

export function filesContent(workDirectoryId: string): ContentRef {
	return {
		id: contentIdFrom(`files:${workDirectoryId}`),
		kind: "files",
		extensions: { workDirectoryId },
	};
}

export function findSession(snapshot: OqtoUiSnapshot, sessionId: string) {
	for (const directory of snapshot.workDirectories) {
		const session = directory.sessions.find(
			(candidate) => candidate.id === sessionId,
		);
		if (session) return { directory, session };
	}
	return null;
}
